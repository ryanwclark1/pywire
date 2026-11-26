//! Extended-query, COPY, startup/auth and notice codecs for `pywire.messages`.
//!
//! Same contract as `crate::messages`: one class per pgwire message, built
//! from Python fields, with `.encode() -> bytes` (a full frame) and
//! `Class.decode(bytes)`, structural equality, hashing and a `__repr__`.
//!
//! Conventions where pgwire uses Rust-only shapes:
//!
//! - Optional statement/portal names (`Option<String>`) are `str`, with
//!   `""` meaning the unnamed statement or portal, as on the wire.
//! - Cancel secret keys (`SecretKey`) are `bytes`: four big-endian bytes for
//!   protocol 3.0's `i32`, up to 256 bytes for protocol 3.2.
//! - `Authentication` is one class tagged by `kind`.

use bytes::Bytes;
use pgwire::messages::cancel::CancelRequest as PgCancelRequest;
use pgwire::messages::copy::{
    CopyData as PgCopyData, CopyDone as PgCopyDone, CopyFail as PgCopyFail,
    CopyInResponse as PgCopyInResponse, CopyOutResponse as PgCopyOutResponse,
};
use pgwire::messages::data::{NoData as PgNoData, ParameterDescription as PgParameterDescription};
use pgwire::messages::extendedquery::{
    Bind as PgBind, BindComplete as PgBindComplete, Close as PgClose,
    CloseComplete as PgCloseComplete, Describe as PgDescribe, Execute as PgExecute,
    Flush as PgFlush, Parse as PgParse, ParseComplete as PgParseComplete,
    PortalSuspended as PgPortalSuspended, Sync as PgSync,
};
use pgwire::messages::response::{
    EmptyQueryResponse as PgEmptyQueryResponse, NoticeResponse as PgNoticeResponse,
    NotificationResponse as PgNotificationResponse,
};
use pgwire::messages::startup::{
    Authentication as PgAuthentication, BackendKeyData as PgBackendKeyData,
    ParameterStatus as PgParameterStatus, Password as PgPassword,
    SASLInitialResponse as PgSASLInitialResponse, SASLResponse as PgSASLResponse, SecretKey,
    SslRequest as PgSslRequest,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyType};

use crate::messages::{decode_from_slice, encode_to_pybytes};

// ---------- helpers -----------------------------------------------------

fn name_to_pg(name: &str) -> Option<String> {
    (!name.is_empty()).then(|| name.to_owned())
}

fn name_from_pg(name: Option<String>) -> String {
    name.unwrap_or_default()
}

fn secret_to_bytes(key: &SecretKey) -> Vec<u8> {
    match key {
        SecretKey::I32(key) => key.to_be_bytes().to_vec(),
        SecretKey::Bytes(key) => key.to_vec(),
    }
}

fn secret_from_bytes(key: &[u8]) -> SecretKey {
    match <[u8; 4]>::try_from(key) {
        Ok(four) => SecretKey::I32(i32::from_be_bytes(four)),
        Err(_) => SecretKey::Bytes(Bytes::copy_from_slice(key)),
    }
}

/// `b'S'` / `b'P'` target of `Describe` and `Close`, as a Python string.
fn target_from_str(target: &str) -> PyResult<u8> {
    match target {
        "S" => Ok(b'S'),
        "P" => Ok(b'P'),
        other => Err(PyValueError::new_err(format!(
            "target must be 'S' (statement) or 'P' (portal), not {other:?}"
        ))),
    }
}

fn target_to_str(target: u8) -> String {
    char::from(target).to_string()
}

/// Messages without a payload: construct, encode, decode, compare.
macro_rules! unit_message {
    ($(#[$doc:meta])* $py_name:literal, $rust:ident, $pg:ty) => {
        $(#[$doc])*
        #[pyclass(name = $py_name, module = "pywire.messages", frozen, eq, hash)]
        #[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $rust;

        #[pymethods]
        impl $rust {
            #[new]
            fn new() -> Self {
                Self
            }

            fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
                encode_to_pybytes(py, &<$pg>::new())
            }

            #[classmethod]
            fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
                let _: $pg = decode_from_slice(data)?;
                Ok(Self)
            }

            fn __repr__(&self) -> String {
                concat!($py_name, "()").to_owned()
            }
        }
    };
}

