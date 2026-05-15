import abc
from dataclasses import dataclass

from pywire.errors import ErrorInfo

__all__: list[str]

# ---- Simple query (Rust-backed) ----

class FieldInfo:
    """Metadata for one column of a query result."""

    name: str
    type_id: int

    def __init__(self, name: str, *, type_id: int = 25) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

class Response:
    """One statement's result inside a simple-query response."""

    @property
    def kind(self) -> str: ...
    @classmethod
    def empty(cls) -> Response: ...
    @classmethod
    def execution(
        cls,
        command: str,
        *,
        oid: int | None = None,
        rows: int | None = None,
    ) -> Response: ...
    @classmethod
    def query(
        cls,
        fields: list[FieldInfo],
        rows: list[list[bytes | None]],
        *,
        command_tag: str = "SELECT",
    ) -> Response: ...
    @classmethod
    def error(cls, info: ErrorInfo) -> Response: ...

class SimpleQueryHandler(abc.ABC):
    """Subclass to define your simple-query response policy."""

    @abc.abstractmethod
    async def do_query(self, query: str) -> list[Response]: ...

# ---- Extended query (pure Python dataclasses + ABC) ----

@dataclass(frozen=True)
class PreparedStatement:
    name: str
    query: str
    parameter_types: list[int] = ...

@dataclass(frozen=True)
class Portal:
    name: str
    statement: PreparedStatement
    parameters: list[bytes | None] = ...
    result_formats: list[int] = ...

@dataclass(frozen=True)
class DescribeStatementResponse:
    parameter_types: list[int]
    fields: list[FieldInfo]

@dataclass(frozen=True)
class DescribePortalResponse:
    fields: list[FieldInfo]

class ExtendedQueryHandler(abc.ABC):
    @abc.abstractmethod
    async def parse_statement(
        self, name: str, query: str, parameter_types: list[int]
    ) -> PreparedStatement: ...
    @abc.abstractmethod
    async def describe_statement(
        self, statement: PreparedStatement
    ) -> DescribeStatementResponse: ...
    @abc.abstractmethod
    async def bind_portal(
        self,
        name: str,
        statement: PreparedStatement,
        parameters: list[bytes | None],
        result_formats: list[int],
    ) -> Portal: ...
    @abc.abstractmethod
    async def describe_portal(self, portal: Portal) -> DescribePortalResponse: ...
    @abc.abstractmethod
    async def do_query(self, portal: Portal, max_rows: int) -> Response: ...
    async def close_statement(self, name: str) -> None: ...
    async def close_portal(self, name: str) -> None: ...
