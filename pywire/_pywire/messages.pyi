from collections.abc import Sequence
from typing import ClassVar, Literal, Self, final

from pywire.messages import AuthenticationKind

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
    "Parse",
    "ParseComplete",
    "Bind",
    "BindComplete",
    "Describe",
    "Execute",
    "Close",
    "CloseComplete",
    "Sync",
    "Flush",
    "ParameterDescription",
    "NoData",
    "PortalSuspended",
    "EmptyQueryResponse",
    "CopyData",
    "CopyDone",
    "CopyFail",
    "CopyInResponse",
    "CopyOutResponse",
    "SslRequest",
    "Authentication",
    "PasswordMessage",
    "SASLInitialResponse",
    "SASLResponse",
    "ParameterStatus",
    "BackendKeyData",
    "CancelRequest",
    "NoticeResponse",
    "NotificationResponse",
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

# ---- extended query, COPY, startup/auth and notices ----

@final
class ParseComplete(_Message):
    """Backend `'1'`: Parse succeeded."""

    def __new__(cls) -> ParseComplete: ...

@final
class BindComplete(_Message):
    """Backend `'2'`: Bind succeeded."""

    def __new__(cls) -> BindComplete: ...

@final
class CloseComplete(_Message):
    """Backend `'3'`: Close succeeded."""

    def __new__(cls) -> CloseComplete: ...

@final
class NoData(_Message):
    """Backend `'n'`: the described statement or portal returns no rows."""

    def __new__(cls) -> NoData: ...

@final
class PortalSuspended(_Message):
    """Backend `'s'`: Execute stopped at its row limit."""

    def __new__(cls) -> PortalSuspended: ...

@final
class EmptyQueryResponse(_Message):
    """Backend `'I'`: response to an empty query string."""

    def __new__(cls) -> EmptyQueryResponse: ...

@final
class Sync(_Message):
    """Frontend `'S'`: end of an extended-query batch."""

    def __new__(cls) -> Sync: ...

@final
class Flush(_Message):
    """Frontend `'H'`: flush pending backend output."""

    def __new__(cls) -> Flush: ...

@final
class CopyDone(_Message):
    """`'c'`: end of COPY data (either direction)."""

    def __new__(cls) -> CopyDone: ...

@final
class SslRequest(_Message):
    """Frontend SSL negotiation request. Like `Startup`, it has no type tag."""

    def __new__(cls) -> SslRequest: ...

@final
class Parse(_Message):
    """Frontend `'P'`: parse `query` into prepared statement `name` (`""` = unnamed)."""

    @property
    def name(self) -> str: ...
    @property
    def query(self) -> str: ...
    @property
    def type_oids(self) -> list[int]: ...
    def __new__(cls, query: str, *, name: str = "", type_oids: Sequence[int] = ()) -> Parse: ...

@final
class Bind(_Message):
    """Frontend `'B'`: bind parameters to a statement, creating a portal."""

    @property
    def portal_name(self) -> str: ...
    @property
    def statement_name(self) -> str: ...
    @property
    def parameters(self) -> list[bytes | None]: ...
    @property
    def parameter_formats(self) -> list[int]: ...
    @property
    def result_formats(self) -> list[int]: ...
    def __new__(
        cls,
        *,
        portal_name: str = "",
        statement_name: str = "",
        parameters: Sequence[bytes | None] = (),
        parameter_formats: Sequence[int] = (),
        result_formats: Sequence[int] = (),
    ) -> Bind: ...

@final
class Describe(_Message):
    """Frontend `'D'`: describe a prepared statement (`"S"`) or portal (`"P"`)."""

    @property
    def target(self) -> Literal["S", "P"]: ...
    @property
    def name(self) -> str: ...
    def __new__(cls, target: Literal["S", "P"], name: str = "") -> Describe: ...

@final
class Close(_Message):
    """Frontend `'C'`: close a prepared statement (`"S"`) or portal (`"P"`)."""

    @property
    def target(self) -> Literal["S", "P"]: ...
    @property
    def name(self) -> str: ...
    def __new__(cls, target: Literal["S", "P"], name: str = "") -> Close: ...

@final
class Execute(_Message):
    """Frontend `'E'`: run portal `name`, returning at most `max_rows` (`0` = all)."""

    @property
    def name(self) -> str: ...
    @property
    def max_rows(self) -> int: ...
    def __new__(cls, name: str = "", max_rows: int = 0) -> Execute: ...

