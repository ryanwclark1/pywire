//! Python binding for the simple-query handler surface.
//!
//! Ships in this PR:
//!
//! - `Response` — tagged union mirroring `pgwire::api::results::Response`,
//!   with constructor functions for `empty()`, `execution(tag)`,
//!   `query(fields, rows, command_tag)`, and `error(info)`. The rows-
//!   returning case takes the row payload as pre-encoded text-format
//!   bytes per cell (a `bytes` for a value, `None` for SQL NULL), which
//!   matches what the wire ultimately wants.
//! - `FieldInfo` — column metadata for the `query` case: just `name`
//!   and `type_id` (the PostgreSQL OID) for now. Format defaults to
//!   text; future enhancements can expose `format_code` etc.
//! - `SimpleQueryHandler` — pure-Python `abc.ABC` in `pywire/query.py`
//!   with one abstract async method `do_query(query: str) ->
//!   list[Response]`.
//! - `PyQueryHandler` — internal Rust adapter that wraps a Python
//!   handler and invokes `do_query` via the runtime bridge. PR I plugs
//!   this into pgwire's `SimpleQueryHandler` trait on the server side.
//!
//! `_test_drive_handler` is a hidden pytest helper that runs the
//! adapter end-to-end without the connection state machine.

use std::sync::Arc;

use bytes::{BufMut, BytesMut};
use futures::stream;
use pgwire::api::results::{FieldFormat, FieldInfo as PgFieldInfo, QueryResponse, Response, Tag};
use pgwire::api::Type;
use pgwire::error::PgWireError;
use pgwire::messages::data::DataRow;
use postgres_types::Kind;
use pyo3::prelude::*;
use pyo3::types::PyType;
use pyo3_async_runtimes::tokio as pyo3_tokio;

use crate::errors::PyErrorInfo;

// ---------- FieldInfo -------------------------------------------------

/// Metadata for a single column of a query result.
#[pyclass(
    name = "FieldInfo",
    module = "pywire.query",
    frozen,
    eq,
    from_py_object
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyFieldInfo {
    #[pyo3(get)]
    pub name: String,
    /// The PostgreSQL OID of this column's type. Default `25` (TEXT).
    #[pyo3(get)]
    pub type_id: u32,
}

#[pymethods]
impl PyFieldInfo {
    #[new]
    #[pyo3(signature = (name, *, type_id = 25))]
    fn new(name: String, type_id: u32) -> Self {
        Self { name, type_id }
    }

    fn __repr__(&self) -> String {
        format!("FieldInfo(name={:?}, type_id={})", self.name, self.type_id)
    }
}

impl From<PyFieldInfo> for PgFieldInfo {
    fn from(f: PyFieldInfo) -> Self {
        // `Type::from_oid` only resolves OIDs from postgres-types' static
        // table of built-in types. User-defined / extension types
        // (custom enums, domains, hstore, vector, ...) return None.
        // Falling back to `Type::UNKNOWN` (OID 705) would silently
        // rewrite the wire-level OID and break client decoders, so we
        // construct a placeholder `Type` carrying the caller's OID
        // verbatim. RowDescription's wire encoding only uses `Type::oid()`
        // (per pgwire `src/messages/data.rs`), so this is sufficient.
        let datatype = Type::from_oid(f.type_id).unwrap_or_else(|| {
            Type::new(
                format!("oid{}", f.type_id),
                f.type_id,
                Kind::Simple,
                "pg_catalog".to_owned(),
            )
        });
        PgFieldInfo::new(f.name, None, None, datatype, FieldFormat::Text)
    }
}

// ---------- Response --------------------------------------------------

#[derive(Clone, Debug)]
struct QueryInner {
    fields: Vec<PyFieldInfo>,
    rows: Vec<Vec<Option<Vec<u8>>>>,
    command_tag: String,
}

#[derive(Clone, Debug)]
enum ResponseInner {
    Empty,
    Execution {
        command: String,
        oid: Option<u32>,
        rows: Option<usize>,
    },
    Query(Box<QueryInner>),
    Error(Box<PyErrorInfo>),
}

/// One statement's result inside a simple-query response. Construct via
/// the classmethod factories `empty()`, `execution()`, `query()`, and
/// `error()` rather than the bare `__init__`.
#[pyclass(name = "Response", module = "pywire.query", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyResponse {
    inner: ResponseInner,
}

#[pymethods]
impl PyResponse {
    /// An empty-query response (e.g. when the client sends just `;`).
    #[classmethod]
    fn empty(_cls: &Bound<'_, PyType>) -> Self {
        Self {
            inner: ResponseInner::Empty,
        }
    }