unit_message!(
    /// Backend `'1'`: Parse succeeded.
    "ParseComplete",
    PyParseComplete,
    PgParseComplete
);
unit_message!(
    /// Backend `'2'`: Bind succeeded.
    "BindComplete",
    PyBindComplete,
    PgBindComplete
);
unit_message!(
    /// Backend `'3'`: Close succeeded.
    "CloseComplete",
    PyCloseComplete,
    PgCloseComplete
);
unit_message!(
    /// Backend `'n'`: the described statement or portal returns no rows.
    "NoData",
    PyNoData,
    PgNoData
);
unit_message!(
    /// Backend `'s'`: Execute stopped at its row limit.
    "PortalSuspended",
    PyPortalSuspended,
    PgPortalSuspended
);
unit_message!(
    /// Backend `'I'`: response to an empty query string.
    "EmptyQueryResponse",
    PyEmptyQueryResponse,
    PgEmptyQueryResponse
);
unit_message!(
    /// Frontend `'S'`: end of an extended-query batch.
    "Sync",
    PySync,
    PgSync
);
unit_message!(
    /// Frontend `'H'`: flush pending backend output.
    "Flush",
    PyFlush,
    PgFlush
);
unit_message!(
    /// `'c'`: end of COPY data (either direction).
    "CopyDone",
    PyCopyDone,
    PgCopyDone
);
unit_message!(
    /// Frontend SSL negotiation request. Like `Startup`, it has no type tag.
    "SslRequest",
    PySslRequest,
    PgSslRequest
);

// ---------- extended query ---------------------------------------------

/// Frontend `'P'`: parse `query` into prepared statement `name`.
#[pyclass(name = "Parse", module = "pywire.messages", frozen, eq, hash)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PyParse {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    query: String,
    #[pyo3(get)]
    type_oids: Vec<u32>,
}

#[pymethods]
impl PyParse {
    #[new]
    #[pyo3(signature = (query, *, name = String::new(), type_oids = Vec::new()), text_signature = "(query, *, name='', type_oids=())")]
    fn new(query: String, name: String, type_oids: Vec<u32>) -> Self {
        Self {
            name,
            query,
            type_oids,
        }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let message = PgParse::new(
            name_to_pg(&self.name),
            self.query.clone(),
            self.type_oids.clone(),
        );
        encode_to_pybytes(py, &message)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let message: PgParse = decode_from_slice(data)?;
        Ok(Self {
            name: name_from_pg(message.name),
            query: message.query,
            type_oids: message.type_oids,
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "Parse(query={:?}, name={:?}, type_oids={:?})",
            self.query, self.name, self.type_oids
        )
    }
}

/// Frontend `'B'`: bind parameters to a statement, creating a portal.
#[pyclass(name = "Bind", module = "pywire.messages", frozen, eq, hash)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PyBind {
    #[pyo3(get)]
    portal_name: String,
    #[pyo3(get)]
    statement_name: String,
    #[pyo3(get)]
    parameter_formats: Vec<i16>,
    parameters: Vec<Option<Vec<u8>>>,
    #[pyo3(get)]
    result_formats: Vec<i16>,
}

