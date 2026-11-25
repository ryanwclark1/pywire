"""PostgreSQL wire-protocol message codecs.

pywire mirrors `pgwire::messages` one type to one class. Every message
class has:

- a constructor taking the message's fields as keyword or positional
  arguments,
- a `.encode() -> bytes` method that emits a full wire frame
  (type tag + length + body, except for `Startup` which has no type tag
  per the protocol),
- a `Class.decode(data: bytes) -> Class` classmethod that parses a
  single full wire frame,
- structural equality and a `__repr__` that includes every field.

Covered: the simple-query and result messages, the extended-query
protocol (`Parse`, `Bind`, `Describe`, `Execute`, `Close`, `Sync`, `Flush`
and their replies), COPY (`CopyData`, `CopyDone`, `CopyFail`,
`CopyInResponse`, `CopyOutResponse`), startup and authentication
(`SslRequest`, `Authentication`, `PasswordMessage`, `SASLInitialResponse`,
`SASLResponse`, `ParameterStatus`, `BackendKeyData`, `CancelRequest`) and
asynchronous messages (`NoticeResponse`, `NotificationResponse`).

Statement and portal names are `str`, with `""` for the unnamed one.

`TransactionStatus` is exported as the enum used by `ReadyForQuery`.
`FieldDescription` is the per-column metadata carried inside
`RowDescription`.
"""

from typing import Literal, TypeAlias

from pywire._pywire.messages import (
    Authentication,
    BackendKeyData,
    Bind,
    BindComplete,
    CancelRequest,
    Close,
    CloseComplete,
    CommandComplete,
    CopyData,
    CopyDone,
    CopyFail,
    CopyInResponse,
    CopyOutResponse,
    DataRow,
    Describe,
    EmptyQueryResponse,
    ErrorResponse,
    Execute,
    FieldDescription,
    Flush,
    NoData,
    NoticeResponse,
    NotificationResponse,
    ParameterDescription,
    ParameterStatus,
    Parse,
    ParseComplete,
    PasswordMessage,
    PortalSuspended,
    Query,
    ReadyForQuery,
    RowDescription,
    SASLInitialResponse,
    SASLResponse,
    SslRequest,
    Startup,
    Sync,
    Terminate,
    TransactionStatus,
)

AuthenticationKind: TypeAlias = Literal[
    "ok",
    "cleartext_password",
    "kerberos_v5",
    "md5_password",
    "sasl",
    "sasl_continue",
    "sasl_final",
]
"""Values of `Authentication.kind`."""

__all__ = [
    "AuthenticationKind",
    "Authentication",
    "BackendKeyData",
    "Bind",
    "BindComplete",
    "CancelRequest",
    "Close",
    "CloseComplete",
    "CommandComplete",
    "CopyData",
    "CopyDone",
    "CopyFail",
    "CopyInResponse",
    "CopyOutResponse",
    "DataRow",
    "Describe",
    "EmptyQueryResponse",
    "ErrorResponse",
    "Execute",
    "FieldDescription",
    "Flush",
    "NoData",
    "NoticeResponse",
    "NotificationResponse",
    "ParameterDescription",
    "ParameterStatus",
    "Parse",
    "ParseComplete",
    "PasswordMessage",
    "PortalSuspended",
    "Query",
    "ReadyForQuery",
    "RowDescription",
    "SASLInitialResponse",
    "SASLResponse",
    "SslRequest",
    "Startup",
    "Sync",
    "Terminate",
    "TransactionStatus",
]