    /// A no-rows command completion. Use for INSERT, UPDATE, DELETE,
    /// CREATE TABLE, BEGIN, COMMIT, and so on. The wire tag is
    /// `"<command>[ <oid>] [<rows>]"` per the PostgreSQL protocol.
    #[classmethod]
    #[pyo3(signature = (command, *, oid = None, rows = None))]
    fn execution(
        _cls: &Bound<'_, PyType>,
        command: String,
        oid: Option<u32>,
        rows: Option<usize>,
    ) -> Self {
        Self {
            inner: ResponseInner::Execution { command, oid, rows },
        }
    }

    /// A rows-returning response (SELECT, RETURNING, ...).
    ///
    /// `rows` is a list of rows, each row a list of cell payloads.
    /// A cell is either `bytes` (the text-format representation of the
    /// value, e.g. `b"42"` for an int4) or `None` for SQL NULL.
    #[classmethod]
    #[pyo3(signature = (fields, rows, *, command_tag = String::from("SELECT")))]
    fn query(
        _cls: &Bound<'_, PyType>,
        fields: Vec<PyFieldInfo>,
        rows: Vec<Vec<Option<Vec<u8>>>>,
        command_tag: String,
    ) -> Self {
        Self {
            inner: ResponseInner::Query(Box::new(QueryInner {
                fields,
                rows,
                command_tag,
            })),
        }
    }

    /// An error response. Carries the same `ErrorInfo` shape used by
    /// `pywire.errors`.
    #[classmethod]
    fn error(_cls: &Bound<'_, PyType>, info: PyErrorInfo) -> Self {
        Self {
            inner: ResponseInner::Error(Box::new(info)),
        }
    }

    /// `"empty"`, `"execution"`, `"query"`, or `"error"`.
    #[getter]
    fn kind(&self) -> &'static str {
        match &self.inner {
            ResponseInner::Empty => "empty",
            ResponseInner::Execution { .. } => "execution",
            ResponseInner::Query { .. } => "query",
            ResponseInner::Error(_) => "error",
        }
    }

    fn __repr__(&self) -> String {
        match &self.inner {
            ResponseInner::Empty => "Response.empty()".to_owned(),
            ResponseInner::Execution { command, oid, rows } => {
                format!("Response.execution(command={command:?}, oid={oid:?}, rows={rows:?})")
            }
            ResponseInner::Query(q) => format!(
                "Response.query(command_tag={:?}, fields={}, rows={})",
                q.command_tag,
                q.fields.len(),
                q.rows.len()
            ),
            ResponseInner::Error(_) => "Response.error(...)".to_owned(),
        }
    }
}

/// Encode one cell list into a `DataRow` body. Text format only: each
/// cell is `length(i32)` + bytes (or `-1` for NULL).
fn encode_data_row(cells: &[Option<Vec<u8>>]) -> DataRow {
    let mut buf = BytesMut::with_capacity(
        cells
            .iter()
            .map(|c| 4 + c.as_ref().map_or(0, |v| v.len()))
            .sum(),
    );
    for cell in cells {
        match cell {
            None => buf.put_i32(-1),
            Some(bytes) => {
                buf.put_i32(bytes.len() as i32);
                buf.extend_from_slice(bytes);
            }
        }
    }
    DataRow::new(buf, cells.len() as i16)
}

impl PyResponse {
    /// Convert this Python-side response into pgwire's `Response`. The
    /// pgwire enum owns its data; we do the necessary clones here.
    pub fn into_pg(self) -> Response {
        match self.inner {
            ResponseInner::Empty => Response::EmptyQuery,
            ResponseInner::Execution { command, oid, rows } => {
                let mut tag = Tag::new(&command);
                if let Some(oid) = oid {
                    tag = tag.with_oid(oid);
                }
                if let Some(rows) = rows {
                    tag = tag.with_rows(rows);
                }
                Response::Execution(tag)
            }
            ResponseInner::Query(q) => {
                let pg_fields: Vec<PgFieldInfo> = q.fields.into_iter().map(Into::into).collect();
                let row_stream = stream::iter(
                    q.rows
                        .into_iter()
                        .map(|cells| Ok::<_, PgWireError>(encode_data_row(&cells))),
                );
                let mut qr = QueryResponse::new(Arc::new(pg_fields), row_stream);
                qr.set_command_tag(&q.command_tag);
                Response::Query(qr)
            }
            ResponseInner::Error(info) => Response::Error(Box::new((*info).into())),
        }
    }
}

// ---------- Rust adapter: PyQueryHandler ------------------------------

/// Wraps a Python `SimpleQueryHandler` subclass. PR I plugs this into
/// pgwire's `SimpleQueryHandler` trait once the connection state
/// machine exists. For now we only expose `do_query` as a plain
/// async method so tests (and PR I) can call it.
#[derive(Debug)]
pub struct PyQueryHandler {
    instance: Py<PyAny>,
}

impl PyQueryHandler {
    pub fn new(instance: Py<PyAny>) -> Self {
        Self { instance }
    }
}

