# Server

`pywire.server.serve(simple_query, addr)` is the high-level entry
point: bind a TCP listener, accept connections, and dispatch each one
through pgwire's connection-state machine. Each connection gets its own
async task; queries flow through your `SimpleQueryHandler`.

## Quick example

```python
import asyncio
import pywire
from pywire.query import FieldInfo, Response, SimpleQueryHandler


class Hello(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        return [
            Response.query(
                fields=[FieldInfo("greeting", type_id=25)],
                rows=[[b"hello, world"]],
            ),
        ]


async def main() -> None:
    await pywire.server.serve(Hello(), "127.0.0.1:5433")


asyncio.run(main())
```

To stop the server, cancel the task it lives in:

```python
task = asyncio.create_task(pywire.server.serve(Hello(), "127.0.0.1:5433"))
await asyncio.sleep(10)
task.cancel()
```

## Scope

This first iteration of the server is intentionally narrow.

| Capability        | Status                                                                                  |
| ----------------- | --------------------------------------------------------------------------------------- |
| TCP accept loop   | ✅ Multiple concurrent connections via tokio task per connection.                       |
| Simple query (`'Q'`) | ✅ Routed to your `SimpleQueryHandler.do_query`.                                     |
| Startup handshake | ✅ `NoopStartupHandler` (no authentication; every client is trusted).                   |
| Authentication    | ⬜ Cleartext / MD5 / SCRAM startup handlers ship in v0.40.1.                            |
| Extended query    | ⬜ `Parse`/`Bind`/`Describe`/`Execute` get a protocol error today. The Python ABC is in place (`pywire.query.ExtendedQueryHandler`); wiring lands in a follow-up. |
| COPY              | ⬜ Same — protocol error today; `pywire.copy.CopyHandler` ABC is in place.              |
| Cancel requests   | ⬜ pgwire's `NoopHandler` default; no cancel-token routing yet.                         |
| TLS               | ⬜ Plain TCP only. SSL/TLS negotiation lands in a follow-up.                            |

## Errors that reach the wire

If your `do_query` raises a `pywire.errors.Error` (or any subclass),
pywire translates it into a PostgreSQL `ErrorResponse` on the wire.
The client sees the error and the next message it sends starts a
fresh statement.

```python
class Failing(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        raise pywire.errors.QueryCanceled("user cancel")
```

To send a structured error without aborting the whole response stream
(useful when one statement of a multi-statement simple-query fails),
return `Response.error(ErrorInfo(...))` from `do_query` and let other
`Response` entries through.

## What `serve` returns

`serve()` is an `async def` that runs until cancelled. Cancellation
shuts down the accept loop but in-flight per-connection tasks may keep
running until they complete their current request. For deterministic
shutdown, build your own shutdown handle and weave it into the task.

## Reference

::: pywire.server
    options:
      show_source: false
      heading_level: 3
      members_order: source
