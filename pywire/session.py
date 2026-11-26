"""Structural types for per-connection session objects.

A `pywire.server.SessionFactory` returns one session object per
authenticated connection. pywire then calls the session's methods instead of
the process-wide handlers passed to `serve`, so per-connection state (a
transaction, a user's catalog, a backend connection) can live on `self`.

pywire looks methods up by name, mirroring pgwire's handler traits:

- `SimpleQuerySession`: `do_query(query)` answers simple-protocol queries.
- `ExtendedQuerySession`: the extended-protocol methods, including
  `do_query(portal, max_rows)` for Execute.
- `ClosableSession`: an optional synchronous `close()`, called once when the
  connection is released.

pgwire names both execute methods `do_query` on two separate Rust traits. A
Python session that serves both protocols therefore implements one
`do_query` that accepts either call shape; declare it with `typing.overload`
so both protocols match:

```python
from typing import overload

from pywire.query import Portal, Response


class Session:
    @overload
    async def do_query(self, query: str, /) -> list[Response]: ...
    @overload
    async def do_query(self, portal: Portal, max_rows: int, /) -> Response: ...
    async def do_query(
        self, target: str | Portal, max_rows: int = 0, /
    ) -> list[Response] | Response:
        if isinstance(target, str):
            return await self.simple(target)
        return await self.execute(target, max_rows)
```
"""

from __future__ import annotations

from typing import Protocol, TypeAlias, runtime_checkable

from pywire.query import (
    DescribePortalResponse,
    DescribeStatementResponse,
    Portal,
    PreparedStatement,
    Response,
)


@runtime_checkable
class SimpleQuerySession(Protocol):
    """A session that answers simple-protocol (`'Q'`) queries."""

    async def do_query(self, query: str, /) -> list[Response]:
        """Execute `query` and return one `Response` per statement."""
        ...


@runtime_checkable
class ExtendedQuerySession(Protocol):
    """A session that implements the extended (prepared-statement) protocol.

    The methods match `pywire.query.ExtendedQueryHandler`; subclassing that
    ABC is the easiest way to satisfy this protocol.
    """

    async def parse_statement(
        self, name: str, query: str, parameter_types: list[int], /
    ) -> PreparedStatement:
        """Parse `query` into a named `PreparedStatement`."""
        ...

    async def describe_statement(
        self, statement: PreparedStatement, /
    ) -> DescribeStatementResponse:
        """Return the parameter types and output columns of `statement`."""
        ...

    async def bind_portal(
        self,
        name: str,
        statement: PreparedStatement,
        parameters: list[bytes | None],
        parameter_formats: list[int],
        result_formats: list[int],
        /,
    ) -> Portal:
        """Bind parameters to `statement`, producing a named `Portal`."""
        ...

    async def describe_portal(self, portal: Portal, /) -> DescribePortalResponse:
        """Return the output columns of a bound `portal`."""
        ...

    async def do_query(self, portal: Portal, max_rows: int, /) -> Response:
        """Execute up to `max_rows` rows of `portal` (`0` means all)."""
        ...

    async def close_statement(self, name: str, /) -> None:
        """Forget a named prepared statement."""
        ...

    async def close_portal(self, name: str, /) -> None:
        """Forget a named portal."""
        ...


@runtime_checkable
class ClosableSession(Protocol):
    """A session with a synchronous `close()` hook."""

    def close(self) -> None:
        """Release per-connection resources; called once on disconnect."""
        ...


Session: TypeAlias = SimpleQuerySession | ExtendedQuerySession
"""Anything `SessionFactory.open` may return."""

__all__ = ["ClosableSession", "ExtendedQuerySession", "Session", "SimpleQuerySession"]
