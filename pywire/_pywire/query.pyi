from collections.abc import AsyncIterable, Awaitable
from typing import Literal, final

from pywire._pywire.errors import ErrorInfo
from pywire.copy import CopyHandler
from pywire.query import ResponseKind

__all__ = [
    "FieldInfo",
    "Response",
    "_test_drive_handler",
]

@final
class FieldInfo:
    """Metadata for one column of a query result.

    `type_id` is the PostgreSQL type OID (default `25`, `text`); `format` is
    the column's format code, `0` for text or `1` for binary.
    """

    @property
    def name(self) -> str: ...
    @property
    def type_id(self) -> int: ...
    @property
    def format(self) -> int: ...
    def __new__(cls, name: str, *, type_id: int = 25, format: int = 0) -> FieldInfo: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __hash__(self) -> int: ...

@final
class Response:
    """One statement's result inside a query response.

    Build instances with the classmethods; `kind` names the variant.
    """

    @property
    def kind(self) -> ResponseKind: ...
    @classmethod
    def empty(cls) -> Response:
        """An `EmptyQueryResponse`, for an empty query string."""

    @classmethod
    def execution(
        cls,
        command: str,
        *,
        oid: int | None = None,
        rows: int | None = None,
        transaction: Literal["start", "end"] | None = None,
    ) -> Response:
        """A `CommandComplete` tag such as `INSERT 0 1` or `BEGIN`.

        `transaction` reports that the command started or ended a
        transaction block, which updates the `ReadyForQuery` status.
        """

    @classmethod
    def query(
        cls,
        fields: list[FieldInfo],
        rows: list[list[bytes | None]],
        *,
        command_tag: str = "SELECT",
    ) -> Response:
        """A complete result set: `RowDescription`, the rows, then the tag."""

    @classmethod
    def stream(
        cls,
        fields: list[FieldInfo],
        rows: AsyncIterable[list[bytes | None]],
        *,
        command_tag: str = "SELECT",
    ) -> Response:
        """A result set whose rows are pulled lazily from an async iterable."""

    @classmethod
    def error(cls, info: ErrorInfo) -> Response:
        """A statement-level `ErrorResponse`."""

    @classmethod
    def copy_in(cls, handler: CopyHandler, *, columns: int, format: int = 0) -> Response:
        """Start `COPY ... FROM STDIN`, delivering the data to `handler`.

        `columns` is the number of columns being copied and `format` is
        `0` (text/CSV) or `1` (binary). See `pywire.copy`.
        """

    @classmethod
    def copy_out(cls, chunks: AsyncIterable[bytes], *, columns: int, format: int = 0) -> Response:
        """Answer `COPY ... TO STDOUT` with `chunks`, one `CopyData` per chunk.

        Each chunk counts as one row in the `COPY n` tag. For binary format
        put the PGCOPY header in the first chunk; the trailer is added.
        """

def _test_drive_handler(handler: object, query: str) -> Awaitable[list[tuple[str, str]]]: ...
