//! Adapter for PostgreSQL's extended query protocol.

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use futures::{Sink, SinkExt};
use pgwire::api::portal::{Format, Portal};
use pgwire::api::query::ExtendedQueryHandler as PgExtendedQueryHandler;
use pgwire::api::results::{
    DescribePortalResponse, DescribeStatementResponse, FieldInfo as PgFieldInfo, Response,
};
use pgwire::api::stmt::{QueryParser, StoredStatement};
use pgwire::api::store::{Entry, PortalStore};
use pgwire::api::{ClientInfo, ClientPortalStore, Type, DEFAULT_NAME};
use pgwire::error::{ErrorInfo, PgWireError, PgWireResult};
use pgwire::messages::extendedquery::{
    Bind, BindComplete, Close, CloseComplete, Parse, ParseComplete, Sync as PgSync,
    TARGET_TYPE_BYTE_PORTAL, TARGET_TYPE_BYTE_STATEMENT,
};
use pgwire::messages::response::{ReadyForQuery, TransactionStatus};
use pgwire::messages::PgWireBackendMessage;
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio as pyo3_tokio;

use crate::errors::py_err_to_pywire;
use crate::query::{PyFieldInfo, PyResponse};
use crate::server::PySession;

#[derive(Debug)]
pub struct PyStatement(pub Py<PyAny>);

impl Clone for PyStatement {
    // pgwire requires Clone for stored statements; its Arc-backed portal
    // store does not clone this payload in the server paths we exercise.
    // LCOV_EXCL_START
    fn clone(&self) -> Self {
        Python::attach(|py| Self(self.0.clone_ref(py)))
    }
    // LCOV_EXCL_STOP
}

#[derive(Debug)]
pub struct PyExtendedHandler {
    instance: Option<Py<PyAny>>,
}

pub(crate) struct PythonPortals {
    portals: Mutex<HashMap<String, PortalEntry>>,
    statements: Mutex<HashMap<String, Py<PyAny>>>,
    session: Mutex<Option<Py<PyAny>>>,
    locals: pyo3_async_runtimes::TaskLocals,
}

struct PortalEntry {
    statement_name: String,
    bound: Option<BoundPortal>,
}

struct BoundPortal {
    instance: Py<PyAny>,
    portal: Py<PyAny>,
}

impl PythonPortals {
    pub(crate) fn new(locals: pyo3_async_runtimes::TaskLocals) -> Self {
        Self {
            portals: Mutex::new(HashMap::new()),
            statements: Mutex::new(HashMap::new()),
            session: Mutex::new(None),
            locals,
        }
    }

    pub(crate) fn set_session(&self, instance: Py<PyAny>) {
        *self.session.lock().unwrap() = Some(instance);
    }

    pub(crate) async fn cleanup(&self) {
        let portals = std::mem::take(&mut *self.portals.lock().unwrap());
        let statements = std::mem::take(&mut *self.statements.lock().unwrap());
        let session = self.session.lock().unwrap().take();
        cleanup_entries(portals, statements, session).await;
    }
}

impl Drop for PythonPortals {
    fn drop(&mut self) {
        let portals = std::mem::take(self.portals.get_mut().unwrap());
        let statements = std::mem::take(self.statements.get_mut().unwrap());
        let session = self.session.get_mut().unwrap().take();
        if portals.is_empty() && statements.is_empty() && session.is_none() {
            return;
        }
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let locals = self.locals.clone();
            handle.spawn(pyo3_tokio::scope(locals, async move {
                cleanup_entries(portals, statements, session).await;
            }));
        } // LCOV_EXCL_LINE - closing region emitted separately by llvm-cov
    }
}

async fn cleanup_entries(
    portals: HashMap<String, PortalEntry>,
    statements: HashMap<String, Py<PyAny>>,
    session: Option<Py<PyAny>>,
) {
    for (name, entry) in portals {
        if let Some(bound) = entry.bound {
            let _ = call_python(&bound.instance, "close_portal", &name).await;
        } // LCOV_EXCL_LINE - closing region emitted separately by llvm-cov
    }
    for (name, instance) in statements {
        let _ = call_python(&instance, "close_statement", &name).await;
    }
    if let Some(instance) = session {
        Python::attach(|py| {
            let instance = instance.bind(py);
            if instance.hasattr("close").unwrap_or(false) {
                let _ = instance.call_method0("close");
            }
        });
    }
}

