//! Adapter for PostgreSQL's extended query protocol.

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{Sink, SinkExt};
use pgwire::api::portal::{Format, Portal};
use pgwire::api::query::ExtendedQueryHandler as PgExtendedQueryHandler;
use pgwire::api::results::{
    DescribePortalResponse, DescribeStatementResponse, FieldInfo as PgFieldInfo, Response,
};
use pgwire::api::stmt::{QueryParser, StoredStatement};
use pgwire::api::store::PortalStore;
use pgwire::api::{ClientInfo, ClientPortalStore, Type, DEFAULT_NAME};
use pgwire::error::{PgWireError, PgWireResult};
use pgwire::messages::extendedquery::{Parse, ParseComplete};
use pgwire::messages::PgWireBackendMessage;
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio as pyo3_tokio;

use crate::errors::py_err_to_pywire;
use crate::query::{PyFieldInfo, PyResponse};

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

impl PyExtendedHandler {
    pub fn new(instance: Option<Py<PyAny>>) -> Self {
        Self { instance }
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
        name: &str,
        sql: &str,
        types: &[Option<Type>],
    ) -> PgWireResult<PyStatement> {
        let Some(instance) = &self.instance else {
            return Err(PgWireError::ApiError(
                "extended query callback is not configured".into(),
            ));
        };
        let type_oids = types
            .iter()
            .map(|value| value.as_ref().map_or(0, Type::oid))
            .collect::<Vec<_>>();
        let future = Python::attach(|py| -> PyResult<_> {
            let coroutine = instance
                .bind(py)
                .call_method1("parse_statement", (name, sql, type_oids))?;
            pyo3_tokio::into_future(coroutine)
        })
        .map_err(py_error)?;
        future.await.map(PyStatement).map_err(py_error)
    }
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
    Type::from_oid(oid).unwrap_or(Type::UNKNOWN)
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
        assert_eq!(pg_type(u32::MAX), Type::UNKNOWN);
    }
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
    ) -> PgWireResult<Self::Statement>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
        self.parse_statement("", sql, types).await
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
            .map(|oid| Type::from_oid(*oid))
            .collect::<Vec<_>>();
        let statement = self.parse_statement(&name, &message.query, &types).await?;
        client
            .portal_store()
            .put_statement(Arc::new(StoredStatement::new(name, statement, types)));
        client
            .send(PgWireBackendMessage::ParseComplete(ParseComplete::new()))
            .await?;
        Ok(())
    }

    async fn do_describe_statement<C>(
        &self,
        _client: &mut C,
        target: &StoredStatement<Self::Statement>,
    ) -> PgWireResult<DescribeStatementResponse>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement = Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let Some(instance) = &self.instance else {
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
        _client: &mut C,
        target: &Portal<Self::Statement>,
    ) -> PgWireResult<DescribePortalResponse>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement = Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        if self.instance.is_none() {
            return Ok(DescribePortalResponse::new(vec![]));
        }
        let instance = self.instance.as_ref().expect("checked above");
        let portal = self.bind_portal(instance, target).await?;
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
        _client: &mut C,
        portal: &Portal<Self::Statement>,
        max_rows: usize,
    ) -> PgWireResult<Response>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement = Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        if let Some(instance) = &self.instance {
            let py_portal = self.bind_portal(instance, portal).await?;
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