#[pymethods]
impl PyBind {
    #[new]
    #[pyo3(
        signature = (*, portal_name = String::new(), statement_name = String::new(), parameters = Vec::new(), parameter_formats = Vec::new(), result_formats = Vec::new()),
        text_signature = "(*, portal_name='', statement_name='', parameters=(), parameter_formats=(), result_formats=())"
    )]
    fn new(
        portal_name: String,
        statement_name: String,
        parameters: Vec<Option<Vec<u8>>>,
        parameter_formats: Vec<i16>,
        result_formats: Vec<i16>,
    ) -> Self {
        Self {
            portal_name,
            statement_name,
            parameter_formats,
            parameters,
            result_formats,
        }
    }

    /// Parameter values (`None` is SQL NULL), encoded per `parameter_formats`.
    #[getter]
    fn parameters<'py>(&self, py: Python<'py>) -> Vec<Option<Bound<'py, PyBytes>>> {
        self.parameters
            .iter()
            .map(|value| value.as_deref().map(|bytes| PyBytes::new(py, bytes)))
            .collect()
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let message = PgBind::new(
            name_to_pg(&self.portal_name),
            name_to_pg(&self.statement_name),
            self.parameter_formats.clone(),
            self.parameters
                .iter()
                .map(|value| value.as_deref().map(Bytes::copy_from_slice))
                .collect(),
            self.result_formats.clone(),
        );
        encode_to_pybytes(py, &message)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let message: PgBind = decode_from_slice(data)?;
        Ok(Self {
            portal_name: name_from_pg(message.portal_name),
            statement_name: name_from_pg(message.statement_name),
            parameter_formats: message.parameter_format_codes,
            parameters: message
                .parameters
                .into_iter()
                .map(|value| value.map(|bytes| bytes.to_vec()))
                .collect(),
            result_formats: message.result_column_format_codes,
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "Bind(portal_name={:?}, statement_name={:?}, parameters=<{}>, parameter_formats={:?}, result_formats={:?})",
            self.portal_name,
            self.statement_name,
            self.parameters.len(),
            self.parameter_formats,
            self.result_formats
        )
    }
}

macro_rules! target_message {
    ($(#[$doc:meta])* $py_name:literal, $rust:ident, $pg:ident) => {
        $(#[$doc])*
        #[pyclass(name = $py_name, module = "pywire.messages", frozen, eq, hash)]
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $rust {
            /// `"S"` for a prepared statement, `"P"` for a portal.
            #[pyo3(get)]
            target: String,
            #[pyo3(get)]
            name: String,
        }

        #[pymethods]
        impl $rust {
            #[new]
            #[pyo3(signature = (target, name = String::new()), text_signature = "(target, name='')")]
            fn new(target: &str, name: String) -> PyResult<Self> {
                target_from_str(target)?;
                Ok(Self {
                    target: target.to_owned(),
                    name,
                })
            }

            fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
                let target = target_from_str(&self.target)?;
                encode_to_pybytes(py, &$pg::new(target, name_to_pg(&self.name)))
            }

            #[classmethod]
            fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
                let message: $pg = decode_from_slice(data)?;
                Ok(Self {
                    target: target_to_str(message.target_type),
                    name: name_from_pg(message.name),
                })
            }

            fn __repr__(&self) -> String {
                format!(concat!($py_name, "(target={:?}, name={:?})"), self.target, self.name)
            }
        }
    };
}

target_message!(
    /// Frontend `'D'`: describe a prepared statement (`"S"`) or portal (`"P"`).
    "Describe",
    PyDescribe,
    PgDescribe
);
target_message!(
    /// Frontend `'C'`: close a prepared statement (`"S"`) or portal (`"P"`).
    "Close",
    PyClose,
    PgClose
);

/// Frontend `'E'`: run portal `name`, returning at most `max_rows` rows
/// (`0` means no limit).
#[pyclass(name = "Execute", module = "pywire.messages", frozen, eq, hash)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PyExecute {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    max_rows: i32,
}

#[pymethods]
impl PyExecute {
    #[new]
    #[pyo3(signature = (name = String::new(), max_rows = 0), text_signature = "(name='', max_rows=0)")]
    fn new(name: String, max_rows: i32) -> Self {
        Self { name, max_rows }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        encode_to_pybytes(py, &PgExecute::new(name_to_pg(&self.name), self.max_rows))
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let message: PgExecute = decode_from_slice(data)?;
        Ok(Self {
            name: name_from_pg(message.name),
            max_rows: message.max_rows,
        })
    }

    fn __repr__(&self) -> String {
        format!("Execute(name={:?}, max_rows={})", self.name, self.max_rows)
    }
}

/// Backend `'t'`: parameter type OIDs of a described statement.
#[pyclass(
    name = "ParameterDescription",
    module = "pywire.messages",
    frozen,
    eq,
    hash
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PyParameterDescription {
    #[pyo3(get)]
    types: Vec<u32>,
}

#[pymethods]
impl PyParameterDescription {
    #[new]
    #[pyo3(signature = (types = Vec::new()), text_signature = "(types=())")]
    fn new(types: Vec<u32>) -> Self {
        Self { types }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        encode_to_pybytes(py, &PgParameterDescription::new(self.types.clone()))
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let message: PgParameterDescription = decode_from_slice(data)?;
        Ok(Self {
            types: message.types,
        })
    }

