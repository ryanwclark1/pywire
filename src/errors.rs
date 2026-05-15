//! Python binding for `pgwire::error::PgWireError` and `pgwire::error::ErrorInfo`.
//!
//! Each `PgWireError` variant maps to a dedicated Python exception class.
//! Concrete classes inherit from one of two mid-level umbrellas
//! (`ProtocolError`, `AuthError`) when they fit naturally, or from the root
//! `Error` otherwise. `IoError` is flattened to Python's built-in
//! `OSError` because that matches what callers will already be catching
//! around socket code.
//!
//! `pywire_to_py_err` translates a `PgWireError` to a `PyErr` at the
//! binding boundary; future bindings (auth/query/...) call it from the
//! `?` of their `PgWireResult<T>` shims.
//!
//! `_test_raise_for` is a hidden test helper that exercises every variant
//! of the mapping. It is registered as `pywire._pywire.errors._test_raise_for`
//! and is intended only for pytest.

use std::io;

use pgwire::error::{ErrorInfo, PgWireError};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyOSError};
use pyo3::prelude::*;

// ---------- exception types -----------------------------------------------

create_exception!(
    pywire.errors,
    Error,
    PyException,
    "Base class for every pywire-defined exception."
);

create_exception!(
    pywire.errors,
    ProtocolError,
    Error,
    "Umbrella for wire-protocol-level violations: malformed frames, \
     unsupported version negotiation, illegal message tags, and so on."
);

create_exception!(
    pywire.errors,
    AuthError,
    Error,
    "Umbrella for authentication-related errors, including SASL/SCRAM \
     and OAuth flows."
);

// Protocol-level concrete classes.
create_exception!(pywire.errors, UnsupportedProtocolVersion, ProtocolError, "");
create_exception!(pywire.errors, InvalidCancelRequest, ProtocolError, "");
create_exception!(pywire.errors, InvalidMessageType, ProtocolError, "");
create_exception!(pywire.errors, MessageTooLarge, ProtocolError, "");
create_exception!(pywire.errors, InvalidTargetType, ProtocolError, "");
create_exception!(pywire.errors, InvalidTransactionStatus, ProtocolError, "");
create_exception!(pywire.errors, InvalidSSLRequestMessage, ProtocolError, "");
create_exception!(
    pywire.errors,
    InvalidGssEncRequestMessage,
    ProtocolError,
    ""
);
create_exception!(pywire.errors, InvalidStartupMessage, ProtocolError, "");
create_exception!(
    pywire.errors,
    InvalidAuthenticationMessageCode,
    ProtocolError,
    ""
);
create_exception!(pywire.errors, InvalidSecretKey, ProtocolError, "");
create_exception!(pywire.errors, NotReadyForQuery, ProtocolError, "");
create_exception!(pywire.errors, InvalidOptionValue, ProtocolError, "");

// Auth-related concrete classes.
create_exception!(pywire.errors, FailedToCoercePasswordMessage, AuthError, "");
create_exception!(pywire.errors, InvalidSASLState, AuthError, "");
create_exception!(pywire.errors, UnsupportedSASLAuthMethod, AuthError, "");
create_exception!(pywire.errors, InvalidScramMessage, AuthError, "");
create_exception!(pywire.errors, InvalidPassword, AuthError, "");
create_exception!(
    pywire.errors,
    UnsupportedCertificateSignatureAlgorithm,
    AuthError,
    ""
);
create_exception!(pywire.errors, UserNameRequired, AuthError, "");
create_exception!(pywire.errors, InvalidOauthMessage, AuthError, "");
create_exception!(pywire.errors, OAuthAuthenticationFailed, AuthError, "");
create_exception!(pywire.errors, OAuthValidationError, AuthError, "");
create_exception!(pywire.errors, OauthAuthzIdError, AuthError, "");

