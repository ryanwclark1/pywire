use pgwire::messages::startup::Startup;
use pyo3::prelude::*;

#[pyfunction]
fn supported_protocol_range() -> (u16, u16) {
    (Startup::PG_PROTOCOL_EARLIEST, Startup::PG_PROTOCOL_LATEST)
}

#[pymodule]
fn _pywire(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(supported_protocol_range, m)?)?;
    Ok(())
}
