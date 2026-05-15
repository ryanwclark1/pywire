import abc

from pywire.errors import ErrorInfo

__all__: list[str]

class FieldInfo:
    """Metadata for one column of a query result."""

    name: str
    type_id: int

    def __init__(self, name: str, *, type_id: int = 25) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

class Response:
    """One statement's result inside a simple-query response.

    Use the classmethod factories, not the bare constructor.
    """

    @property
    def kind(self) -> str:
        """`'empty'`, `'execution'`, `'query'`, or `'error'`."""

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