// Other concrete classes (no umbrella).
create_exception!(pywire.errors, PortalNotFound, Error, "");
create_exception!(pywire.errors, PortalNotStarted, Error, "");
create_exception!(pywire.errors, StatementNotFound, Error, "");
create_exception!(pywire.errors, ParameterIndexOutOfBound, Error, "");
create_exception!(pywire.errors, InvalidRustTypeForParameter, Error, "");
create_exception!(pywire.errors, FailedToParseParameter, Error, "");
create_exception!(pywire.errors, QueryCanceled, Error, "");
create_exception!(pywire.errors, ApiError, Error, "");
create_exception!(pywire.errors, UserError, Error, "");

// ---------- ErrorInfo pyclass ---------------------------------------------

/// A PostgreSQL-protocol ErrorResponse / NoticeResponse field set.
///
/// Mirrors `pgwire::error::ErrorInfo`. Field names match the upstream
/// struct so that the
/// [PostgreSQL protocol error-fields reference](https://www.postgresql.org/docs/18/protocol-error-fields.html)
/// reads as a straight key-by-key index.
#[pyclass(
    name = "ErrorInfo",
    module = "pywire.errors",
    frozen,
    skip_from_py_object
)]
#[derive(Clone, Debug)]
pub struct PyErrorInfo {
    #[pyo3(get)]
    pub severity: String,
    #[pyo3(get)]
    pub code: String,
    #[pyo3(get)]
    pub message: String,
    #[pyo3(get)]
    pub detail: Option<String>,
    #[pyo3(get)]
    pub hint: Option<String>,
    #[pyo3(get)]
    pub position: Option<String>,
    #[pyo3(get)]
    pub internal_position: Option<String>,
    #[pyo3(get)]
    pub internal_query: Option<String>,
    #[pyo3(get)]
    pub where_context: Option<String>,
    #[pyo3(get)]
    pub file_name: Option<String>,
    #[pyo3(get)]
    pub line: Option<usize>,
    #[pyo3(get)]
    pub routine: Option<String>,
    #[pyo3(get)]
    pub severity_nonlocalized: Option<String>,
    #[pyo3(get)]
    pub schema: Option<String>,
    #[pyo3(get)]
    pub table: Option<String>,
    #[pyo3(get)]
    pub column: Option<String>,
    #[pyo3(get)]
    pub datatype: Option<String>,
    #[pyo3(get)]
    pub constraint: Option<String>,
}

#[pymethods]
impl PyErrorInfo {
    #[new]
    #[pyo3(signature = (
        severity,
        code,
        message,
        *,
        detail = None,
        hint = None,
        position = None,
        internal_position = None,
        internal_query = None,
        where_context = None,
        file_name = None,
        line = None,
        routine = None,
        severity_nonlocalized = None,
        schema = None,
        table = None,
        column = None,
        datatype = None,
        constraint = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        severity: String,
        code: String,
        message: String,
        detail: Option<String>,
        hint: Option<String>,
        position: Option<String>,
        internal_position: Option<String>,
        internal_query: Option<String>,
        where_context: Option<String>,
        file_name: Option<String>,
        line: Option<usize>,
        routine: Option<String>,
        severity_nonlocalized: Option<String>,
        schema: Option<String>,
        table: Option<String>,
        column: Option<String>,
        datatype: Option<String>,
        constraint: Option<String>,
    ) -> Self {
        Self {
            severity,
            code,
            message,
            detail,
            hint,
            position,
            internal_position,
            internal_query,
            where_context,
            file_name,
            line,
            routine,
            severity_nonlocalized,
            schema,
            table,
            column,
            datatype,
            constraint,
        }
    }

    /// True when `severity == "FATAL"`. Matches pgwire's `ErrorInfo::is_fatal`.
    fn is_fatal(&self) -> bool {
        self.severity == "FATAL"
    }

    fn __repr__(&self) -> String {
        format!(
            "ErrorInfo(severity={:?}, code={:?}, message={:?})",
            self.severity, self.code, self.message
        )
    }
}

