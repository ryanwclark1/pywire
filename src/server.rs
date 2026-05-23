//! Server bootstrap: wires our Python handlers into pgwire's
//! `PgWireServerHandlers` trait and exposes `pywire.serve(...)`.
//!
//! What ships:
//!
//! - **Simple query**: `PyServerSimpleQueryHandler` implements pgwire's
//!   `SimpleQueryHandler::do_query` by delegating to our `PyQueryHandler`
//!   adapter (PR F). The Python handler returns `list[Response]`; we
//!   convert each entry to pgwire's `Response` and the server takes
//!   care of writing the frames.
//! - **Startup / auth**: `PyStartupHandler` dispatches between
//!   `NoopHandler` (no auth, every client trusted) and pgwire's
//!   `CleartextPasswordAuthStartupHandler` driven by a Python
//!   `AuthSource` subclass. Selected at `serve()` time by the
//!   `auth=...` keyword argument.
//! - **Extended query, COPY, cancel**: fall through to pgwire's
//!   `NoopHandler` defaults. v0.40.1+ will plumb the Python ABCs
//!   from PRs G and H through here.
//!
//! `pywire.serve(simple_query, addr, *, auth=None)` binds a TCP
//! listener and returns a Python awaitable that runs the accept
//! loop forever; cancel the `asyncio.Task` to stop.

use std::fmt::Debug;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use futures::sink::Sink;
use pgwire::api::auth::cleartext::CleartextPasswordAuthStartupHandler;
use pgwire::api::auth::{
    AuthSource as PgAuthSource, DefaultServerParameterProvider, LoginInfo, StartupHandler,
};
use pgwire::api::query::SimpleQueryHandler as PgSimpleQueryHandler;
use pgwire::api::results::Response;
use pgwire::api::{ClientInfo, ClientPortalStore, NoopHandler, PgWireServerHandlers};
use pgwire::error::{PgWireError, PgWireResult};
use pgwire::messages::{PgWireBackendMessage, PgWireFrontendMessage};
use pgwire::tokio::process_socket;
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio as pyo3_tokio;
use tokio::net::TcpListener;

use crate::auth::PyAuthSource;
use crate::errors::py_err_to_pywire;
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
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
    {
        let py_responses = self
            .inner
            .do_query(query)
            .await
            .map_err(|err| Python::attach(|py| py_err_to_pywire(py, err)))?;
        Ok(py_responses.into_iter().map(|r| r.into_pg()).collect())
    }
}

// ---------- Auth: PyAuthSource -> pgwire::AuthSource shim -------------

/// Adapter from `Arc<PyAuthSource>` to the `AuthSource` trait.
/// `CleartextPasswordAuthStartupHandler` takes its `AuthSource` by
/// value; we wrap the `Arc` so callers can keep the original handle
/// alive (the `serve()` accept loop also clones it per connection
/// later if we ever need to).
#[derive(Debug)]
struct PyAuthSourceWrapper(Arc<PyAuthSource>);

#[async_trait]
impl PgAuthSource for PyAuthSourceWrapper {
    async fn get_password(&self, login: &LoginInfo) -> PgWireResult<pgwire::api::auth::Password> {
        self.0.get_password(login).await
    }
}

// ---------- Startup handler: runtime dispatch between Noop + Cleartext ----

/// Top-level startup handler held inside `PyServerHandlers`. We can't
/// `dyn StartupHandler` because the trait's `on_startup` is generic
/// over the connection type, so instead we hand-roll the runtime
/// dispatch with a tagged enum and an explicit `StartupHandler` impl.
struct PyStartupHandler {
    inner: PyStartupInner,
}

enum PyStartupInner {
    Noop(NoopHandler),
    /// Boxed because pgwire's handler is ~256 bytes and the `Noop`
    /// variant is zero-sized — clippy's `large_enum_variant` triggers
    /// without the indirection.
    Cleartext(
        Box<
            CleartextPasswordAuthStartupHandler<
                PyAuthSourceWrapper,
                DefaultServerParameterProvider,
            >,
        >,
    ),
}

#[async_trait]
impl StartupHandler for PyStartupHandler {
    async fn on_startup<C>(
        &self,
        client: &mut C,
        message: PgWireFrontendMessage,
    ) -> PgWireResult<()>
    where
        C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        match &self.inner {
            PyStartupInner::Noop(h) => h.on_startup(client, message).await,
            PyStartupInner::Cleartext(h) => h.on_startup(client, message).await,
        }
    }
}

// ---------- PgWireServerHandlers --------------------------------------

#[derive(Clone)]
struct PyServerHandlers {
    simple_query: Arc<PyServerSimpleQueryHandler>,
    startup: Arc<PyStartupHandler>,
}

impl PgWireServerHandlers for PyServerHandlers {
    fn simple_query_handler(&self) -> Arc<impl PgSimpleQueryHandler> {
        self.simple_query.clone()
    }

    fn startup_handler(&self) -> Arc<impl StartupHandler> {
        self.startup.clone()
    }

    // Default extended_query_handler / copy_handler / error_handler /
    // cancel_handler from the trait blanket impl return pgwire's
    // `NoopHandler`. Wired in v0.40.1+.
}

// ---------- Python entry point ----------------------------------------

/// Bind a TCP listener on `addr` and run the pywire server.
///
/// `simple_query` is an instance of a `pywire.query.SimpleQueryHandler`
/// subclass. `auth` is an optional instance of a
/// `pywire.auth.AuthSource` subclass; when supplied, the server runs
/// PostgreSQL's cleartext-password authentication flow and calls the
/// subclass's `get_password` to look up the reference password.
/// When omitted, the server accepts every connection.
///
/// Returns a Python awaitable that runs the accept loop forever;
/// cancel via `asyncio.Task.cancel` to stop.
#[pyfunction]
#[pyo3(signature = (simple_query, addr, *, auth = None))]
fn serve<'py>(
    py: Python<'py>,
    simple_query: Bound<'py, PyAny>,
    addr: String,
    auth: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let socket_addr = SocketAddr::from_str(&addr).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("invalid address {addr:?}: {e}"))
    })?;

    let startup_inner = match auth {
        None => PyStartupInner::Noop(NoopHandler),
        Some(auth_obj) => {
            let py_auth = Arc::new(PyAuthSource::new(auth_obj.unbind()));
            PyStartupInner::Cleartext(Box::new(CleartextPasswordAuthStartupHandler::new(
                PyAuthSourceWrapper(py_auth),
                DefaultServerParameterProvider::default(),
            )))
        }
    };

    let handlers = PyServerHandlers {
        simple_query: Arc::new(PyServerSimpleQueryHandler {
            inner: Arc::new(PyQueryHandler::new(simple_query.unbind())),
        }),
        startup: Arc::new(PyStartupHandler {
            inner: startup_inner,
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
            // surface as a Python OSError. Marked LCOV_EXCL_LINE because
            // reliably triggering a mid-listen accept failure from a
            // test isn't worth the contortion.
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