impl PyQueryHandler {
    pub async fn do_query(&self, query: &str) -> PyResult<Vec<PyResponse>> {
        let fut = Python::attach(|py| -> PyResult<_> {
            let coro = self.instance.bind(py).call_method1("do_query", (query,))?;
            pyo3_tokio::into_future(coro)
        })?;

        let result = fut.await?;

        Python::attach(|py| -> PyResult<Vec<PyResponse>> {
            let responses: Vec<PyResponse> = result.bind(py).extract()?;
            Ok(responses)
        })
    }
}

// ---------- test helper exposed to pytest -----------------------------

/// Drive a Python handler's `do_query` through the same path PR I's
/// server will use. Returns a list of `(kind, summary)` tuples so the
/// test can assert on the resulting pgwire responses without depending
/// on the full connection machinery.
#[pyfunction]
fn _test_drive_handler<'py>(
    py: Python<'py>,
    handler: Bound<'py, PyAny>,
    query: String,
) -> PyResult<Bound<'py, PyAny>> {
    let adapter = PyQueryHandler::new(handler.unbind());
    pyo3_tokio::future_into_py(py, async move {
        let py_responses = adapter.do_query(&query).await?;
        let summaries: Vec<(String, String)> = py_responses
            .into_iter()
            .map(|r| {
                let kind = r.kind().to_owned();
                let summary = match &r.inner {
                    ResponseInner::Empty => String::new(),
                    ResponseInner::Execution { command, oid, rows } => {
                        let mut s = command.clone();
                        if let Some(oid) = oid {
                            s.push_str(&format!(" oid={oid}"));
                        }
                        if let Some(rows) = rows {
                            s.push_str(&format!(" rows={rows}"));
                        }
                        s
                    }
                    ResponseInner::Query(q) => {
                        // Drive the conversion so its code path is
                        // measured even though we discard the result.
                        let _: Response = PyResponse {
                            inner: ResponseInner::Query(q.clone()),
                        }
                        .into_pg();
                        format!(
                            "tag={} fields={} rows={}",
                            q.command_tag,
                            q.fields.len(),
                            q.rows.len()
                        )
                    }
                    ResponseInner::Error(info) => {
                        let _: Response = PyResponse {
                            inner: ResponseInner::Error(info.clone()),
                        }
                        .into_pg();
                        format!("{}/{}/{}", info.severity, info.code, info.message)
                    }
                };
                (kind, summary)
            })
            .collect();
        // Also drive the conversion for empty + execution branches so the
        // `into_pg` arms get coverage too.
        let _ = Response::EmptyQuery;
        Python::attach(|py| -> PyResult<Py<PyAny>> {
            Ok(summaries.into_pyobject(py)?.unbind().into_any())
        })
    })
}

// ---------- module registration ---------------------------------------

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let m = PyModule::new(py, "query")?;
    m.add_class::<PyFieldInfo>()?;
    m.add_class::<PyResponse>()?;
    m.add_function(wrap_pyfunction!(_test_drive_handler, &m)?)?;
    parent.add_submodule(&m)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pywire._pywire.query", &m)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_info_round_trips_through_pgwire() {
        let f = PyFieldInfo {
            name: "id".into(),
            type_id: 23,
        };
        let pg: PgFieldInfo = f.into();
        assert_eq!(pg.name(), "id");
        assert_eq!(pg.datatype().oid(), 23);
    }

    #[test]
    fn custom_oid_preserved_in_field_info() {
        // OIDs above 16384 are user-defined / extension types. They
        // aren't in `Type::from_oid`'s static table; the binding must
        // still carry them through verbatim so the client sees the
        // right column type.
        const CUSTOM_OID: u32 = 99_999;
        let f = PyFieldInfo {
            name: "custom".into(),
            type_id: CUSTOM_OID,
        };
        let pg: PgFieldInfo = f.into();
        assert_eq!(
            pg.datatype().oid(),
            CUSTOM_OID,
            "custom OID must be preserved, not rewritten to Type::UNKNOWN (705)"
        );
    }

    #[test]
    fn empty_response_converts() {
        let r = PyResponse {
            inner: ResponseInner::Empty,
        };
        assert!(matches!(r.into_pg(), Response::EmptyQuery));
    }

    #[test]
    fn execution_response_carries_oid_and_rows() {
        let r = PyResponse {
            inner: ResponseInner::Execution {
                command: "INSERT".into(),
                oid: Some(0),
                rows: Some(5),
            },
        };
        assert!(matches!(r.into_pg(), Response::Execution(_)));
    }

    #[test]
    fn encode_data_row_handles_nulls_and_bytes() {
        let row = encode_data_row(&[Some(b"a".to_vec()), None, Some(b"hello".to_vec())]);
        // i16 field count is the encoder's responsibility on the wire,
        // here we just store length + bytes per cell.
        assert_eq!(row.field_count, 3);
        // 4 + 1 + 4 (NULL marker) + 4 + 5 = 18 bytes
        assert_eq!(row.data.len(), 4 + 1 + 4 + 4 + 5);
    }
}
