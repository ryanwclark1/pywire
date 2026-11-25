__all__ = [
    "type_name",
    "array_element",
    "builtin_types",
]

def type_name(oid: int) -> str | None:
    """Upstream name for a built-in type OID (`int4`, `_text`, ...), or `None`."""

def array_element(oid: int) -> int | None:
    """Element type OID of a built-in array type, or `None` for non-arrays."""

def builtin_types() -> list[tuple[str, int]]:
    """Every built-in type `postgres-types` knows, as `(name, oid)` pairs."""
