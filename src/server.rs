//! Server bootstrap: wires our Python handlers into pgwire's
//! `PgWireServerHandlers` trait and exposes `pywire.serve(...)`.
//!
//! What ships in this first server PR:
//!
//! - **Simple query**: `PyServerSimpleQueryHandler` implements pgwire's
//!   `SimpleQueryHandler::do_query` by delegating to our `PyQueryHandler`
//!   adapter (PR F). The Python handler returns `list[Response]`; we
//!   convert each entry to pgwire's `Response` and the server takes
//!   care of writing the frames.
//! - **Startup**: pgwire's `NoopStartupHandler` (no authentication).
//!   Cleartext / MD5 / SCRAM startup handlers ship in a follow-up;
//!   pgwire's trait is generic over the connection type so wiring them
//!   alongside a "no auth" selector at runtime needs more glue. Today
//!   a pywire server trusts every client.
//! - **Extended query, COPY, cancel**: fall through to pgwire's
//!   `NoopHandler` defaults. A client that tries `parse`/`bind`/`COPY`
//!   against this server will get a protocol error. The Python ABCs
//!   from PRs G and H are in place; v0.40.1+ will plumb them through
//!   here.
//!
//! `pywire.serve(simple_query, addr)` binds a TCP listener and returns
//! a Python awaitable that runs the accept loop forever; cancel the
//! `asyncio.Task` to stop.

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use pgwire::api::query::SimpleQueryHandler as PgSimpleQueryHandler;
use pgwire::api::results::Response;
use pgwire::api::{ClientInfo, ClientPortalStore, PgWireServerHandlers};
use pgwire::error::PgWireResult;
use pgwire::messages::PgWireBackendMessage;
use pgwire::tokio::process_socket;
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio as pyo3_tokio;
use tokio::net::TcpListener;

use crate::query::PyQueryHandler;

// ---------- SimpleQueryHandler adapter --------------------------------

/// Implements pgwire's `SimpleQueryHandler` trait by delegating to the
/// Python `PyQueryHandler`.
struct PyServerSimpleQueryHandler {
    inner: Arc<PyQueryHandler>,
}

#[async_trait]
impl PgSimpleQueryHandler for PyServerSimpleQueryHandler {
    async fn do_query<C>(&self, _client: &mut C, query: &str) -> PgWireResult<Vec<Response>>
    where
        C: ClientInfo
            + ClientPortalStore
            + futures::sink::Sink<PgWireBackendMessage>
            + Unpin
            + Send
            + Sync,
    {
        let py_responses = self.inner.do_query(query).await.map_err(|err| {
            pgwire::error::PgWireError::ApiError(Box::new(std::io::Error::other(err.to_string())))
        })?;
        Ok(py_responses.into_iter().map(|r| r.into_pg()).collect())
    }
}

// ---------- PgWireServerHandlers --------------------------------------

#[derive(Clone)]
struct PyServerHandlers {
    simple_query: Arc<PyServerSimpleQueryHandler>,
}

impl PgWireServerHandlers for PyServerHandlers {
    fn simple_query_handler(&self) -> Arc<impl PgSimpleQueryHandler> {
        self.simple_query.clone()
    }

    // Default startup_handler / extended_query_handler / copy_handler /
    // error_handler / cancel_handler from the trait blanket impl return
    // pgwire's `NoopHandler`. That's exactly what we want at this stage.
}

// ---------- Python entry point ----------------------------------------

/// Bind a TCP listener on `addr` and run the pywire server, dispatching
/// each accepted connection to pgwire's `process_socket`. Returns a
/// Python awaitable that runs the accept loop forever; cancel it via
/// `asyncio.Task.cancel` to stop.
///
/// `simple_query` is an instance of a `pywire.query.SimpleQueryHandler`
/// subclass.
#[pyfunction]
fn serve<'py>(
    py: Python<'py>,
    simple_query: Bound<'py, PyAny>,
    addr: String,
) -> PyResult<Bound<'py, PyAny>> {
    let socket_addr = SocketAddr::from_str(&addr).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("invalid address {addr:?}: {e}"))
    })?;

    let handlers = PyServerHandlers {
        simple_query: Arc::new(PyServerSimpleQueryHandler {
            inner: Arc::new(PyQueryHandler::new(simple_query.unbind())),
        }),
    };

    // Capture the caller's asyncio task locals (event loop) so spawned
    // per-connection tasks can re-enter Python and `await` user
    // coroutines without "no running event loop" panics.
    let task_locals = pyo3_tokio::get_current_locals(py)?;

    pyo3_tokio::future_into_py(py, async move {
        let listener = TcpListener::bind(socket_addr).await.map_err(|e| {
            pyo3::exceptions::PyOSError::new_err(format!("bind {socket_addr} failed: {e}"))
        })?;
        loop {
            // Defensive: accept-loop IO failures (e.g. fd exhaustion)
            // surface as a Python OSError. Marked LCOV_EXCL_LINE on the
            // actual error-emitting lines because reliably triggering a
            // mid-listen accept failure from a test isn't worth the
            // contortion.
            let (sock, _peer) = listener
                .accept()
                .await
                .map_err(|e| pyo3::exceptions::PyOSError::new_err(format!("accept failed: {e}")))?; // LCOV_EXCL_LINE
            let handlers = handlers.clone();
            let locals = task_locals.clone();
            tokio::spawn(async move {
                let _ =
                    pyo3_tokio::scope(
                        locals,
                        async move { process_socket(sock, None, handlers).await },
                    )
                    .await;
            });
        }
        // Unreachable; the loop body never breaks. The type annotation
        // tells the compiler what the future's Output is.
        #[allow(unreachable_code)]
        Ok::<(), PyErr>(())
    })
}

/// Bind and immediately release a TCP listener on an ephemeral port.
/// Returns the chosen port. Used by integration-test fixtures to learn
/// a free port before starting `serve()` on it.
#[pyfunction]
fn _test_bind_ephemeral(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    pyo3_tokio::future_into_py(py, async move {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| pyo3::exceptions::PyOSError::new_err(format!("bind failed: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?
            .port();
        Ok(port)
    })
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_function(wrap_pyfunction!(serve, parent)?)?;
    parent.add_function(wrap_pyfunction!(_test_bind_ephemeral, parent)?)?;
    Ok(())
}