impl PyExtendedHandler {
    pub fn new(instance: Option<Py<PyAny>>) -> Self {
        Self { instance }
    }

    fn instance_for<C: ClientInfo>(&self, client: &C) -> Option<Py<PyAny>> {
        Python::attach(|py| {
            client
                .session_extensions()
                .get::<PySession>()
                .map(|session| session.instance.clone_ref(py))
                .or_else(|| {
                    self.instance
                        .as_ref()
                        .map(|instance| instance.clone_ref(py))
                })
        })
    }

    async fn bind_portal(
        &self,
        instance: &Py<PyAny>,
        portal: &Portal<PyStatement>,
    ) -> PgWireResult<Py<PyAny>> {
        let parameter_formats = format_codes(&portal.parameter_format, portal.parameters.len());
        let result_formats = result_format_codes(&portal.result_column_format);
        let parameters = portal
            .parameters
            .iter()
            .map(|value| value.as_ref().map(|bytes| bytes.to_vec()))
            .collect::<Vec<_>>();
        let future = Python::attach(|py| -> PyResult<_> {
            let statement = portal.statement.statement.0.clone_ref(py);
            let coroutine = instance.bind(py).call_method1(
                "bind_portal",
                (
                    portal.name.clone(),
                    statement,
                    parameters,
                    parameter_formats,
                    result_formats,
                ),
            )?; // LCOV_EXCL_LINE - defensive Python dispatch failure
            pyo3_tokio::into_future(coroutine)
        })
        .map_err(py_error)?;
        future.await.map_err(py_error)
    }

    async fn parse_statement(
        &self,
        instance: Option<&Py<PyAny>>,
        name: &str,
        sql: &str,
        type_oids: &[u32],
    ) -> PgWireResult<PyStatement> {
        let Some(instance) = instance else {
            return Err(PgWireError::ApiError(
                "extended query callback is not configured".into(),
            ));
        };
        let future = Python::attach(|py| -> PyResult<_> {
            let coroutine = instance
                .bind(py)
                .call_method1("parse_statement", (name, sql, type_oids.to_vec()))?;
            pyo3_tokio::into_future(coroutine)
        })
        .map_err(py_error)?;
        future.await.map(PyStatement).map_err(py_error)
    }

    fn portal_for<C: ClientInfo>(&self, client: &C, name: &str) -> PgWireResult<Py<PyAny>> {
        let portals = client
            .session_extensions()
            .get::<Arc<PythonPortals>>()
            .ok_or_else(|| PgWireError::PortalNotFound(name.to_owned()))?;
        let entries = portals.portals.lock().unwrap();
        Python::attach(|py| {
            entries
                .get(name)
                .and_then(|entry| entry.bound.as_ref())
                .map(|bound| bound.portal.clone_ref(py))
                .ok_or_else(|| PgWireError::PortalNotFound(name.to_owned()))
        })
    }

    pub(crate) async fn close_portal<C: ClientInfo>(
        &self,
        client: &C,
        name: &str,
    ) -> PgWireResult<()> {
        let Some(portals) = client.session_extensions().get::<Arc<PythonPortals>>() else {
            return Ok(()); // LCOV_EXCL_LINE - startup always installs connection resources
        };
        let instance = {
            let entries = portals.portals.lock().unwrap();
            Python::attach(|py| {
                entries
                    .get(name)
                    .and_then(|entry| entry.bound.as_ref())
                    .map(|bound| bound.instance.clone_ref(py))
            })
        };
        if let Some(instance) = instance {
            call_python(&instance, "close_portal", name).await?;
        }
        portals.portals.lock().unwrap().remove(name);
        Ok(())
    }

