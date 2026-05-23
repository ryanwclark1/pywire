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

Only the foundational variants are exposed in this first messages PR;
the extended-query, COPY, and startup-handshake messages land in later
PRs (see BINDING_STRATEGY.md for the roadmap).

`TransactionStatus` is exported as the enum used by `ReadyForQuery`.
`FieldDescription` is the per-column metadata carried inside
`RowDescription`.
"""

from pywire._pywire.messages import (
    CommandComplete,
    DataRow,
    ErrorResponse,
    FieldDescription,
    Query,
    ReadyForQuery,
    RowDescription,
    Startup,
    Terminate,
    TransactionStatus,
)

__all__ = [
    "CommandComplete",
    "DataRow",
    "ErrorResponse",
    "FieldDescription",
    "Query",
    "ReadyForQuery",
    "RowDescription",
    "Startup",
    "Terminate",
    "TransactionStatus",
]
