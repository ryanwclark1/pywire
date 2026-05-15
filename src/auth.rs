//! Python binding for `pgwire::api::auth` user-facing types.
//!
//! What ships in this PR:
//!
//! - `LoginInfo` — mirrors `pgwire::api::auth::LoginInfo`. Carries the
//!   `user`, `database`, and `host` extracted from the startup message.
//! - `Password` — mirrors `pgwire::api::auth::Password`. Holds the raw
//!   password bytes plus an optional salt for hashed schemes.
//! - `PyAuthSource` — an internal Rust adapter that wraps a Python
//!   `pywire.auth.AuthSource` subclass and implements pgwire's
//!   `AuthSource` trait by calling the user's `async def get_password`
//!   over the runtime bridge.
//!
//! The actual `StartupHandler` plumbing (cleartext / MD5 / SCRAM
//! handlers, the connection state machine) ships with `pywire.server`
//! (PR I), because pgwire's handlers are generic over the connection
//! type and can't be instantiated standalone.
//!
//! `_test_call_get_password` is a hidden pytest helper that exercises
//! the adapter end-to-end without needing a real server.

use async_trait::async_trait;
use pgwire::api::auth::{
    AuthSource as PgAuthSource, LoginInfo as PgLoginInfo, Password as PgPassword,
};
use pgwire::error::{PgWireError, PgWireResult};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3_async_runtimes::tokio as pyo3_tokio;

use crate::errors::pywire_to_py_err;

// ---------- LoginInfo --------------------------------------------------

/// User-facing login info: the user, database, and client host
/// extracted from the connection's startup parameters.
#[pyclass(
    name = "LoginInfo",
    module = "pywire.auth",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyLoginInfo {
    #[pyo3(get)]
    pub user: Option<String>,
    #[pyo3(get)]
    pub database: Option<String>,
    #[pyo3(get)]
    pub host: String,
}

#[pymethods]
impl PyLoginInfo {
    #[new]
    #[pyo3(signature = (*, user = None, database = None, host = String::from("127.0.0.1")))]
    fn new(user: Option<String>, database: Option<String>, host: String) -> Self {
        Self {
            user,
            database,
            host,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "LoginInfo(user={:?}, database={:?}, host={:?})",
            self.user, self.database, self.host
        )
    }
}

impl PyLoginInfo {
    /// Build a `PyLoginInfo` from pgwire's borrowed view.
    pub fn from_pg(info: &PgLoginInfo<'_>) -> Self {
        Self {
            user: info.user().map(str::to_owned),
            database: info.database().map(str::to_owned),
            host: info.host().to_owned(),
        }
    }
}

// ---------- Password ---------------------------------------------------

/// Password material returned by an `AuthSource`. `salt` is `None` for
/// cleartext auth and `Some` for salted/hashed schemes (e.g. MD5).
#[pyclass(name = "Password", module = "pywire.auth", frozen, eq, from_py_object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyPassword {
    password: Vec<u8>,
    salt: Option<Vec<u8>>,
}

#[pymethods]
impl PyPassword {
    #[new]
    #[pyo3(signature = (password, *, salt = None))]
    fn new(password: &[u8], salt: Option<&[u8]>) -> Self {
        Self {
            password: password.to_vec(),
            salt: salt.map(|s| s.to_vec()),
        }
    }

    #[getter]
    fn password<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.password)
    }

    #[getter]
    fn salt<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.salt.as_deref().map(|s| PyBytes::new(py, s))
    }

    fn __repr__(&self) -> String {
        let salted = if self.salt.is_some() {
            "salted"
        } else {
            "cleartext"
        };
        format!(
            "Password(<{} {}-byte password>)",
            salted,
            self.password.len()
        )
    }
}

impl From<PyPassword> for PgPassword {
    fn from(p: PyPassword) -> Self {
        PgPassword::new(p.salt, p.password)
    }
}

// ---------- Rust adapter: PyAuthSource → pgwire::AuthSource -----------

