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
//! - **Startup / auth**: trust, cleartext, and SCRAM-SHA-256, with
//!   optional TLS and SCRAM channel binding.
//! - **Extended query**: Parse/Bind/Describe/Execute route to the
//!   Python `ExtendedQueryHandler`; pgwire owns portal suspension.
//! - **Cancel**: pgwire's connection manager routes PostgreSQL cancel
//!   requests to the active Python query future.
//!
//! `pywire.serve(simple_query, addr, *, auth=None)` binds a TCP
//! listener and returns a Python awaitable that runs the accept
//! loop forever; cancel the `asyncio.Task` to stop.

use std::fmt::Debug;
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use futures::sink::Sink;
use pgwire::api::auth::cleartext::CleartextPasswordAuthStartupHandler;
use pgwire::api::auth::noop::NoopStartupHandler;
use pgwire::api::auth::sasl::scram::ScramAuth;
use pgwire::api::auth::sasl::SASLAuthStartupHandler;
use pgwire::api::auth::{
    AuthSource as PgAuthSource, DefaultServerParameterProvider, LoginInfo, StartupHandler,
};
use pgwire::api::cancel::{CancelHandler, DefaultCancelHandler};
use pgwire::api::query::{
    ExtendedQueryHandler as PgExtendedQueryHandler, SimpleQueryHandler as PgSimpleQueryHandler,
};
use pgwire::api::results::Response;
use pgwire::api::{ClientInfo, ClientPortalStore, ConnectionManager, PgWireServerHandlers};
use pgwire::error::{PgWireError, PgWireResult};
use pgwire::messages::{PgWireBackendMessage, PgWireFrontendMessage};
use pgwire::tokio::process_socket;
use pgwire::tokio::tokio_rustls::rustls::ServerConfig;
use pgwire::tokio::TlsAcceptor;
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio as pyo3_tokio;
use tokio::net::TcpListener;

use crate::auth::PyAuthSource;
use crate::errors::py_err_to_pywire;
use crate::extended::PyExtendedHandler;
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
    require_tls: bool,
}

enum PyStartupInner {
    Noop(ManagedNoopStartup),
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
    Scram(Box<SASLAuthStartupHandler<DefaultServerParameterProvider>>),
}

struct ManagedNoopStartup {
    manager: Arc<ConnectionManager>,
}

#[async_trait]
impl NoopStartupHandler for ManagedNoopStartup {
    fn connection_manager(&self) -> Option<Arc<ConnectionManager>> {
        Some(self.manager.clone())
    }
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
        if self.require_tls
            && matches!(message, PgWireFrontendMessage::Startup(_))
            && !client.is_secure()
        {
            return Err(PgWireError::UserError(Box::new(
                pgwire::error::ErrorInfo::new(
                    "FATAL".to_owned(),
                    "28000".to_owned(),
                    "TLS is required".to_owned(),
                ),
            )));
        }
        match &self.inner {
            PyStartupInner::Noop(h) => h.on_startup(client, message).await,
            PyStartupInner::Cleartext(h) => h.on_startup(client, message).await,
            PyStartupInner::Scram(h) => h.on_startup(client, message).await,
        }
    }
}

// ---------- PgWireServerHandlers --------------------------------------

#[derive(Clone)]
struct PyServerHandlers {
    simple_query: Arc<PyServerSimpleQueryHandler>,
    startup: Arc<PyStartupHandler>,
    extended_query: Arc<PyExtendedHandler>,
    cancel: Arc<DefaultCancelHandler>,
}

impl PgWireServerHandlers for PyServerHandlers {
    fn simple_query_handler(&self) -> Arc<impl PgSimpleQueryHandler> {
        self.simple_query.clone()
    }

    fn startup_handler(&self) -> Arc<impl StartupHandler> {
        self.startup.clone()
    }

    fn extended_query_handler(&self) -> Arc<impl PgExtendedQueryHandler> {
        self.extended_query.clone()
    }

    fn cancel_handler(&self) -> Arc<impl CancelHandler> {
        self.cancel.clone()
    }

    // COPY and custom error handling retain pgwire's defaults.
}

#[derive(Clone, Copy)]
enum AuthMethod {
    Trust,
    Cleartext,
    Scram,
}

struct HandlerConfig {
    simple_query: Arc<PyServerSimpleQueryHandler>,
    extended_query: Arc<PyExtendedHandler>,
    auth: Option<Arc<PyAuthSource>>,
    auth_method: AuthMethod,
    manager: Arc<ConnectionManager>,
    require_tls: bool,
    scram_iterations: usize,
    tls_certificate_pem: Option<Arc<Vec<u8>>>,
}