impl From<ErrorInfo> for PyErrorInfo {
    fn from(e: ErrorInfo) -> Self {
        Self {
            severity: e.severity,
            code: e.code,
            message: e.message,
            detail: e.detail,
            hint: e.hint,
            position: e.position,
            internal_position: e.internal_position,
            internal_query: e.internal_query,
            where_context: e.where_context,
            file_name: e.file_name,
            line: e.line,
            routine: e.routine,
            severity_nonlocalized: e.severity_nonlocalized,
            schema: e.schema,
            table: e.table,
            column: e.column,
            datatype: e.datatype,
            constraint: e.constraint,
        }
    }
}

impl From<PyErrorInfo> for ErrorInfo {
    fn from(e: PyErrorInfo) -> Self {
        // ErrorInfo is `#[non_exhaustive]` in pgwire, so we go through the
        // `new` constructor and then fill the optional fields by name.
        let mut info = ErrorInfo::new(e.severity, e.code, e.message);
        info.detail = e.detail;
        info.hint = e.hint;
        info.position = e.position;
        info.internal_position = e.internal_position;
        info.internal_query = e.internal_query;
        info.where_context = e.where_context;
        info.file_name = e.file_name;
        info.line = e.line;
        info.routine = e.routine;
        info.severity_nonlocalized = e.severity_nonlocalized;
        info.schema = e.schema;
        info.table = e.table;
        info.column = e.column;
        info.datatype = e.datatype;
        info.constraint = e.constraint;
        info
    }
}

// ---------- boundary: PgWireError -> PyErr --------------------------------

/// Translate a `PgWireError` to the matching Python exception. Every other
/// pywire binding calls this from its `?` shim.
pub fn pywire_to_py_err(err: PgWireError) -> PyErr {
    let msg = err.to_string();
    match err {
        PgWireError::UnsupportedProtocolVersion(_, _) => UnsupportedProtocolVersion::new_err(msg),
        PgWireError::InvalidCancelRequest => InvalidCancelRequest::new_err(msg),
        PgWireError::InvalidSecretKey => InvalidSecretKey::new_err(msg),
        PgWireError::InvalidMessageType(_) => InvalidMessageType::new_err(msg),
        PgWireError::MessageTooLarge(_, _) => MessageTooLarge::new_err(msg),
        PgWireError::InvalidTargetType(_) => InvalidTargetType::new_err(msg),
        PgWireError::InvalidTransactionStatus(_) => InvalidTransactionStatus::new_err(msg),
        PgWireError::InvalidSSLRequestMessage => InvalidSSLRequestMessage::new_err(msg),
        PgWireError::InvalidGssEncRequestMessage => InvalidGssEncRequestMessage::new_err(msg),
        PgWireError::InvalidStartupMessage => InvalidStartupMessage::new_err(msg),
        PgWireError::InvalidAuthenticationMessageCode(_) => {
            InvalidAuthenticationMessageCode::new_err(msg)
        }
        PgWireError::FailedToCoercePasswordMessage => FailedToCoercePasswordMessage::new_err(msg),
        PgWireError::InvalidSASLState => InvalidSASLState::new_err(msg),
        PgWireError::UnsupportedSASLAuthMethod(_) => UnsupportedSASLAuthMethod::new_err(msg),
        PgWireError::IoError(io) => PyOSError::new_err(io.to_string()),
        PgWireError::PortalNotFound(_) => PortalNotFound::new_err(msg),
        PgWireError::PortalNotStarted => PortalNotStarted::new_err(msg),
        PgWireError::StatementNotFound(_) => StatementNotFound::new_err(msg),
        PgWireError::ParameterIndexOutOfBound(_) => ParameterIndexOutOfBound::new_err(msg),
        PgWireError::InvalidRustTypeForParameter(_) => InvalidRustTypeForParameter::new_err(msg),
        PgWireError::FailedToParseParameter(_) => FailedToParseParameter::new_err(msg),
        PgWireError::InvalidScramMessage(_) => InvalidScramMessage::new_err(msg),
        PgWireError::InvalidPassword(_) => InvalidPassword::new_err(msg),
        PgWireError::UnsupportedCertificateSignatureAlgorithm => {
            UnsupportedCertificateSignatureAlgorithm::new_err(msg)
        }
        PgWireError::UserNameRequired => UserNameRequired::new_err(msg),
        PgWireError::NotReadyForQuery => NotReadyForQuery::new_err(msg),
        PgWireError::QueryCanceled => QueryCanceled::new_err(msg),
        PgWireError::InvalidOptionValue(_) => InvalidOptionValue::new_err(msg),
        PgWireError::InvalidOauthMessage(_) => InvalidOauthMessage::new_err(msg),
        PgWireError::OAuthAuthenticationFailed(_) => OAuthAuthenticationFailed::new_err(msg),
        PgWireError::OAuthValidationError(_) => OAuthValidationError::new_err(msg),
        PgWireError::OauthAuthzIdError(_) => OauthAuthzIdError::new_err(msg),
        PgWireError::ApiError(_) => ApiError::new_err(msg),
        PgWireError::UserError(_) => UserError::new_err(msg),
    }
}

