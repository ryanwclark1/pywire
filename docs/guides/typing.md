# Typing

pywire ships `py.typed` and complete type information, so mypy and pyright
check your handlers against the real API.

## How the types are maintained

- The pure-Python modules (`pywire.server`, `pywire.query`, `pywire.auth`,
  `pywire.copy`, `pywire.session`, `pywire.sync`, `pywire.types`) are typed
  inline, so what you read is what checkers see.
- The Rust extension is described by a stub package, `pywire/_pywire/*.pyi`.
  CI compares it with the built module using `mypy.stubtest`, and checks the
  whole package with mypy **and** pyright in strict mode.
- Native classes are `@final`, their attributes are read-only properties,
  and value classes are hashable.

## Useful types

| Type | Where | Use |
| --- | --- | --- |
| `ResponseKind` | `pywire.query` | `Literal` values of `Response.kind` |
| `AuthMethod` | `pywire.server` | `Literal` values of `serve(auth_method=...)` |
| `AuthenticationKind` | `pywire.messages` | `Literal` values of `Authentication.kind` |
| `Type`, `Format` | `pywire.types` | OID and format-code `IntEnum`s |
| `SessionFactory[S]` | `pywire.server` | generic in the session type it opens |
| `SimpleQuerySession`, `ExtendedQuerySession`, `ClosableSession`, `Session` | `pywire.session` | structural protocols for session objects |

## Handler signatures

Subclassing the ABCs gives you checked signatures:

```python
from pywire.query import Response, SimpleQueryHandler


class Handler(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:  # checked
        ...
```

Rows are `list[bytes | None]`. Checkers flag a forgotten `.encode()`, a
`str` cell, early in development.