impl HandlerConfig {
    fn build(&self) -> PgWireResult<PyServerHandlers> {
        let startup = match self.auth_method {
            AuthMethod::Trust => PyStartupInner::Noop(ManagedNoopStartup {
                manager: self.manager.clone(),
            }),
            AuthMethod::Cleartext => {
                let auth = self.auth.as_ref().expect("validated auth configuration");
                PyStartupInner::Cleartext(Box::new(
                    CleartextPasswordAuthStartupHandler::new(
                        PyAuthSourceWrapper(auth.clone()),
                        DefaultServerParameterProvider::default(),
                    )
                    .with_connection_manager(self.manager.clone()),
                ))
            }
            AuthMethod::Scram => {
                let auth = self.auth.as_ref().expect("validated auth configuration");
                let mut scram = ScramAuth::new(Arc::new(PyAuthSourceWrapper(auth.clone())));
                scram.set_iterations(self.scram_iterations);
                if let Some(certificate) = &self.tls_certificate_pem {
                    scram.configure_certificate(certificate)?;
                } // LCOV_EXCL_LINE - closing region emitted separately by llvm-cov
                PyStartupInner::Scram(Box::new(
                    SASLAuthStartupHandler::new(
                        Arc::new(DefaultServerParameterProvider::default()),
                    )
                    .with_scram(scram)
                    .with_connection_manager(self.manager.clone()),
                ))
            }
        };
        Ok(PyServerHandlers {
            simple_query: self.simple_query.clone(),
            startup: Arc::new(PyStartupHandler {
                inner: startup,
                require_tls: self.require_tls,
            }),
            extended_query: self.extended_query.clone(),
            cancel: Arc::new(DefaultCancelHandler::new(self.manager.clone())),
        })
    }
}

fn load_tls(cert_path: &str, key_path: &str) -> PyResult<(TlsAcceptor, Arc<Vec<u8>>)> {
    let certificate_pem = std::fs::read(cert_path).map_err(|error| {
        pyo3::exceptions::PyOSError::new_err(format!("read TLS certificate {cert_path:?}: {error}"))
    })?;
    let certificates = rustls_pemfile::certs(&mut Cursor::new(&certificate_pem))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
    let key = rustls_pemfile::private_key(&mut BufReader::new(File::open(key_path).map_err(
        |error| pyo3::exceptions::PyOSError::new_err(format!("open TLS key {key_path:?}: {error}")),
    )?))
    .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?
    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("TLS key contains no private key"))?;
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
    config.alpn_protocols = vec![b"postgresql".to_vec()];
    Ok((
        TlsAcceptor::from(Arc::new(config)),
        Arc::new(certificate_pem),
    ))
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
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (simple_query, addr, *, auth = None, extended = None, auth_method = "cleartext", tls_cert = None, tls_key = None, require_tls = false, scram_iterations = 4096))]
fn serve<'py>(
    py: Python<'py>,
    simple_query: Bound<'py, PyAny>,
    addr: String,
    auth: Option<Bound<'py, PyAny>>,
    extended: Option<Bound<'py, PyAny>>,
    auth_method: &str,
    tls_cert: Option<String>,
    tls_key: Option<String>,
    require_tls: bool,
    scram_iterations: usize,
) -> PyResult<Bound<'py, PyAny>> {
    let socket_addr = SocketAddr::from_str(&addr).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("invalid address {addr:?}: {e}"))
    })?;

    let auth_method = match (auth.is_some(), auth_method) {
        (false, "cleartext" | "trust") => AuthMethod::Trust,
        (true, "cleartext") => AuthMethod::Cleartext,
        (true, "scram-sha-256" | "scram") => AuthMethod::Scram,
        (true, "trust") => AuthMethod::Trust,
        (false, "scram-sha-256" | "scram") => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "SCRAM authentication requires auth=AuthSource",
            ));
        }
        (_, value) => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "unsupported auth_method {value:?}"
            )));
        }
    };
    if scram_iterations < 4096 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "scram_iterations must be at least 4096",
        ));
    }
    let (tls_acceptor, tls_certificate_pem) = match (tls_cert, tls_key) {
        (Some(cert), Some(key)) => {
            let (acceptor, pem) = load_tls(&cert, &key)?;
            (Some(acceptor), Some(pem))
        }
        (None, None) if !require_tls => (None, None),
        (None, None) => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "require_tls=True requires tls_cert and tls_key",
            ));
        }
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "tls_cert and tls_key must be supplied together",
            ));
        }
    };
    let simple = Arc::new(PyQueryHandler::new(simple_query.unbind()));
    let manager = Arc::new(ConnectionManager::new());
    let config = Arc::new(HandlerConfig {
        simple_query: Arc::new(PyServerSimpleQueryHandler {
            inner: simple.clone(),
        }),
        extended_query: Arc::new(PyExtendedHandler::new(extended.map(Bound::unbind))),
        auth: auth.map(|value| Arc::new(PyAuthSource::new(value.unbind()))),
        auth_method,
        manager,
        require_tls,
        scram_iterations,
        tls_certificate_pem,
    });

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
            let handlers = config
                .build()
                .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))?;
            let handlers = Arc::new(handlers);
            let tls_acceptor = tls_acceptor.clone();
            let locals = task_locals.clone();
            tokio::spawn(async move {
                let _ = pyo3_tokio::scope(locals, async move {
                    process_socket(sock, tls_acceptor, handlers).await
                })
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
