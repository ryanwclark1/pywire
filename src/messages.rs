//! Python binding for the foundational `pgwire::messages` codec layer.
//!
//! In scope for this first messages PR (`pywire.messages`):
//!
//! Frontend:
//!   - `Startup`         (frontend, no type tag)
//!   - `Query`           (`'Q'`)
//!   - `Terminate`       (`'X'`)
//!
//! Backend:
//!   - `ReadyForQuery`   (`'Z'`)   + `TransactionStatus` enum
//!   - `CommandComplete` (`'C'`)
//!   - `RowDescription`  (`'T'`)   + `FieldDescription` row entries
//!   - `DataRow`         (`'D'`)
//!   - `ErrorResponse`   (`'E'`)
//!
//! Every class supports:
//!
//! - construction from Python with named fields
//! - `.encode() -> bytes`        (full wire frame, including type tag + length)
//! - `Class.decode(data) -> ...` (parses a single full wire frame; raises
//!   `pywire.errors.ProtocolError` on malformed input)
//! - `__eq__` and a `__repr__` that includes every field
//!
//! Extended-query / COPY / startup-handshake messages land in later PRs;
//! see `BINDING_STRATEGY.md`.

use bytes::BytesMut;
use pgwire::messages::{
    data::{
        DataRow as PgDataRow, FieldDescription as PgFieldDescription,
        RowDescription as PgRowDescription,
    },
    response::{
        CommandComplete as PgCommandComplete, ErrorResponse as PgErrorResponse,
        ReadyForQuery as PgReadyForQuery, TransactionStatus as PgTransactionStatus,
    },
    simplequery::Query as PgQuery,
    startup::Startup as PgStartup,
    terminate::Terminate as PgTerminate,
    DecodeContext, Message,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyType};

use crate::errors::pywire_to_py_err;

// ---------- shared codec helpers ----------------------------------------

fn encode_to_pybytes<'py, T: Message>(py: Python<'py>, msg: &T) -> PyResult<Bound<'py, PyBytes>> {
    let mut buf = BytesMut::new();
    msg.encode(&mut buf).map_err(pywire_to_py_err)?;
    Ok(PyBytes::new(py, &buf))
}

fn decode_from_slice<T: Message>(data: &[u8]) -> PyResult<T> {
    let mut buf = BytesMut::from(data);
    let ctx = DecodeContext::default();
    match T::decode(&mut buf, &ctx).map_err(pywire_to_py_err)? {
        Some(msg) => Ok(msg),
        None => Err(PyValueError::new_err(
            "incomplete message: input bytes are shorter than the declared length",
        )),
    }
}

// ---------- TransactionStatus (ReadyForQuery indicator) -----------------

/// Transaction status reported by the backend in `ReadyForQuery`.
#[pyclass(
    name = "TransactionStatus",
    module = "pywire.messages",
    eq,
    eq_int,
    from_py_object
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PyTransactionStatus {
    /// Not currently in a transaction (`'I'`).
    Idle,
    /// Inside a transaction block (`'T'`).
    Transaction,
    /// Inside a failed transaction block (`'E'`).
    Error,
}

impl From<PgTransactionStatus> for PyTransactionStatus {
    fn from(s: PgTransactionStatus) -> Self {
        match s {
            PgTransactionStatus::Idle => Self::Idle,
            PgTransactionStatus::Transaction => Self::Transaction,
            PgTransactionStatus::Error => Self::Error,
        }
    }
}

impl From<PyTransactionStatus> for PgTransactionStatus {
    fn from(s: PyTransactionStatus) -> Self {
        match s {
            PyTransactionStatus::Idle => Self::Idle,
            PyTransactionStatus::Transaction => Self::Transaction,
            PyTransactionStatus::Error => Self::Error,
        }
    }
}

// ---------- Startup -----------------------------------------------------

/// Frontend startup message. No type-tag byte; encoded as
/// `length(4) + protocol_major(2) + protocol_minor(2) + parameters + 0x00`.
#[pyclass(
    name = "Startup",
    module = "pywire.messages",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyStartup {
    #[pyo3(get)]
    pub protocol_number_major: u16,
    #[pyo3(get)]
    pub protocol_number_minor: u16,
    #[pyo3(get)]
    pub parameters: std::collections::BTreeMap<String, String>,
}