@final
class ParameterDescription(_Message):
    """Backend `'t'`: parameter type OIDs of a described statement."""

    @property
    def types(self) -> list[int]: ...
    def __new__(cls, types: Sequence[int] = ()) -> ParameterDescription: ...

@final
class CopyData(_Message):
    """`'d'`: one chunk of COPY data (either direction)."""

    @property
    def data(self) -> bytes: ...
    def __new__(cls, data: bytes) -> CopyData: ...

@final
class CopyFail(_Message):
    """Frontend `'f'`: abort COPY FROM STDIN with `message`."""

    @property
    def message(self) -> str: ...
    def __new__(cls, message: str) -> CopyFail: ...

@final
class CopyInResponse(_Message):
    """Backend `'G'`: the server is ready to receive COPY data."""

    @property
    def format(self) -> int: ...
    @property
    def columns(self) -> int: ...
    @property
    def column_formats(self) -> list[int]: ...
    def __new__(
        cls, format: int, columns: int, column_formats: Sequence[int] | None = None
    ) -> CopyInResponse: ...

@final
class CopyOutResponse(_Message):
    """Backend `'H'`: the server is about to send COPY data."""

    @property
    def format(self) -> int: ...
    @property
    def columns(self) -> int: ...
    @property
    def column_formats(self) -> list[int]: ...
    def __new__(
        cls, format: int, columns: int, column_formats: Sequence[int] | None = None
    ) -> CopyOutResponse: ...

@final
class Authentication(_Message):
    """Backend `'R'`: an authentication request or result, tagged by `kind`.

    `md5_password` carries a 4-byte `salt`, `sasl` the offered
    `mechanisms`, and `sasl_continue` / `sasl_final` their `data`.
    """

    @property
    def kind(self) -> AuthenticationKind: ...
    @property
    def salt(self) -> bytes | None: ...
    @property
    def mechanisms(self) -> list[str]: ...
    @property
    def data(self) -> bytes | None: ...
    def __new__(
        cls,
        kind: AuthenticationKind,
        *,
        salt: bytes | None = None,
        mechanisms: Sequence[str] = (),
        data: bytes | None = None,
    ) -> Authentication: ...

@final
class PasswordMessage(_Message):
    """Frontend `'p'`: a cleartext (or MD5-hashed) password."""

    @property
    def password(self) -> str: ...
    def __new__(cls, password: str) -> PasswordMessage: ...

@final
class SASLInitialResponse(_Message):
    """Frontend `'p'`: first SASL message, naming the chosen mechanism."""

    @property
    def mechanism(self) -> str: ...
    @property
    def data(self) -> bytes | None: ...
    def __new__(cls, mechanism: str, data: bytes | None = None) -> SASLInitialResponse: ...

@final
class SASLResponse(_Message):
    """Frontend `'p'`: a follow-up SASL message."""

    @property
    def data(self) -> bytes: ...
    def __new__(cls, data: bytes) -> SASLResponse: ...

@final
class ParameterStatus(_Message):
    """Backend `'S'`: a run-time parameter such as `server_version`."""

    @property
    def name(self) -> str: ...
    @property
    def value(self) -> str: ...
    def __new__(cls, name: str, value: str) -> ParameterStatus: ...

@final
class BackendKeyData(_Message):
    """Backend `'K'`: the process ID and secret key for cancel requests."""

    @property
    def pid(self) -> int: ...
    @property
    def secret_key(self) -> bytes:
        """Cancel key: 4 bytes in protocol 3.0, up to 256 in 3.2."""

    def __new__(cls, pid: int, secret_key: bytes) -> BackendKeyData: ...

@final
class CancelRequest(_Message):
    """Frontend cancel request (sent on a new connection, no type tag)."""

    @property
    def pid(self) -> int: ...
    @property
    def secret_key(self) -> bytes:
        """Cancel key: 4 bytes in protocol 3.0, up to 256 in 3.2."""

    def __new__(cls, pid: int, secret_key: bytes) -> CancelRequest: ...

@final
class NoticeResponse(_Message):
    """Backend `'N'`: a notice; `fields` are `(tag_byte, value)` pairs."""

    @property
    def fields(self) -> list[tuple[int, str]]: ...
    def __new__(cls, fields: list[tuple[int, str]] | None = None) -> NoticeResponse: ...

@final
class NotificationResponse(_Message):
    """Backend `'A'`: a `LISTEN`/`NOTIFY` notification."""

    @property
    def pid(self) -> int: ...
    @property
    def channel(self) -> str: ...
    @property
    def payload(self) -> str: ...
    def __new__(cls, pid: int, channel: str, payload: str = "") -> NotificationResponse: ...