    fn __repr__(&self) -> String {
        format!("ParameterDescription(types={:?})", self.types)
    }
}

// ---------- COPY --------------------------------------------------------

/// `'d'`: one chunk of COPY data (either direction).
#[pyclass(name = "CopyData", module = "pywire.messages", frozen, eq, hash)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PyCopyData {
    data: Vec<u8>,
}

#[pymethods]
impl PyCopyData {
    #[new]
    fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.data)
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        encode_to_pybytes(py, &PgCopyData::new(Bytes::copy_from_slice(&self.data)))
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let message: PgCopyData = decode_from_slice(data)?;
        Ok(Self {
            data: message.data.to_vec(),
        })
    }

    fn __repr__(&self) -> String {
        format!("CopyData(<{} bytes>)", self.data.len())
    }
}

/// Frontend `'f'`: abort COPY FROM STDIN with `message`.
#[pyclass(name = "CopyFail", module = "pywire.messages", frozen, eq, hash)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PyCopyFail {
    #[pyo3(get)]
    message: String,
}

#[pymethods]
impl PyCopyFail {
    #[new]
    fn new(message: String) -> Self {
        Self { message }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        encode_to_pybytes(py, &PgCopyFail::new(self.message.clone()))
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let message: PgCopyFail = decode_from_slice(data)?;
        Ok(Self {
            message: message.message,
        })
    }

    fn __repr__(&self) -> String {
        format!("CopyFail(message={:?})", self.message)
    }
}

macro_rules! copy_response {
    ($(#[$doc:meta])* $py_name:literal, $rust:ident, $pg:ident) => {
        $(#[$doc])*
        #[pyclass(name = $py_name, module = "pywire.messages", frozen, eq, hash)]
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $rust {
            /// Overall format: `0` text, `1` binary.
            #[pyo3(get)]
            format: i8,
            #[pyo3(get)]
            columns: i16,
            /// One format code per column.
            #[pyo3(get)]
            column_formats: Vec<i16>,
        }

        #[pymethods]
        impl $rust {
            #[new]
            #[pyo3(signature = (format, columns, column_formats = None))]
            fn new(format: i8, columns: i16, column_formats: Option<Vec<i16>>) -> Self {
                let column_formats = column_formats
                    .unwrap_or_else(|| vec![i16::from(format); usize::try_from(columns).unwrap_or(0)]);
                Self {
                    format,
                    columns,
                    column_formats,
                }
            }

            fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
                let message = $pg::new(self.format, self.columns, self.column_formats.clone());
                encode_to_pybytes(py, &message)
            }

            #[classmethod]
            fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
                let message: $pg = decode_from_slice(data)?;
                Ok(Self {
                    format: message.format,
                    columns: message.columns,
                    column_formats: message.column_formats,
                })
            }

            fn __repr__(&self) -> String {
                format!(
                    concat!($py_name, "(format={}, columns={}, column_formats={:?})"),
                    self.format, self.columns, self.column_formats
                )
            }
        }
    };
}

copy_response!(
    /// Backend `'G'`: the server is ready to receive COPY data.
    "CopyInResponse",
    PyCopyInResponse,
    PgCopyInResponse
);
copy_response!(
    /// Backend `'H'`: the server is about to send COPY data.
    "CopyOutResponse",
    PyCopyOutResponse,
    PgCopyOutResponse
);

// ---------- startup, auth and session status ----------------------------

/// Backend `'R'`: an authentication request or result, tagged by `kind`.
///
/// `kind` is one of `"ok"`, `"cleartext_password"`, `"kerberos_v5"`,
/// `"md5_password"` (with a 4-byte `salt`), `"sasl"` (with the offered
/// `mechanisms`), `"sasl_continue"` or `"sasl_final"` (with `data`).
#[pyclass(name = "Authentication", module = "pywire.messages", frozen, eq, hash)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PyAuthentication {
    #[pyo3(get)]
    kind: String,
    salt: Option<Vec<u8>>,
    #[pyo3(get)]
    mechanisms: Vec<String>,
    data: Option<Vec<u8>>,
}