/// Wraps a Python object that exposes `async def get_password(login)` and
/// adapts it to pgwire's `AuthSource` trait. Future server code (PR I)
/// constructs one of these per connection so pgwire's startup handlers
/// can invoke the user's Python auth logic.
#[derive(Debug)]
pub struct PyAuthSource {
    instance: Py<PyAny>,
}

impl PyAuthSource {
    pub fn new(instance: Py<PyAny>) -> Self {
        Self { instance }
    }
}

#[async_trait]
impl PgAuthSource for PyAuthSource {
    async fn get_password(&self, login: &PgLoginInfo) -> PgWireResult<PgPassword> {
        // Build a Python LoginInfo and call the user's coroutine. We
        // need the GIL just long enough to invoke the method and turn
        // its coroutine into a Rust Future, then drop the GIL while we
        // await.
        let fut = Python::attach(|py| -> PyResult<_> {
            let py_login = PyLoginInfo::from_pg(login);
            let coro = self
                .instance
                .bind(py)
                .call_method1("get_password", (py_login,))?;
            pyo3_tokio::into_future(coro)
        })
        .map_err(py_err_to_pgwire)?;

        let result = fut.await.map_err(py_err_to_pgwire)?;

        Python::attach(|py| -> PyResult<PgPassword> {
            let pwd: PyPassword = result.bind(py).extract()?;
            Ok(pwd.into())
        })
        .map_err(py_err_to_pgwire)
    }
}

/// Translate a `PyErr` back into a `PgWireError`. We funnel everything
/// into `PgWireError::ApiError` because anything raised inside Python
/// auth code is, by definition, a user-supplied error.
fn py_err_to_pgwire(err: PyErr) -> PgWireError {
    PgWireError::ApiError(Box::new(std::io::Error::other(err.to_string())))
}

// ---------- test helper exposed to pytest -----------------------------

/// Call `auth_source.get_password(LoginInfo(...))` via the same adapter
/// that future server work will use. Returns the resulting password
/// bytes (no salt) for the test to inspect.
#[pyfunction]
fn _test_call_get_password<'py>(
    py: Python<'py>,
    auth_source: Bound<'py, PyAny>,
    user: Option<String>,
    database: Option<String>,
    host: String,
) -> PyResult<Bound<'py, PyAny>> {
    let wrapper = PyAuthSource::new(auth_source.unbind());
    pyo3_tokio::future_into_py(py, async move {
        let host_clone = host.clone();
        let login = PgLoginInfo::new(user.as_deref(), database.as_deref(), host_clone);
        let pg_pass = wrapper
            .get_password(&login)
            .await
            .map_err(pywire_to_py_err)?;
        Python::attach(|py| -> PyResult<Py<PyAny>> {
            let salt_bytes: Option<Vec<u8>> = pg_pass.salt().map(|s| s.to_vec());
            let pwd = PyPassword {
                password: pg_pass.password().to_vec(),
                salt: salt_bytes,
            };
            Ok(pwd.into_pyobject(py)?.unbind().into_any())
        })
    })
}

// ---------- module registration ---------------------------------------

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let m = PyModule::new(py, "auth")?;
    m.add_class::<PyLoginInfo>()?;
    m.add_class::<PyPassword>()?;
    m.add_function(wrap_pyfunction!(_test_call_get_password, &m)?)?;
    parent.add_submodule(&m)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pywire._pywire.auth", &m)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_round_trips_through_pgwire() {
        let p = PyPassword {
            password: b"hunter2".to_vec(),
            salt: Some(vec![1, 2, 3, 4]),
        };
        let pg: PgPassword = p.into();
        assert_eq!(pg.password(), b"hunter2");
        assert_eq!(pg.salt(), Some(&[1u8, 2, 3, 4][..]));
    }

    #[test]
    fn login_info_from_pg_copies_fields() {
        let pg = PgLoginInfo::new(Some("alice"), Some("postgres"), "10.0.0.1".to_owned());
        let info = PyLoginInfo::from_pg(&pg);
        assert_eq!(info.user.as_deref(), Some("alice"));
        assert_eq!(info.database.as_deref(), Some("postgres"));
        assert_eq!(info.host, "10.0.0.1");
    }
}
