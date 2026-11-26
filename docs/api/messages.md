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

## What's exposed

Every message in the simple and extended query protocols, COPY,
startup/authentication and asynchronous notices. The
[Wire messages guide](../guides/messages.md) has the full table and the
naming conventions (`""` for unnamed statements, `bytes` cancel keys,
`Authentication.kind`).

`TransactionStatus` has `Idle`, `Transaction` and `Error` members (a native
class, not an `enum.Enum`). `FieldDescription` is the per-column metadata
carried inside `RowDescription`.

## Quick round-trip example

```python
from pywire.messages import Query

q = Query("SELECT 1")
wire = q.encode()  # b'Q\x00\x00\x00\rSELECT 1\x00'
back = Query.decode(wire)  # Query(query="SELECT 1")
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