impl PyAuthentication {
    fn to_pg(&self) -> PyResult<PgAuthentication> {
        let missing = |field: &str| {
            PyValueError::new_err(format!("Authentication {:?} requires {field}", self.kind))
        };
        Ok(match self.kind.as_str() {
            "ok" => PgAuthentication::Ok,
            "cleartext_password" => PgAuthentication::CleartextPassword,
            "kerberos_v5" => PgAuthentication::KerberosV5,
            "md5_password" => match self.salt.as_deref() {
                Some(salt) if salt.len() == 4 => PgAuthentication::MD5Password(salt.to_vec()),
                _ => return Err(missing("a 4-byte salt")),
            },
            "sasl" => PgAuthentication::SASL(self.mechanisms.clone()),
            "sasl_continue" | "sasl_final" => {
                let data = Bytes::from(self.data.clone().ok_or_else(|| missing("data"))?);
                if self.kind == "sasl_continue" {
                    PgAuthentication::SASLContinue(data)
                } else {
                    PgAuthentication::SASLFinal(data)
                }
            }
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown Authentication kind {other:?}"
                )))
            }
        })
    }

    fn from_pg(message: PgAuthentication) -> Self {
        let (kind, salt, mechanisms, data) = match message {
            PgAuthentication::Ok => ("ok", None, Vec::new(), None),
            PgAuthentication::CleartextPassword => ("cleartext_password", None, Vec::new(), None),
            PgAuthentication::KerberosV5 => ("kerberos_v5", None, Vec::new(), None),
            PgAuthentication::MD5Password(salt) => ("md5_password", Some(salt), Vec::new(), None),
            PgAuthentication::SASL(mechanisms) => ("sasl", None, mechanisms, None),
            PgAuthentication::SASLContinue(data) => {
                ("sasl_continue", None, Vec::new(), Some(data.to_vec()))
            }
            PgAuthentication::SASLFinal(data) => {
                ("sasl_final", None, Vec::new(), Some(data.to_vec()))
            }
            // pgwire marks the enum non-exhaustive; its decoder only
            // produces the variants above.
            _ => ("unknown", None, Vec::new(), None), // LCOV_EXCL_LINE
        };
        Self {
            kind: kind.to_owned(),
            salt,
            mechanisms,
            data,
        }
    }
}

#[pymethods]
impl PyAuthentication {
    #[new]
    #[pyo3(signature = (kind, *, salt = None, mechanisms = Vec::new(), data = None), text_signature = "(kind, *, salt=None, mechanisms=(), data=None)")]
    fn new(
        kind: String,
        salt: Option<Vec<u8>>,
        mechanisms: Vec<String>,
        data: Option<Vec<u8>>,
    ) -> PyResult<Self> {
        let message = Self {
            kind,
            salt,
            mechanisms,
            data,
        };
        message.to_pg()?;
        Ok(message)
    }

    #[getter]
    fn salt<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.salt.as_deref().map(|salt| PyBytes::new(py, salt))
    }

    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.data.as_deref().map(|data| PyBytes::new(py, data))
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        encode_to_pybytes(py, &self.to_pg()?)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        Ok(Self::from_pg(decode_from_slice(data)?))
    }

    fn __repr__(&self) -> String {
        format!("Authentication(kind={:?})", self.kind)
    }
}

/// Frontend `'p'`: a cleartext (or MD5-hashed) password.
#[pyclass(name = "PasswordMessage", module = "pywire.messages", frozen, eq, hash)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PyPasswordMessage {
    #[pyo3(get)]
    password: String,
}

#[pymethods]
impl PyPasswordMessage {
    #[new]
    fn new(password: String) -> Self {
        Self { password }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        encode_to_pybytes(py, &PgPassword::new(self.password.clone()))
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let message: PgPassword = decode_from_slice(data)?;
        Ok(Self {
            password: message.password,
        })
    }

    fn __repr__(&self) -> String {
        "PasswordMessage(<redacted>)".to_owned()
    }
}

/// Frontend `'p'`: first SASL message, naming the chosen mechanism.
#[pyclass(
    name = "SASLInitialResponse",
    module = "pywire.messages",
    frozen,
    eq,
    hash
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PySASLInitialResponse {
    #[pyo3(get)]
    mechanism: String,
    data: Option<Vec<u8>>,
}

