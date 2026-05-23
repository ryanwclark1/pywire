//! Async-runtime infrastructure shared by every async-using binding.
//!
//! pgwire is built on tokio. Python's standard concurrency runtime is
//! asyncio. This module owns a single multi-threaded tokio runtime for
//! the lifetime of the extension module and provides the two bridging
//! primitives every future binding needs:
//!
//! - [`future_into_py`](pyo3_async_runtimes::tokio::future_into_py) wraps
//!   a Rust `Future<Output = PyResult<T>>` in a Python awaitable so a
//!   `#[pyfunction]` can be `await`-ed from Python.
//! - [`into_future`](pyo3_async_runtimes::tokio::into_future) lifts a
//!   Python coroutine into a Rust `Future` so Rust handlers can `await`
//!   the user's Python callback.
//!
//! [`init`] tells pyo3-async-runtimes which runtime builder to use; it
//! is called once from `#[pymodule] fn _pywire(...)` and the resulting
//! tokio runtime is owned by pyo3-async-runtimes (we never touch it
//! again from this side of the binding).
//!
//! The three `_test_*` pyfunctions exposed here are pytest fixtures,
//! not a public API. They live inside `pywire._pywire` (no `pywire.*`
//! re-export) and exist purely so the bridge gets test coverage before
//! the first real async binding lands.

use pyo3::prelude::*;
use pyo3_async_runtimes::tokio as pyo3_tokio;

/// Configure pyo3-async-runtimes with our preferred tokio runtime
/// settings. Called from `_pywire`'s `#[pymodule]` initializer.
pub fn init() {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();
    pyo3_tokio::init(builder);
}

// ---- test helpers exercised by tests/test_runtime.py -------------------

/// Async `sleep` exposed as `pywire._pywire._test_async_sleep`. Verifies
/// the Rust → Python awaitable bridge end-to-end.
#[pyfunction]
fn _test_async_sleep(py: Python<'_>, seconds: f64) -> PyResult<Bound<'_, PyAny>> {
    pyo3_tokio::future_into_py(py, async move {
        tokio::time::sleep(std::time::Duration::from_secs_f64(seconds)).await;
        Ok(seconds)
    })
}

/// Awaits a Python coroutine from a Rust async context and returns its
/// result. Verifies the Python → Rust direction.
#[pyfunction]
fn _test_await_python_coro<'py>(
    py: Python<'py>,
    coro: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let fut = pyo3_tokio::into_future(coro)?;
    pyo3_tokio::future_into_py(py, fut)
}

/// A Rust async function that does some real work across yield points.
/// Confirms the runtime correctly schedules continuations.
#[pyfunction]
fn _test_async_add(py: Python<'_>, a: i64, b: i64) -> PyResult<Bound<'_, PyAny>> {
    pyo3_tokio::future_into_py(py, async move {
        tokio::task::yield_now().await;
        let intermediate = a + b;
        tokio::task::yield_now().await;
        Ok(intermediate)
    })
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_function(wrap_pyfunction!(_test_async_sleep, parent)?)?;
    parent.add_function(wrap_pyfunction!(_test_await_python_coro, parent)?)?;
    parent.add_function(wrap_pyfunction!(_test_async_add, parent)?)?;
    Ok(())
}
