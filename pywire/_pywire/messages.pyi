from typing import ClassVar, Self, final

__all__ = [
    "TransactionStatus",
    "FieldDescription",
    "Startup",
    "Query",
    "Terminate",
    "ReadyForQuery",
    "CommandComplete",
    "RowDescription",
    "DataRow",
    "ErrorResponse",
]

class _Message:
    # Shared shape of every frontend/backend message codec (stub-only base).
    def encode(self) -> bytes:
        """Encode as one full wire frame (type tag + length + body)."""

    @classmethod
    def decode(cls, data: bytes) -> Self:
        """Decode exactly one full wire frame."""

    def __eq__(self, other: object, /) -> bool: ...
    def __hash__(self) -> int: ...

@final
class TransactionStatus:
    """Transaction status reported by the backend in `ReadyForQuery`."""

    Idle: ClassVar[TransactionStatus]
    """Not in a transaction block (`'I'`)."""
    Transaction: ClassVar[TransactionStatus]
    """In a transaction block (`'T'`)."""
    Error: ClassVar[TransactionStatus]
    """In a failed transaction block (`'E'`)."""

    def __eq__(self, other: object, /) -> bool: ...
    def __hash__(self) -> int: ...
    def __int__(self) -> int: ...

@final
class FieldDescription:
    """One column's metadata, inside a `RowDescription`."""

    @property
    def name(self) -> str: ...
    @property
    def table_id(self) -> int: ...
    @property
    def column_id(self) -> int: ...
    @property
    def type_id(self) -> int: ...
    @property
    def type_size(self) -> int: ...
    @property
    def type_modifier(self) -> int: ...
    @property
    def format_code(self) -> int: ...
    def __new__(
        cls,
        name: str,
        *,
        table_id: int = 0,
        column_id: int = 0,
        type_id: int = 0,
        type_size: int = 0,
        type_modifier: int = 0,
        format_code: int = 0,
    ) -> FieldDescription: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __hash__(self) -> int: ...

@final
class Startup(_Message):
    """Frontend startup message. Unlike other frames it has no type-tag byte."""

    @property
    def protocol_number_major(self) -> int: ...
    @property
    def protocol_number_minor(self) -> int: ...
    @property
    def parameters(self) -> dict[str, str]: ...
    def __new__(
        cls,
        protocol_number_major: int = 3,
        protocol_number_minor: int = 0,
        parameters: dict[str, str] | None = None,
    ) -> Startup: ...

@final
class Query(_Message):
    """Simple-query frontend message (`'Q'`)."""

    @property
    def query(self) -> str: ...
    def __new__(cls, query: str) -> Query: ...

@final
class Terminate(_Message):
    """Frontend connection-close message (`'X'`). Carries no payload."""

    def __new__(cls) -> Terminate: ...

@final
class ReadyForQuery(_Message):
    """Backend `'Z'` message: backend is ready for a new query."""

    @property
    def status(self) -> TransactionStatus: ...
    def __new__(cls, status: TransactionStatus) -> ReadyForQuery: ...

@final
class CommandComplete(_Message):
    """Backend `'C'` message: command-finished tag."""

    @property
    def tag(self) -> str: ...
    def __new__(cls, tag: str) -> CommandComplete: ...

@final
class RowDescription(_Message):
    """Backend `'T'` message: describes the columns of a result set."""

    @property
    def fields(self) -> list[FieldDescription]: ...
    def __new__(cls, fields: list[FieldDescription] | None = None) -> RowDescription: ...

@final
class DataRow(_Message):
    """Backend `'D'` message: one result-row payload as opaque bytes."""

    @property
    def field_count(self) -> int: ...
    @property
    def data(self) -> bytes: ...
    def __new__(cls, field_count: int, data: bytes) -> DataRow: ...

@final
class ErrorResponse(_Message):
    """Backend `'E'` message: PostgreSQL error-field list.

    Each entry is a `(tag_byte, value)` pair. For a structured view,
    convert these into a `pywire.errors.ErrorInfo`.
    """

    @property
    def fields(self) -> list[tuple[int, str]]: ...
    def __new__(cls, fields: list[tuple[int, str]] | None = None) -> ErrorResponse: ...
