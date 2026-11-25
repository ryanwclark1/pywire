//! COPY FROM STDIN: pgwire's `CopyHandler` backed by Python.
//!
//! pgwire splits COPY across two hooks: the query handler returns
//! `Response::CopyIn`, then the server loop passes each `CopyData`,
//! `CopyDone` or `CopyFail` message to the server-wide `CopyHandler` along
//! with the client. pywire carries the Python receiver on the response
//! (`Response.copy_in(handler, ...)`) and parks it in the connection's
//! session extensions, so concurrent connections never share COPY state.
//!
//! On `CopyDone`, the handler's `on_copy_done()` returns the row count and
//! this module sends the `COPY n` `CommandComplete`, as pgwire's contract
//! requires; pgwire then sends `ReadyForQuery`.

use std::fmt::Debug;
use std::sync::Mutex;

use async_trait::async_trait;
use futures::sink::{Sink, SinkExt};
use pgwire::api::copy::CopyHandler;
use pgwire::api::results::Tag;
use pgwire::api::ClientInfo;
use pgwire::error::{ErrorInfo, PgWireError, PgWireResult};
use pgwire::messages::copy::{CopyData, CopyDone, CopyFail};
use pgwire::messages::PgWireBackendMessage;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString, PyTuple};
use pyo3_async_runtimes::tokio as pyo3_tokio;

use crate::errors::py_err_to_pywire;

/// The Python receiver of the COPY FROM in progress on a connection.
#[derive(Default)]
struct ActiveCopy(Mutex<Option<Py<PyAny>>>);

/// Make `handler` the receiver for the COPY FROM that `client` is starting.
pub fn begin_copy_in<C: ClientInfo + ?Sized>(client: &C, handler: Py<PyAny>) {
    let active = client
        .session_extensions()
        .get_or_insert_with(ActiveCopy::default);
    *active.0.lock().unwrap() = Some(handler);
}

fn current<C: ClientInfo + ?Sized>(client: &C) -> Option<Py<PyAny>> {
    let active = client.session_extensions().get::<ActiveCopy>()?;
    let guard = active.0.lock().unwrap();
    Python::attach(|py| guard.as_ref().map(|handler| handler.clone_ref(py)))
}

fn finish<C: ClientInfo + ?Sized>(client: &C) -> Option<Py<PyAny>> {
    client
        .session_extensions()
        .get::<ActiveCopy>()?
        .0
        .lock()
        .unwrap()
        .take()
}

/// Await `handler.<method>(*args)` and return its result.
async fn call(handler: &Py<PyAny>, method: &str, args: Vec<Py<PyAny>>) -> PyResult<Py<PyAny>> {
    let future = Python::attach(|py| {
        let args = PyTuple::new(py, args)?;
        pyo3_tokio::into_future(handler.bind(py).call_method1(method, args)?)
    })?;
    future.await
}

fn no_copy_in_progress() -> PgWireError {
    PgWireError::UserError(Box::new(ErrorInfo::new(
        "ERROR".to_owned(),
        "08P01".to_owned(),
        "COPY data received with no COPY FROM STDIN in progress".to_owned(),
    )))
}

/// pgwire `CopyHandler` that dispatches to the connection's Python receiver.
#[derive(Debug, Default)]
pub struct PyCopyHandler;

#[async_trait]
impl CopyHandler for PyCopyHandler {
    async fn on_copy_data<C>(&self, client: &mut C, copy_data: CopyData) -> PgWireResult<()>
    where
        C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let handler = current(client).ok_or_else(no_copy_in_progress)?;
        let chunk = Python::attach(|py| PyBytes::new(py, &copy_data.data).into_any().unbind());
        if let Err(err) = call(&handler, "on_copy_data", vec![chunk]).await {
            // pgwire reports the error and leaves COPY mode; forget the
            // receiver so a stale one is never reused.
            finish(client);
            return Err(Python::attach(|py| py_err_to_pywire(py, err)));
        }
        Ok(())
    }

    async fn on_copy_done<C>(&self, client: &mut C, _done: CopyDone) -> PgWireResult<()>
    where
        C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let handler = finish(client).ok_or_else(no_copy_in_progress)?;
        let rows = call(&handler, "on_copy_done", Vec::new())
            .await
            .and_then(|rows| Python::attach(|py| rows.bind(py).extract::<usize>()))
            .map_err(|err| Python::attach(|py| py_err_to_pywire(py, err)))?;
        let tag = Tag::new("COPY").with_rows(rows);
        client
            .send(PgWireBackendMessage::CommandComplete(tag.into()))
            .await?;
        Ok(())
    }

    async fn on_copy_fail<C>(&self, client: &mut C, fail: CopyFail) -> PgWireError
    where
        C: ClientInfo + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        if let Some(handler) = finish(client) {
            // The COPY is failing either way; a cleanup error must not mask
            // the client's message.
            let message = Python::attach(|py| PyString::new(py, &fail.message).into_any().unbind());
            let _ = call(&handler, "on_copy_fail", vec![message]).await;
        }
        // Same error as pgwire's default `on_copy_fail`.
        PgWireError::UserError(Box::new(ErrorInfo::new(
            "ERROR".to_owned(),
            "XX000".to_owned(),
            format!("COPY IN mode terminated by the user: {}", fail.message),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stray_copy_messages_get_a_protocol_violation() {
        // pgwire only routes COPY messages here while a COPY is active, so
        // this guard is defensive; pin its SQLSTATE anyway.
        assert!(matches!(
            no_copy_in_progress(),
            PgWireError::UserError(info) if info.code == "08P01"
        ));
    }
}