#[pymethods]
impl PySASLInitialResponse {
    #[new]
    #[pyo3(signature = (mechanism, data = None))]
    fn new(mechanism: String, data: Option<Vec<u8>>) -> Self {
        Self { mechanism, data }
    }

    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.data.as_deref().map(|data| PyBytes::new(py, data))
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let message = PgSASLInitialResponse::new(
            self.mechanism.clone(),
            self.data.as_deref().map(Bytes::copy_from_slice),
        );
        encode_to_pybytes(py, &message)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let message: PgSASLInitialResponse = decode_from_slice(data)?;
        Ok(Self {
            mechanism: message.auth_method,
            data: message.data.map(|data| data.to_vec()),
        })
    }

    fn __repr__(&self) -> String {
        format!("SASLInitialResponse(mechanism={:?})", self.mechanism)
    }
}

/// Frontend `'p'`: a follow-up SASL message.
#[pyclass(name = "SASLResponse", module = "pywire.messages", frozen, eq, hash)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PySASLResponse {
    data: Vec<u8>,
}

#[pymethods]
impl PySASLResponse {
    #[new]
    fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.data)
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let message = PgSASLResponse::new(Bytes::copy_from_slice(&self.data));
        encode_to_pybytes(py, &message)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let message: PgSASLResponse = decode_from_slice(data)?;
        Ok(Self {
            data: message.data.to_vec(),
        })
    }

    fn __repr__(&self) -> String {
        format!("SASLResponse(<{} bytes>)", self.data.len())
    }
}

/// Backend `'S'`: a run-time parameter such as `server_version`.
#[pyclass(name = "ParameterStatus", module = "pywire.messages", frozen, eq, hash)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PyParameterStatus {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    value: String,
}

#[pymethods]
impl PyParameterStatus {
    #[new]
    fn new(name: String, value: String) -> Self {
        Self { name, value }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let message = PgParameterStatus::new(self.name.clone(), self.value.clone());
        encode_to_pybytes(py, &message)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let message: PgParameterStatus = decode_from_slice(data)?;
        Ok(Self {
            name: message.name,
            value: message.value,
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "ParameterStatus(name={:?}, value={:?})",
            self.name, self.value
        )
    }
}

macro_rules! key_message {
    ($(#[$doc:meta])* $py_name:literal, $rust:ident, $pg:ident) => {
        $(#[$doc])*
        #[pyclass(name = $py_name, module = "pywire.messages", frozen, eq, hash)]
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $rust {
            #[pyo3(get)]
            pid: i32,
            secret_key: Vec<u8>,
        }

        #[pymethods]
        impl $rust {
            #[new]
            fn new(pid: i32, secret_key: Vec<u8>) -> Self {
                Self { pid, secret_key }
            }

            /// Cancel key: 4 bytes in protocol 3.0, up to 256 in 3.2.
            #[getter]
            fn secret_key<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
                PyBytes::new(py, &self.secret_key)
            }

            fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
                let message = $pg::new(self.pid, secret_from_bytes(&self.secret_key));
                encode_to_pybytes(py, &message)
            }

            #[classmethod]
            fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
                let message: $pg = decode_from_slice(data)?;
                Ok(Self {
                    pid: message.pid,
                    secret_key: secret_to_bytes(&message.secret_key),
                })
            }

            fn __repr__(&self) -> String {
                format!(concat!($py_name, "(pid={}, <secret key>)"), self.pid)
            }
        }
    };
}

key_message!(
    /// Backend `'K'`: the process ID and secret key for cancel requests.
    "BackendKeyData",
    PyBackendKeyData,
    PgBackendKeyData
);
key_message!(
    /// Frontend cancel request (sent on a new connection, no type tag).
    "CancelRequest",
    PyCancelRequest,
    PgCancelRequest
);

