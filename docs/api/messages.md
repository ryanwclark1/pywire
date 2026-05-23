# Messages

`pywire.messages` exposes the PostgreSQL wire-protocol message types as
Python classes. Every class supports:

- **construction** from Python with named or positional fields,
- a **`.encode() -> bytes`** method that emits a full wire frame
  (type tag + length + body — except for `Startup`, which has no type
  tag per the protocol),
- a **`Class.decode(data: bytes) -> Class`** classmethod that parses a
  single full wire frame and raises a
  [`pywire.errors.ProtocolError`](errors.md) subclass on malformed
  input,
- **structural equality** and a `__repr__` that includes the salient
  fields.

## What's exposed today

| Direction | Class             | Tag   | Notes                                                         |
| --------- | ----------------- | ----- | ------------------------------------------------------------- |
| Frontend  | `Startup`         | —     | No type tag (protocol-historical reasons).                    |
| Frontend  | `Query`           | `'Q'` | Simple-query protocol.                                        |
| Frontend  | `Terminate`       | `'X'` | Connection close. No body.                                    |
| Backend   | `ReadyForQuery`   | `'Z'` | Carries a `TransactionStatus` indicator.                      |
| Backend   | `CommandComplete` | `'C'` | Command-finished tag (e.g. `"SELECT 1"`, `"INSERT 0 1"`).     |
| Backend   | `RowDescription`  | `'T'` | A list of `FieldDescription` per result column.               |
| Backend   | `DataRow`         | `'D'` | One row's payload as opaque bytes.                            |
| Backend   | `ErrorResponse`   | `'E'` | List of `(tag_byte, value)` field pairs.                      |

`TransactionStatus` is a Python enum with `Idle`, `Transaction`, and
`Error` variants. `FieldDescription` is the per-column metadata carried
inside `RowDescription`.

The extended-query messages (`Parse`, `Bind`, `Execute`, `Sync`,
`PortalSuspended`, …), COPY, and the rest of the startup handshake
(`Authentication`, `ParameterStatus`, `BackendKeyData`, …) land in
later PRs — see
[`BINDING_STRATEGY.md`](https://github.com/ryanwclark1/pywire/blob/main/BINDING_STRATEGY.md).

## Quick round-trip example

```python
from pywire.messages import Query

q = Query("SELECT 1")
wire = q.encode()                  # b'Q\x00\x00\x00\rSELECT 1\x00'
back = Query.decode(wire)          # Query(query="SELECT 1")
assert back == q
```

## Inspecting an `ErrorResponse`

`ErrorResponse.fields` carries raw `(tag_byte, value)` pairs as they
appear on the wire. For a structured view aligned with PostgreSQL's
[error-fields reference](https://www.postgresql.org/docs/current/protocol-error-fields.html),
walk the list yourself or convert into
[`pywire.errors.ErrorInfo`](errors.md). The mapping (e.g. `'S'` →
`severity`, `'C'` → `code`, `'M'` → `message`) is documented in the
PostgreSQL manual.

## Reference

::: pywire.messages
    options:
      show_source: false
      heading_level: 3
      members_order: source