#[pymethods]
impl PyStartup {
    #[new]
    #[pyo3(signature = (protocol_number_major = 3, protocol_number_minor = 0, parameters = None))]
    fn new(
        protocol_number_major: u16,
        protocol_number_minor: u16,
        parameters: Option<std::collections::BTreeMap<String, String>>,
    ) -> Self {
        Self {
            protocol_number_major,
            protocol_number_minor,
            parameters: parameters.unwrap_or_default(),
        }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        // pgwire's structs are `#[non_exhaustive]`, so we can't construct
        // them by field. Use the derive-new constructor and then assign.
        let mut s = PgStartup::new();
        s.protocol_number_major = self.protocol_number_major;
        s.protocol_number_minor = self.protocol_number_minor;
        s.parameters = self.parameters.clone();
        encode_to_pybytes(py, &s)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let s: PgStartup = decode_from_slice(data)?;
        Ok(Self {
            protocol_number_major: s.protocol_number_major,
            protocol_number_minor: s.protocol_number_minor,
            parameters: s.parameters,
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "Startup(protocol_number_major={}, protocol_number_minor={}, parameters={:?})",
            self.protocol_number_major, self.protocol_number_minor, self.parameters
        )
    }
}

// ---------- Query -------------------------------------------------------

/// Simple-query frontend message (`'Q'`).
#[pyclass(
    name = "Query",
    module = "pywire.messages",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyQuery {
    #[pyo3(get)]
    pub query: String,
}

#[pymethods]
impl PyQuery {
    #[new]
    fn new(query: String) -> Self {
        Self { query }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let q = PgQuery::new(self.query.clone());
        encode_to_pybytes(py, &q)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let q: PgQuery = decode_from_slice(data)?;
        Ok(Self { query: q.query })
    }

    fn __repr__(&self) -> String {
        format!("Query(query={:?})", self.query)
    }
}

// ---------- Terminate ---------------------------------------------------

/// Frontend connection-close message (`'X'`). Carries no payload.
#[pyclass(
    name = "Terminate",
    module = "pywire.messages",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PyTerminate;

#[pymethods]
impl PyTerminate {
    #[new]
    fn new() -> Self {
        Self
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        encode_to_pybytes(py, &PgTerminate::new())
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let _: PgTerminate = decode_from_slice(data)?;
        Ok(Self)
    }

    fn __repr__(&self) -> String {
        "Terminate()".to_owned()
    }
}

// ---------- ReadyForQuery -----------------------------------------------

/// Backend `'Z'` message. Indicates the backend is ready for a new query
/// cycle, with the current transaction status.
#[pyclass(
    name = "ReadyForQuery",
    module = "pywire.messages",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyReadyForQuery {
    #[pyo3(get)]
    pub status: PyTransactionStatus,
}

#[pymethods]
impl PyReadyForQuery {
    #[new]
    fn new(status: PyTransactionStatus) -> Self {
        Self { status }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let r = PgReadyForQuery::new(self.status.into());
        encode_to_pybytes(py, &r)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let r: PgReadyForQuery = decode_from_slice(data)?;
        Ok(Self {
            status: r.status.into(),
        })
    }

    fn __repr__(&self) -> String {
        format!("ReadyForQuery(status={:?})", self.status)
    }
}

// ---------- CommandComplete --------------------------------------------

/// Backend `'C'` message. Sent after a simple-query command finishes.
#[pyclass(
    name = "CommandComplete",
    module = "pywire.messages",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyCommandComplete {
    #[pyo3(get)]
    pub tag: String,
}

#[pymethods]
impl PyCommandComplete {
    #[new]
    fn new(tag: String) -> Self {
        Self { tag }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let c = PgCommandComplete::new(self.tag.clone());
        encode_to_pybytes(py, &c)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let c: PgCommandComplete = decode_from_slice(data)?;
        Ok(Self { tag: c.tag })
    }

    fn __repr__(&self) -> String {
        format!("CommandComplete(tag={:?})", self.tag)
    }
}

// ---------- FieldDescription + RowDescription --------------------------

/// One row of a `RowDescription` message: one column's metadata.
#[pyclass(
    name = "FieldDescription",
    module = "pywire.messages",
    frozen,
    eq,
    from_py_object
)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PyFieldDescription {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub table_id: i32,
    #[pyo3(get)]
    pub column_id: i16,
    #[pyo3(get)]
    pub type_id: u32,
    #[pyo3(get)]
    pub type_size: i16,
    #[pyo3(get)]
    pub type_modifier: i32,
    #[pyo3(get)]
    pub format_code: i16,
}

#[pymethods]
impl PyFieldDescription {
    #[new]
    #[pyo3(signature = (
        name,
        *,
        table_id = 0,
        column_id = 0,
        type_id = 0,
        type_size = 0,
        type_modifier = 0,
        format_code = 0,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        name: String,
        table_id: i32,
        column_id: i16,
        type_id: u32,
        type_size: i16,
        type_modifier: i32,
        format_code: i16,
    ) -> Self {
        Self {
            name,
            table_id,
            column_id,
            type_id,
            type_size,
            type_modifier,
            format_code,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "FieldDescription(name={:?}, type_id={}, format_code={})",
            self.name, self.type_id, self.format_code
        )
    }
}

impl From<PyFieldDescription> for PgFieldDescription {
    fn from(f: PyFieldDescription) -> Self {
        PgFieldDescription::new(
            f.name,
            f.table_id,
            f.column_id,
            f.type_id,
            f.type_size,
            f.type_modifier,
            f.format_code,
        )
    }
}

impl From<PgFieldDescription> for PyFieldDescription {
    fn from(f: PgFieldDescription) -> Self {
        Self {
            name: f.name,
            table_id: f.table_id,
            column_id: f.column_id,
            type_id: f.type_id,
            type_size: f.type_size,
            type_modifier: f.type_modifier,
            format_code: f.format_code,
        }
    }
}

