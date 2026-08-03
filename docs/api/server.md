# Server

`pywire.server.serve(simple_query, addr)` is the high-level entry
point: bind a TCP listener, accept connections, and dispatch each one
through pgwire's connection-state machine. Each connection gets its own
async task; queries flow through your simple and optional extended handlers.

## Quick example — open (no auth)

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

## With cleartext authentication

Subclass [`AuthSource`](auth.md) and pass it via `auth=`:

```python
from pywire.auth import AuthSource, LoginInfo, Password
from pywire.errors import InvalidPassword


class StaticUsers(AuthSource):
    def __init__(self, users: dict[str, bytes]) -> None:
        self.users = users

    async def get_password(self, login: LoginInfo) -> Password:
        try:
            return Password(self.users[login.user or ""])
        except KeyError:
            raise InvalidPassword(login.user or "") from None


async def main() -> None:
    await pywire.server.serve(
        Hello(),
        "127.0.0.1:5433",
        auth=StaticUsers({"alice": b"hunter2"}),
    )
```

On connect, pywire sends `AuthenticationCleartextPassword`, awaits the
client's `PasswordMessage`, and calls your `get_password` to look up
the reference password. A mismatch surfaces as
`pywire.errors.InvalidPassword` (SQLSTATE `28P01`).

## SCRAM and TLS

For SCRAM-SHA-256, return the salted password and salt expected by
pgwire, and select `auth_method="scram-sha-256"`. TLS takes a PEM
certificate/key pair and can be required:

```python
from pywire.server import TLSConfig

await pywire.server.serve(
    Hello(),
    "0.0.0.0:5433",
    auth=users,
    auth_method="scram-sha-256",
    tls=TLSConfig("server.crt", "server.key", require=True),
)
```

When TLS is configured, SCRAM-SHA-256-PLUS channel binding is advertised.

## Per-connection sessions

Pass a `SessionFactory` when handlers need state tied to the authenticated
connection, such as a tenant, transaction, or audit context. `open` runs after
authentication and receives the final `LoginInfo`. The returned object handles
simple and extended queries for that connection. pywire calls its optional
synchronous `close()` method when the connection is released.

```python
from pywire.server import SessionFactory


class Sessions(SessionFactory):
    async def open(self, login: LoginInfo) -> Hello:
        return Hello()


await pywire.server.serve(
    Hello(),
    "127.0.0.1:5433",
    auth=users,
    session_factory=Sessions(),
)
```

To stop the server, cancel the task it lives in:

```python
task = asyncio.create_task(pywire.server.serve(Hello(), "127.0.0.1:5433"))
await asyncio.sleep(10)
task.cancel()
```

## Scope

| Capability        | Status                                                                                  |
| ----------------- | --------------------------------------------------------------------------------------- |
| TCP accept loop   | ✅ Multiple concurrent connections via tokio task per connection.                       |
| Simple query (`'Q'`) | ✅ Routed to your `SimpleQueryHandler.do_query`.                                     |
| Startup handshake | ✅ With or without authentication, controlled by the `auth=...` argument.               |
| Cleartext auth    | ✅ Pass an `AuthSource` subclass; pywire runs PostgreSQL's cleartext-password flow.     |
| SCRAM auth        | ✅ SCRAM-SHA-256 and TLS channel binding via pgwire's ring backend.                    |
| Extended query    | ✅ `Parse`/`Bind`/`Describe`/`Execute`, binary formats, and portal suspension.         |
| COPY              | ⬜ Same — protocol error today; `pywire.copy.CopyHandler` ABC is in place.              |
| Cancel requests   | ✅ PostgreSQL cancel requests cancel the active query future.                           |
| TLS               | ✅ PostgreSQL TLS negotiation with optional TLS-required policy.                        |

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