    pub(crate) async fn close_all_portals<C>(&self, client: &mut C) -> PgWireResult<()>
    where
        C: ClientInfo + ClientPortalStore,
        C::PortalStore: PortalStore,
    {
        let portal_names = client
            .session_extensions()
            .get::<Arc<PythonPortals>>()
            .map(|portals| {
                portals
                    .portals
                    .lock()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for portal_name in portal_names {
            self.close_portal(client, &portal_name).await?;
            client.portal_store().rm_portal(&portal_name);
        }
        client.portal_store().rm_portal(DEFAULT_NAME);
        Ok(())
    }

    pub(crate) async fn close_statement<C>(&self, client: &mut C, name: &str) -> PgWireResult<()>
    where
        C: ClientInfo + ClientPortalStore,
        C::PortalStore: PortalStore,
    {
        let portal_names = client
            .session_extensions()
            .get::<Arc<PythonPortals>>()
            .map(|portals| {
                portals
                    .portals
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(_, entry)| entry.statement_name == name)
                    .map(|(portal_name, _)| portal_name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for portal_name in portal_names {
            self.close_portal(client, &portal_name).await?;
            client.portal_store().rm_portal(&portal_name);
        }
        if matches!(
            client.portal_store().get_statement(name),
            Some(Entry::Value(_))
        ) {
            // A stored Python statement always has a configured callback.
            // LCOV_EXCL_START
            if let Some(instance) = self.instance_for(client) {
                call_python(&instance, "close_statement", name).await?;
            }
            // LCOV_EXCL_STOP
        }
        if let Some(portals) = client.session_extensions().get::<Arc<PythonPortals>>() {
            portals.statements.lock().unwrap().remove(name);
        }
        client.portal_store().rm_statement(name);
        Ok(())
    }
}

async fn call_python(instance: &Py<PyAny>, method: &str, name: &str) -> PgWireResult<()> {
    let future = Python::attach(|py| -> PyResult<_> {
        let coroutine = instance.bind(py).call_method1(method, (name,))?;
        pyo3_tokio::into_future(coroutine)
    })
    .map_err(py_error)?;
    future.await.map_err(py_error)?;
    Ok(())
}

fn py_error(error: PyErr) -> PgWireError {
    Python::attach(|py| py_err_to_pywire(py, error))
}

fn format_codes(format: &Format, len: usize) -> Vec<i16> {
    match format {
        Format::UnifiedText => vec![0; len],
        Format::UnifiedBinary => vec![1; len],
        Format::Individual(codes) => codes.clone(),
    }
}

fn result_format_codes(format: &Format) -> Vec<i16> {
    match format {
        Format::Individual(codes) => codes.clone(),
        Format::UnifiedBinary => vec![1],
        Format::UnifiedText => vec![],
    }
}

fn pg_type(oid: u32) -> Type {
    if oid == 0 {
        return Type::UNKNOWN;
    }
    Type::from_oid(oid).unwrap_or_else(|| {
        Type::new(
            format!("oid{oid}"),
            oid,
            postgres_types::Kind::Simple,
            "pg_catalog".to_owned(),
        )
    })
}

fn is_empty_query(sql: &str) -> bool {
    let mut chars = sql.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_whitespace() || ch == ';' {
            continue;
        }
        if ch == '-' && chars.next_if_eq(&'-').is_some() {
            for comment_ch in chars.by_ref() {
                if comment_ch == '\n' || comment_ch == '\r' {
                    break;
                }
            }
            continue;
        }
        if ch == '/' && chars.next_if_eq(&'*').is_some() {
            let mut depth = 1;
            while let Some(comment_ch) = chars.next() {
                if comment_ch == '/' && chars.next_if_eq(&'*').is_some() {
                    depth += 1;
                } else if comment_ch == '*' && chars.next_if_eq(&'/').is_some() {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            if depth != 0 {
                return false;
            }
            continue;
        }
        return false;
    }
    true
}

#[async_trait]
impl QueryParser for PyExtendedHandler {
    type Statement = PyStatement;

    // The server overrides on_parse and both describe callbacks so it can
    // preserve Python's async API and the statement name. pgwire still
    // requires an associated QueryParser; these methods are unreachable.
    // LCOV_EXCL_START
    async fn parse_sql<C>(
        &self,
        _client: &C,
        sql: &str,
        types: &[Option<Type>],
    ) -> PgWireResult<Option<Self::Statement>>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
        let type_oids = types
            .iter()
            .map(|value| value.as_ref().map_or(0, Type::oid))
            .collect::<Vec<_>>();
        self.parse_statement(self.instance.as_ref(), "", sql, &type_oids)
            .await
            .map(Some)
    }

    fn get_parameter_types(&self, _stmt: &Self::Statement) -> PgWireResult<Vec<Type>> {
        Ok(vec![])
    }

    fn get_result_schema(
        &self,
        _stmt: &Self::Statement,
        _column_format: Option<&Format>,
    ) -> PgWireResult<Vec<PgFieldInfo>> {
        Ok(vec![])
    }
    // LCOV_EXCL_STOP
}

#[async_trait]
impl PgExtendedQueryHandler for PyExtendedHandler {
    type Statement = PyStatement;
    type QueryParser = Self;

    // LCOV_EXCL_START
    fn query_parser(&self) -> Arc<Self::QueryParser> {
        // The handler is stored in an Arc by the server. Cloning Python
        // references here is safe and keeps the parser lifetime independent.
        Arc::new(Python::attach(|py| Self {
            instance: self.instance.as_ref().map(|value| value.clone_ref(py)),
        }))
    }
    // LCOV_EXCL_STOP

    async fn on_parse<C>(&self, client: &mut C, message: Parse) -> PgWireResult<()>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement = Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let name = message
            .name
            .clone()
            .unwrap_or_else(|| DEFAULT_NAME.to_owned());
        let types = message
            .type_oids
            .iter()
            .map(|oid| if *oid == 0 { None } else { Some(pg_type(*oid)) })
            .collect::<Vec<_>>();
        if name == DEFAULT_NAME {
            self.close_statement(client, &name).await?;
        } else if client.portal_store().get_statement(&name).is_some() {
            return Err(PgWireError::UserError(Box::new(ErrorInfo::new(
                "ERROR".to_owned(),
                "42P05".to_owned(),
                format!("prepared statement {name:?} already exists"),
            ))));
        }
        if is_empty_query(&message.query) {
            client.portal_store().put_empty_statement(&name);
        } else {
            let instance = self.instance_for(client);
            let statement = self
                .parse_statement(instance.as_ref(), &name, &message.query, &message.type_oids)
                .await?;
            let instance = instance.expect("parsed statements have a Python handler");
            client
                .session_extensions()
                .get::<Arc<PythonPortals>>()
                .expect("startup installed connection resources")
                .statements
                .lock()
                .unwrap()
                .insert(name.clone(), instance);
            client
                .portal_store()
                .put_statement(Arc::new(StoredStatement::new(name, statement, types)));
        }
        client
            .send(PgWireBackendMessage::ParseComplete(ParseComplete::new()))
            .await?;
        Ok(())
    }

    async fn on_bind<C>(&self, client: &mut C, message: Bind) -> PgWireResult<()>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement = Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let statement_name = message.statement_name.as_deref().unwrap_or(DEFAULT_NAME);
        let portal_name = message.portal_name.as_deref().unwrap_or(DEFAULT_NAME);
        if portal_name != DEFAULT_NAME && client.portal_store().get_portal(portal_name).is_some() {
            return Err(PgWireError::UserError(Box::new(ErrorInfo::new(
                "ERROR".to_owned(),
                "42P03".to_owned(),
                format!("portal {portal_name:?} already exists"),
            ))));
        }
        let format_count = message.parameter_format_codes.len();
        if format_count > 1 && format_count != message.parameters.len() {
            return Err(PgWireError::UserError(Box::new(ErrorInfo::new(
                "ERROR".to_owned(),
                "08P01".to_owned(),
                format!(
                    "bind message supplies {format_count} parameter format codes for {} parameters",
                    message.parameters.len()
                ),
            ))));
        }
        if let Some(code) = message
            .parameter_format_codes
            .iter()
            .chain(&message.result_column_format_codes)
            .find(|code| **code != 0 && **code != 1)
        {
            return Err(PgWireError::UserError(Box::new(ErrorInfo::new(
                "ERROR".to_owned(),
                "08P01".to_owned(),
                format!("bind message has invalid format code {code}"),
            ))));
        }
        match client.portal_store().get_statement(statement_name) {
            Some(Entry::Value(statement)) => {
                let portal = Portal::try_new(&message, statement)?;
                // A non-empty statement only enters the store after Python Parse.
                // LCOV_EXCL_START
                let instance = self.instance_for(client).ok_or_else(|| {
                    PgWireError::ApiError("extended query callback is not configured".into())
                })?;
                // LCOV_EXCL_STOP
                self.close_portal(client, portal_name).await?;
                client.portal_store().rm_portal(portal_name);
                let py_portal = self.bind_portal(&instance, &portal).await?;
                client
                    .session_extensions()
                    .get::<Arc<PythonPortals>>()
                    .expect("startup installed connection resources")
                    .portals
                    .lock()
                    .unwrap()
                    .insert(
                        portal_name.to_owned(),
                        PortalEntry {
                            statement_name: statement_name.to_owned(),
                            bound: Some(BoundPortal {
                                instance,
                                portal: py_portal,
                            }),
                        },
                    );
                client.portal_store().put_portal(Arc::new(portal));
            }
            Some(Entry::Empty) => {
                if message.result_column_format_codes.len() > 1 {
                    return Err(PgWireError::UserError(Box::new(ErrorInfo::new(
                        "ERROR".to_owned(),
                        "08P01".to_owned(),
                        format!(
                            "bind message supplies {} result format codes for 0 columns",
                            message.result_column_format_codes.len()
                        ),
                    ))));
                }
                if !message.parameters.is_empty() {
                    return Err(PgWireError::UserError(Box::new(ErrorInfo::new(
                        "ERROR".to_owned(), "08P01".to_owned(),
                        format!("bind message supplies {} parameters, but prepared statement {statement_name:?} requires 0", message.parameters.len()),
                    ))));
                }
                self.close_portal(client, portal_name).await?;
                client.portal_store().rm_portal(portal_name);
                client
                    .session_extensions()
                    .get::<Arc<PythonPortals>>()
                    .expect("startup installed connection resources")
                    .portals
                    .lock()
                    .unwrap()
                    .insert(
                        portal_name.to_owned(),
                        PortalEntry {
                            statement_name: statement_name.to_owned(),
                            bound: None,
                        },
                    );
                client.portal_store().put_empty_portal(portal_name);
            }
            None => return Err(PgWireError::StatementNotFound(statement_name.to_owned())),
        }
        client
            .send(PgWireBackendMessage::BindComplete(BindComplete::new()))
            .await?;
        Ok(())
    }

    async fn on_sync<C>(&self, client: &mut C, _message: PgSync) -> PgWireResult<()>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement = Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        if matches!(client.transaction_status(), TransactionStatus::Idle) {
            self.close_all_portals(client).await?;
        }
        client
            .send(PgWireBackendMessage::ReadyForQuery(ReadyForQuery::new(
                client.transaction_status(),
            )))
            .await?;
        client.flush().await?;
        Ok(())
    }

    async fn on_close<C>(&self, client: &mut C, message: Close) -> PgWireResult<()>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement = Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let name = message.name.as_deref().unwrap_or(DEFAULT_NAME);
        match message.target_type {
            TARGET_TYPE_BYTE_STATEMENT => {
                self.close_statement(client, name).await?;
            }
            TARGET_TYPE_BYTE_PORTAL => {
                self.close_portal(client, name).await?;
                client.portal_store().rm_portal(name);
            }
            _ => return Err(PgWireError::InvalidTargetType(message.target_type)),
        }
        client
            .send(PgWireBackendMessage::CloseComplete(CloseComplete::new()))
            .await?;
        Ok(())
    }