macro_rules! field_list_message {
    ($(#[$doc:meta])* $py_name:literal, $rust:ident, $pg:ident) => {
        $(#[$doc])*
        #[pyclass(name = $py_name, module = "pywire.messages", frozen, eq, hash)]
        #[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $rust {
            /// `(tag_byte, value)` pairs, as in `ErrorResponse`.
            #[pyo3(get)]
            fields: Vec<(u8, String)>,
        }

        #[pymethods]
        impl $rust {
            #[new]
            #[pyo3(signature = (fields = None))]
            fn new(fields: Option<Vec<(u8, String)>>) -> Self {
                Self {
                    fields: fields.unwrap_or_default(),
                }
            }

            fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
                encode_to_pybytes(py, &$pg::new(self.fields.clone()))
            }

            #[classmethod]
            fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
                let message: $pg = decode_from_slice(data)?;
                Ok(Self {
                    fields: message.fields,
                })
            }

            fn __repr__(&self) -> String {
                format!(concat!($py_name, "(field_count={})"), self.fields.len())
            }
        }
    };
}

field_list_message!(
    /// Backend `'N'`: a notice (warning or informational message).
    "NoticeResponse",
    PyNoticeResponse,
    PgNoticeResponse
);

/// Backend `'A'`: a `LISTEN`/`NOTIFY` notification.
#[pyclass(
    name = "NotificationResponse",
    module = "pywire.messages",
    frozen,
    eq,
    hash
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PyNotificationResponse {
    #[pyo3(get)]
    pid: i32,
    #[pyo3(get)]
    channel: String,
    #[pyo3(get)]
    payload: String,
}

#[pymethods]
impl PyNotificationResponse {
    #[new]
    #[pyo3(signature = (pid, channel, payload = String::new()), text_signature = "(pid, channel, payload='')")]
    fn new(pid: i32, channel: String, payload: String) -> Self {
        Self {
            pid,
            channel,
            payload,
        }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let message =
            PgNotificationResponse::new(self.pid, self.channel.clone(), self.payload.clone());
        encode_to_pybytes(py, &message)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let message: PgNotificationResponse = decode_from_slice(data)?;
        Ok(Self {
            pid: message.pid,
            channel: message.channel,
            payload: message.payload,
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "NotificationResponse(pid={}, channel={:?}, payload={:?})",
            self.pid, self.channel, self.payload
        )
    }
}

/// Add every class in this module to the `pywire.messages` submodule.
pub fn add_classes(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyParse>()?;
    m.add_class::<PyParseComplete>()?;
    m.add_class::<PyBind>()?;
    m.add_class::<PyBindComplete>()?;
    m.add_class::<PyDescribe>()?;
    m.add_class::<PyExecute>()?;
    m.add_class::<PyClose>()?;
    m.add_class::<PyCloseComplete>()?;
    m.add_class::<PySync>()?;
    m.add_class::<PyFlush>()?;
    m.add_class::<PyParameterDescription>()?;
    m.add_class::<PyNoData>()?;
    m.add_class::<PyPortalSuspended>()?;
    m.add_class::<PyEmptyQueryResponse>()?;
    m.add_class::<PyCopyData>()?;
    m.add_class::<PyCopyDone>()?;
    m.add_class::<PyCopyFail>()?;
    m.add_class::<PyCopyInResponse>()?;
    m.add_class::<PyCopyOutResponse>()?;
    m.add_class::<PySslRequest>()?;
    m.add_class::<PyAuthentication>()?;
    m.add_class::<PyPasswordMessage>()?;
    m.add_class::<PySASLInitialResponse>()?;
    m.add_class::<PySASLResponse>()?;
    m.add_class::<PyParameterStatus>()?;
    m.add_class::<PyBackendKeyData>()?;
    m.add_class::<PyCancelRequest>()?;
    m.add_class::<PyNoticeResponse>()?;
    m.add_class::<PyNotificationResponse>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_secret_keys_convert_both_ways() {
        assert_eq!(name_to_pg(""), None);
        assert_eq!(name_to_pg("s1").as_deref(), Some("s1"));
        assert_eq!(name_from_pg(None), "");
        let short = secret_from_bytes(&[0, 0, 1, 2]);
        assert_eq!(short, SecretKey::I32(258));
        assert_eq!(secret_to_bytes(&short), vec![0, 0, 1, 2]);
        let long = secret_from_bytes(&[7; 32]);
        assert_eq!(secret_to_bytes(&long), vec![7; 32]);
        assert_eq!(target_to_str(b'P'), "P");
    }
}
