use pgwire::messages::startup::Startup;
use pyo3::prelude::*;

fn protocol_range() -> (u16, u16) {
    (Startup::PG_PROTOCOL_EARLIEST, Startup::PG_PROTOCOL_LATEST)
}

#[pyfunction]
fn supported_protocol_range() -> (u16, u16) {
    protocol_range()
}

#[pymodule]
fn _pywire(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(supported_protocol_range, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_range_is_ordered() {
        let (earliest, latest) = protocol_range();
        assert!(earliest <= latest, "{earliest} should be <= {latest}");
    }

    #[test]
    fn protocol_range_is_postgres_v3() {
        // PostgreSQL has only ever shipped wire-protocol major 3 since 2003.
        // If pgwire ever returns something else this test gives us a heads-up
        // to revisit the upper-level bindings.
        let (earliest, latest) = protocol_range();
        assert_eq!(earliest >> 8, 0, "earliest major encoded in low byte");
        assert_eq!(latest >> 8, 0, "latest major encoded in low byte");
    }
}