    async fn do_describe_statement<C>(
        &self,
        client: &mut C,
        target: &StoredStatement<Self::Statement>,
    ) -> PgWireResult<DescribeStatementResponse>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement = Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let Some(instance) = self.instance_for(client) else {
            return Ok(DescribeStatementResponse::new(vec![], vec![]));
        };
        let future = Python::attach(|py| -> PyResult<_> {
            let statement = target.statement.0.clone_ref(py);
            let coroutine = instance
                .bind(py)
                .call_method1("describe_statement", (statement,))?;
            pyo3_tokio::into_future(coroutine)
        })
        .map_err(py_error)?;
        let result = future.await.map_err(py_error)?;
        Python::attach(|py| -> PyResult<_> {
            let result = result.bind(py);
            let parameters = result
                .getattr("parameter_types")?
                .extract::<Vec<u32>>()?
                .into_iter()
                .map(pg_type)
                .collect();
            let fields = result
                .getattr("fields")?
                .extract::<Vec<PyFieldInfo>>()?
                .into_iter()
                .map(Into::into)
                .collect();
            Ok(DescribeStatementResponse::new(parameters, fields))
        })
        .map_err(py_error)
    }

    async fn do_describe_portal<C>(
        &self,
        client: &mut C,
        target: &Portal<Self::Statement>,
    ) -> PgWireResult<DescribePortalResponse>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement = Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let Some(instance) = self.instance_for(client) else {
            return Ok(DescribePortalResponse::new(vec![]));
        };
        let portal = self.portal_for(client, &target.name)?;
        let future = Python::attach(|py| -> PyResult<_> {
            let coroutine = instance
                .bind(py)
                .call_method1("describe_portal", (portal,))?;
            pyo3_tokio::into_future(coroutine)
        })
        .map_err(py_error)?;
        let result = future.await.map_err(py_error)?;
        Python::attach(|py| -> PyResult<_> {
            let fields = result
                .bind(py)
                .getattr("fields")?
                .extract::<Vec<PyFieldInfo>>()?
                .into_iter()
                .map(Into::into)
                .collect();
            Ok(DescribePortalResponse::new(fields))
        })
        .map_err(py_error)
    }

    async fn do_query<C>(
        &self,
        client: &mut C,
        portal: &Portal<Self::Statement>,
        max_rows: usize,
    ) -> PgWireResult<Response>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement = Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        if let Some(instance) = self.instance_for(client) {
            let py_portal = self.portal_for(client, &portal.name)?;
            let future = Python::attach(|py| -> PyResult<_> {
                let coroutine = instance
                    .bind(py)
                    .call_method1("do_query", (py_portal, max_rows))?;
                pyo3_tokio::into_future(coroutine)
            })
            .map_err(py_error)?;
            let result = future.await.map_err(py_error)?;
            let response = Python::attach(|py| -> PyResult<PyResponse> {
                let response = result.bind(py).extract::<Py<PyResponse>>()?;
                let cloned = response.borrow(py).clone();
                Ok(cloned)
            })
            .map_err(py_error)?;
            return Ok(response.into_pg());
        }

        Err(PgWireError::ApiError(
            "extended query callback is not configured".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_code_helpers_cover_all_pgwire_shapes() {
        assert_eq!(format_codes(&Format::UnifiedText, 2), vec![0, 0]);
        assert_eq!(format_codes(&Format::UnifiedBinary, 2), vec![1, 1]);
        assert_eq!(format_codes(&Format::Individual(vec![0, 1]), 2), vec![0, 1]);
        assert!(result_format_codes(&Format::UnifiedText).is_empty());
        assert_eq!(result_format_codes(&Format::UnifiedBinary), vec![1]);
        assert_eq!(
            result_format_codes(&Format::Individual(vec![1, 0])),
            vec![1, 0]
        );
        assert_eq!(pg_type(23), Type::INT4);
        assert_eq!(pg_type(0), Type::UNKNOWN);
        assert_eq!(pg_type(u32::MAX).oid(), u32::MAX);
    }

    #[test]
    fn empty_query_recognizes_comments() {
        for sql in [
            "",
            "; ;",
            "-- line",
            "/* block */",
            "; /* outer /* inner */ end */ -- tail",
        ] {
            assert!(is_empty_query(sql), "{sql:?}");
        }
        for sql in [
            "SELECT 1",
            "/* incomplete",
            "-- comment\nSELECT 1",
            "-- comment\rSELECT 1",
            "/* comment */ SELECT 1",
            "\u{00a0}",
        ] {
            assert!(!is_empty_query(sql), "{sql:?}");
        }
    }
}
