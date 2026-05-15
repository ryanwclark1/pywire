"""Simple-query handler bindings for pywire.

Subclass `SimpleQueryHandler` and implement async `do_query` to define
how your pywire server answers simple-query (`Q`) requests:

```python
from pywire.query import FieldInfo, Response, SimpleQueryHandler


class HelloHandler(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        if query.strip().lower() == "select 1":
            return [
                Response.query(
                    fields=[FieldInfo("one", type_id=23)],
                    rows=[[b"1"]],
                ),
            ]
        return [Response.execution(query.split()[0].upper(), rows=0)]
```

`Response` constructors mirror pgwire's `Response` variants:

- `Response.empty()` — empty query string.
- `Response.execution(command, *, oid=None, rows=None)` — DML / DDL
  completion. Maps to PostgreSQL's `CommandComplete` tag.
- `Response.query(fields, rows, *, command_tag="SELECT")` — rows-
  returning result. Each row is a `list[bytes | None]`: text-format
  bytes per cell, or `None` for SQL NULL.
- `Response.error(info)` — error response with a
  [`pywire.errors.ErrorInfo`](errors.md).

The actual wire glue (running the handler against a real connection)
ships with `pywire.server` (PR I). The shape is final today; the only
missing piece is the connection state machine that calls it.
"""

from __future__ import annotations

import abc

from pywire._pywire.query import FieldInfo, Response


class SimpleQueryHandler(abc.ABC):
    """Async ABC. Subclass to define your simple-query response policy."""

    @abc.abstractmethod
    async def do_query(self, query: str) -> list[Response]:
        """Execute `query` and return the list of `Response` objects to
        send back to the client.

        Raise any subclass of `pywire.errors.Error` to surface a
        server-side error to the client. Returning
        `Response.error(info)` is the structured way to send a single
        statement-level error.
        """


__all__ = ["FieldInfo", "Response", "SimpleQueryHandler"]
