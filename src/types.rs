//! `pywire._pywire.types`: PostgreSQL built-in type metadata from the
//! `postgres-types` crate that pgwire uses for OIDs.
//!
//! `pywire/types.py` holds the typed `Type` enum generated from
//! [`builtin_types`] (see `scripts/gen_types.py`); a test keeps the two in
//! step when the upstream crate grows new types.

use postgres_types::{Kind, Type};
use pyo3::prelude::*;

/// Upstream name for a built-in type OID (`int4`, `_text`, ...), or `None`.
#[pyfunction]
fn type_name(oid: u32) -> Option<String> {
    Type::from_oid(oid).map(|ty| ty.name().to_owned())
}

/// Element type OID of a built-in array type, or `None` for non-arrays.
#[pyfunction]
fn array_element(oid: u32) -> Option<u32> {
    match Type::from_oid(oid)?.kind() {
        Kind::Array(element) => Some(element.oid()),
        _ => None,
    }
}

/// Every built-in type `postgres-types` knows, as `(name, oid)` pairs in
/// OID order. Built-in OIDs are all below 16384 (`FirstNormalObjectId`).
#[pyfunction]
fn builtin_types() -> Vec<(String, u32)> {
    (0..16384)
        .filter_map(|oid| Type::from_oid(oid).map(|ty| (ty.name().to_owned(), oid)))
        .collect()
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let m = PyModule::new(py, "types")?;
    m.add_function(wrap_pyfunction!(type_name, &m)?)?;
    m.add_function(wrap_pyfunction!(array_element, &m)?)?;
    m.add_function(wrap_pyfunction!(builtin_types, &m)?)?;
    parent.add_submodule(&m)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pywire._pywire.types", &m)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_arrays_come_from_postgres_types() {
        assert_eq!(type_name(25).as_deref(), Some("text"));
        assert_eq!(type_name(1009).as_deref(), Some("_text"));
        assert_eq!(type_name(0), None);
        assert_eq!(array_element(1007), Some(23));
        assert_eq!(array_element(23), None);
        assert_eq!(array_element(0), None);
        let all = builtin_types();
        assert!(all.windows(2).all(|pair| pair[0].1 < pair[1].1));
        assert!(all.contains(&("jsonb".to_owned(), 3802)));
    }
}
