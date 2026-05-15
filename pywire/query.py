"""Simple- and extended-query handler bindings for pywire.

## Simple query

Subclass `SimpleQueryHandler` and implement async `do_query` to define
how your pywire server answers `'Q'` requests:

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

## Extended query

`ExtendedQueryHandler` mirrors PostgreSQL's prepared-statement protocol
(Parse → Bind → Describe → Execute → Sync). Subclass it for full
fidelity, or build on top of `SimpleQueryHandler` when you don't need
prepared statements — the server (PR I) will provide a default
`ExtendedQueryHandler` that forwards to a `SimpleQueryHandler` for
users who want the simpler API.

The Rust wiring that drives these handlers lives in `pywire.server`
(PR I); the types here establish the Python shapes so contract-test
handlers and documentation can reference them today.
"""

from __future__ import annotations

import abc
from dataclasses import dataclass, field

from pywire._pywire.query import FieldInfo, Response

# ---------- Simple query ---------------------------------------------


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


# ---------- Extended query types -------------------------------------


@dataclass(frozen=True)
class PreparedStatement:
    """A parsed but not-yet-bound statement.

    `name` is the empty string for unnamed statements (per the
    PostgreSQL protocol). `parameter_types` is a list of PostgreSQL
    type OIDs — `0` means "type not specified, infer from context".
    """

    name: str
    query: str
    parameter_types: list[int] = field(default_factory=list)


@dataclass(frozen=True)
class Portal:
    """A prepared statement bound to a parameter set.

    `parameters` is a list of `bytes | None` — one entry per parameter,
    text-format encoded, or `None` for SQL NULL.
    `result_formats` is a list of `0` (text) or `1` (binary) per output
    column. An empty list means "all text".
    """

    name: str
    statement: PreparedStatement
    parameters: list[bytes | None] = field(default_factory=list)
    result_formats: list[int] = field(default_factory=list)


@dataclass(frozen=True)
class DescribeStatementResponse:
    """Result of describing a `PreparedStatement` ahead of bind."""

    parameter_types: list[int]
    fields: list[FieldInfo]


@dataclass(frozen=True)
class DescribePortalResponse:
    """Result of describing a bound `Portal`."""

    fields: list[FieldInfo]


class ExtendedQueryHandler(abc.ABC):
    """Async ABC for the extended-query protocol.

    The pywire server (PR I) calls these in the order the client's
    Parse / Bind / Describe / Execute / Sync messages dictate.

    Implementations that don't need prepared statements can subclass
    `SimpleQueryHandler` instead; the server will default-forward
    extended-query Execute to simple-query `do_query`.
    """

    @abc.abstractmethod
    async def parse_statement(
        self,
        name: str,
        query: str,
        parameter_types: list[int],
    ) -> PreparedStatement:
        """Parse `query` into a named `PreparedStatement`."""

    @abc.abstractmethod
    async def describe_statement(
        self,
        statement: PreparedStatement,
    ) -> DescribeStatementResponse:
        """Return the parameter types + output schema of `statement`."""

    @abc.abstractmethod
    async def bind_portal(
        self,
        name: str,
        statement: PreparedStatement,
        parameters: list[bytes | None],
        result_formats: list[int],
    ) -> Portal:
        """Bind `parameters` to `statement`, producing a named `Portal`."""

    @abc.abstractmethod
    async def describe_portal(self, portal: Portal) -> DescribePortalResponse:
        """Return the output schema of an already-bound `portal`."""

    @abc.abstractmethod
    async def do_query(self, portal: Portal, max_rows: int) -> Response:
        """Execute up to `max_rows` rows from `portal` (0 = unlimited)."""

    async def close_statement(self, name: str) -> None:  # noqa: B027 - default no-op
        """Forget a named prepared statement. Default: no-op."""

    async def close_portal(self, name: str) -> None:  # noqa: B027 - default no-op
        """Forget a named portal. Default: no-op."""


__all__ = [
    "DescribePortalResponse",
    "DescribeStatementResponse",
    "ExtendedQueryHandler",
    "FieldInfo",
    "Portal",
    "PreparedStatement",
    "Response",
    "SimpleQueryHandler",
]
