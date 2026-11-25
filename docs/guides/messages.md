# Wire messages

`pywire.messages` exposes pgwire's message codecs directly, for tools that
work below the handler API: protocol tests, proxies, traffic dissectors and
fuzzers. Every class is built from its fields, `encode()`s to exactly one
frame (type tag, length and body), and `decode(frame)`s back. Messages are
immutable, compare by value and are hashable.

```python
from pywire import messages

frame = messages.Parse("SELECT $1", name="s1", type_oids=[23]).encode()
assert frame[:1] == b"P"
assert messages.Parse.decode(frame).type_oids == [23]
```

## What's covered

| Area | Messages |
| --- | --- |
| Startup | `Startup`, `SslRequest`, `CancelRequest` (no type tag), `ParameterStatus`, `BackendKeyData` |
| Authentication | `Authentication` (tagged by `kind`), `PasswordMessage`, `SASLInitialResponse`, `SASLResponse` |
| Simple query | `Query`, `RowDescription` + `FieldDescription`, `DataRow`, `CommandComplete`, `EmptyQueryResponse`, `ReadyForQuery` + `TransactionStatus` |
| Extended query | `Parse`, `Bind`, `Describe`, `Execute`, `Close`, `Sync`, `Flush`, `ParseComplete`, `BindComplete`, `CloseComplete`, `ParameterDescription`, `NoData`, `PortalSuspended` |
| COPY | `CopyData`, `CopyDone`, `CopyFail`, `CopyInResponse`, `CopyOutResponse` |
| Asynchronous | `ErrorResponse`, `NoticeResponse`, `NotificationResponse` |
| Session end | `Terminate` |

Conventions:

- Statement and portal names are `str`, and `""` means unnamed.
- `Describe` and `Close` take `target="S"` (statement) or `"P"` (portal).
- `BackendKeyData` and `CancelRequest` keys are `bytes`: 4 bytes in protocol
  3.0, up to 256 in protocol 3.2.
- `ErrorResponse` and `NoticeResponse` fields are `(tag_byte, value)` pairs,
  using PostgreSQL's single-letter field codes (`ord("C")` for the SQLSTATE).

## Errors

`decode` raises `ValueError` when the input is shorter than the length in
its header, and a `pywire.errors.ProtocolError` subclass for malformed
content, such as the wrong type tag or a truncated field.

## Driving a server by hand

The codecs make protocol-level tests short. This is how pywire's own tests
check behavior that drivers hide:

```python
reader, writer = await asyncio.open_connection("127.0.0.1", port)
writer.write(messages.Startup(parameters={"user": "alice"}).encode())
writer.write(messages.Query("SELECT 1").encode())
await writer.drain()
reply = await reader.read(65536)  # AuthenticationOk ... ReadyForQuery
```