// ---------- test helper ---------------------------------------------------

/// Hidden test helper. Maps a variant name string to a `PgWireError`,
/// runs it through `pywire_to_py_err`, and raises the resulting Python
/// exception. pytest uses this to assert that every variant lands in the
/// expected Python class.
#[pyfunction]
fn _test_raise_for(variant: &str) -> PyResult<()> {
    let err = match variant {
        "UnsupportedProtocolVersion" => PgWireError::UnsupportedProtocolVersion(3, 4),
        "InvalidCancelRequest" => PgWireError::InvalidCancelRequest,
        "InvalidSecretKey" => PgWireError::InvalidSecretKey,
        "InvalidMessageType" => PgWireError::InvalidMessageType(0xFF),
        "MessageTooLarge" => PgWireError::MessageTooLarge(1024, 2048),
        "InvalidTargetType" => PgWireError::InvalidTargetType(0xFF),
        "InvalidTransactionStatus" => PgWireError::InvalidTransactionStatus(0xFF),
        "InvalidSSLRequestMessage" => PgWireError::InvalidSSLRequestMessage,
        "InvalidGssEncRequestMessage" => PgWireError::InvalidGssEncRequestMessage,
        "InvalidStartupMessage" => PgWireError::InvalidStartupMessage,
        "InvalidAuthenticationMessageCode" => PgWireError::InvalidAuthenticationMessageCode(-1),
        "FailedToCoercePasswordMessage" => PgWireError::FailedToCoercePasswordMessage,
        "InvalidSASLState" => PgWireError::InvalidSASLState,
        "UnsupportedSASLAuthMethod" => PgWireError::UnsupportedSASLAuthMethod("test".into()),
        "IoError" => PgWireError::IoError(io::Error::other("test io error")),
        "PortalNotFound" => PgWireError::PortalNotFound("p".into()),
        "PortalNotStarted" => PgWireError::PortalNotStarted,
        "StatementNotFound" => PgWireError::StatementNotFound("s".into()),
        "ParameterIndexOutOfBound" => PgWireError::ParameterIndexOutOfBound(7),
        "InvalidRustTypeForParameter" => PgWireError::InvalidRustTypeForParameter("text".into()),
        "FailedToParseParameter" => {
            PgWireError::FailedToParseParameter(Box::new(io::Error::other("parse")))
        }
        "InvalidScramMessage" => PgWireError::InvalidScramMessage("bad nonce".into()),
        "InvalidPassword" => PgWireError::InvalidPassword("alice".into()),
        "UnsupportedCertificateSignatureAlgorithm" => {
            PgWireError::UnsupportedCertificateSignatureAlgorithm
        }
        "UserNameRequired" => PgWireError::UserNameRequired,
        "NotReadyForQuery" => PgWireError::NotReadyForQuery,
        "QueryCanceled" => PgWireError::QueryCanceled,
        "InvalidOptionValue" => PgWireError::InvalidOptionValue("opt".into()),
        "InvalidOauthMessage" => PgWireError::InvalidOauthMessage("m".into()),
        "OAuthAuthenticationFailed" => PgWireError::OAuthAuthenticationFailed("m".into()),
        "OAuthValidationError" => PgWireError::OAuthValidationError("m".into()),
        "OauthAuthzIdError" => PgWireError::OauthAuthzIdError("m".into()),
        "ApiError" => PgWireError::ApiError(Box::new(io::Error::other("api"))),
        "UserError" => PgWireError::UserError(Box::new(ErrorInfo::new(
            "ERROR".into(),
            "22000".into(),
            "user-defined".into(),
        ))),
        other => {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "unknown variant for test: {other}"
            )));
        }
    };
    Err(pywire_to_py_err(err))
}