/// Backend `'T'` message. Describes the columns of a result set.
#[pyclass(
    name = "RowDescription",
    module = "pywire.messages",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PyRowDescription {
    #[pyo3(get)]
    pub fields: Vec<PyFieldDescription>,
}

#[pymethods]
impl PyRowDescription {
    #[new]
    #[pyo3(signature = (fields = None))]
    fn new(fields: Option<Vec<PyFieldDescription>>) -> Self {
        Self {
            fields: fields.unwrap_or_default(),
        }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let r = PgRowDescription::new(self.fields.iter().cloned().map(Into::into).collect());
        encode_to_pybytes(py, &r)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let r: PgRowDescription = decode_from_slice(data)?;
        Ok(Self {
            fields: r.fields.into_iter().map(Into::into).collect(),
        })
    }

    fn __repr__(&self) -> String {
        let inner = self
            .fields
            .iter()
            .map(|f| f.__repr__())
            .collect::<Vec<_>>()
            .join(", ");
        format!("RowDescription(fields=[{inner}])")
    }
}

// ---------- DataRow ----------------------------------------------------

/// Backend `'D'` message. Carries the raw (already-encoded) payload of one
/// row. The payload format follows the format codes from the most recent
/// `RowDescription` / `Bind` message — pywire treats it as opaque bytes
/// at this layer.
#[pyclass(
    name = "DataRow",
    module = "pywire.messages",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PyDataRow {
    #[pyo3(get)]
    pub field_count: i16,
    /// Raw row payload (the format-coded column values, one after another).
    pub data: Vec<u8>,
}

#[pymethods]
impl PyDataRow {
    #[new]
    fn new(field_count: i16, data: &[u8]) -> Self {
        Self {
            field_count,
            data: data.to_vec(),
        }
    }

    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.data)
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let d = PgDataRow::new(BytesMut::from(&self.data[..]), self.field_count);
        encode_to_pybytes(py, &d)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let d: PgDataRow = decode_from_slice(data)?;
        Ok(Self {
            field_count: d.field_count,
            data: d.data.to_vec(),
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "DataRow(field_count={}, data=<{} bytes>)",
            self.field_count,
            self.data.len()
        )
    }
}

// ---------- ErrorResponse ----------------------------------------------

/// Backend `'E'` message. Carries the PostgreSQL error-field set as a list
/// of (one-byte tag, string value) pairs. For a structured view, convert
/// the fields to a [`pywire.errors.ErrorInfo`].
#[pyclass(
    name = "ErrorResponse",
    module = "pywire.messages",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PyErrorResponse {
    #[pyo3(get)]
    pub fields: Vec<(u8, String)>,
}

#[pymethods]
impl PyErrorResponse {
    #[new]
    #[pyo3(signature = (fields = None))]
    fn new(fields: Option<Vec<(u8, String)>>) -> Self {
        Self {
            fields: fields.unwrap_or_default(),
        }
    }

    fn encode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let e = PgErrorResponse::new(self.fields.clone());
        encode_to_pybytes(py, &e)
    }

    #[classmethod]
    fn decode(_cls: &Bound<'_, PyType>, data: &[u8]) -> PyResult<Self> {
        let e: PgErrorResponse = decode_from_slice(data)?;
        Ok(Self { fields: e.fields })
    }

    fn __repr__(&self) -> String {
        format!("ErrorResponse(field_count={})", self.fields.len())
    }
}

// ---------- module registration ----------------------------------------

/// Register `pywire.messages` as a submodule of `_pywire`.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let m = PyModule::new(py, "messages")?;

    m.add_class::<PyTransactionStatus>()?;
    m.add_class::<PyFieldDescription>()?;
    m.add_class::<PyStartup>()?;
    m.add_class::<PyQuery>()?;
    m.add_class::<PyTerminate>()?;
    m.add_class::<PyReadyForQuery>()?;
    m.add_class::<PyCommandComplete>()?;
    m.add_class::<PyRowDescription>()?;
    m.add_class::<PyDataRow>()?;
    m.add_class::<PyErrorResponse>()?;

    parent.add_submodule(&m)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pywire._pywire.messages", &m)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_status_round_trip() {
        for s in [
            PyTransactionStatus::Idle,
            PyTransactionStatus::Transaction,
            PyTransactionStatus::Error,
        ] {
            let pg: PgTransactionStatus = s.into();
            let back: PyTransactionStatus = pg.into();
            assert_eq!(s, back);
        }
    }

    #[test]
    fn field_description_round_trip() {
        let f = PyFieldDescription {
            name: "id".into(),
            table_id: 16384,
            column_id: 1,
            type_id: 23,
            type_size: 4,
            type_modifier: -1,
            format_code: 0,
        };
        let pg: PgFieldDescription = f.clone().into();
        let back: PyFieldDescription = pg.into();
        assert_eq!(f, back);
    }

    #[test]
    fn decode_rejects_truncated_input() {
        // Empty input is not a complete frame.
        let result: PyResult<PgQuery> = decode_from_slice(&[]);
        assert!(result.is_err());
    }
}