// ---------- module registration ------------------------------------------

/// Register `pywire.errors` as a submodule of the parent `_pywire` module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let m = PyModule::new(py, "errors")?;

    macro_rules! reg {
        ($name:ident) => {
            m.add(stringify!($name), py.get_type::<$name>())?;
        };
    }

    reg!(Error);
    reg!(ProtocolError);
    reg!(UnsupportedProtocolVersion);
    reg!(InvalidCancelRequest);
    reg!(InvalidMessageType);
    reg!(MessageTooLarge);
    reg!(InvalidTargetType);
    reg!(InvalidTransactionStatus);
    reg!(InvalidSSLRequestMessage);
    reg!(InvalidGssEncRequestMessage);
    reg!(InvalidStartupMessage);
    reg!(InvalidAuthenticationMessageCode);
    reg!(InvalidSecretKey);
    reg!(NotReadyForQuery);
    reg!(InvalidOptionValue);
    reg!(AuthError);
    reg!(FailedToCoercePasswordMessage);
    reg!(InvalidSASLState);
    reg!(UnsupportedSASLAuthMethod);
    reg!(InvalidScramMessage);
    reg!(InvalidPassword);
    reg!(UnsupportedCertificateSignatureAlgorithm);
    reg!(UserNameRequired);
    reg!(InvalidOauthMessage);
    reg!(OAuthAuthenticationFailed);
    reg!(OAuthValidationError);
    reg!(OauthAuthzIdError);
    reg!(PortalNotFound);
    reg!(PortalNotStarted);
    reg!(StatementNotFound);
    reg!(ParameterIndexOutOfBound);
    reg!(InvalidRustTypeForParameter);
    reg!(FailedToParseParameter);
    reg!(QueryCanceled);
    reg!(ApiError);
    reg!(UserError);

    m.add_class::<PyErrorInfo>()?;
    m.add_function(wrap_pyfunction!(_test_raise_for, &m)?)?;

    parent.add_submodule(&m)?;
    // So that `from pywire._pywire.errors import X` works inside Python.
    py.import("sys")?
        .getattr("modules")?
        .set_item("pywire._pywire.errors", &m)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_info_round_trips_through_rust() {
        let info = ErrorInfo::new("ERROR".into(), "22000".into(), "bad".into());
        let py_info: PyErrorInfo = info.into();
        assert_eq!(py_info.severity, "ERROR");
        assert_eq!(py_info.code, "22000");
        assert_eq!(py_info.message, "bad");
        assert!(py_info.detail.is_none());

        let back: ErrorInfo = py_info.into();
        assert_eq!(back.severity, "ERROR");
        assert_eq!(back.code, "22000");
        assert_eq!(back.message, "bad");
    }

    #[test]
    fn py_error_info_repr_includes_required_fields() {
        let info = PyErrorInfo::new(
            "ERROR".into(),
            "22000".into(),
            "bad".into(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let s = info.__repr__();
        assert!(s.contains("ERROR"));
        assert!(s.contains("22000"));
        assert!(s.contains("bad"));
    }

    #[test]
    fn py_error_info_is_fatal_matches_severity() {
        let fatal = PyErrorInfo::new(
            "FATAL".into(),
            "08P01".into(),
            "x".into(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(fatal.is_fatal());
        let warn = PyErrorInfo::new(
            "WARNING".into(),
            "00000".into(),
            "x".into(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(!warn.is_fatal());
    }
}
