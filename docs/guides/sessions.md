# Per-connection sessions

Handlers passed to `serve` are shared by every connection. When state must
belong to one client (a transaction, a tenant, a backend connection, an audit
context), give `serve` a `SessionFactory`. Its `open(login)` runs once per
connection, after authentication, and returns the object that answers that
connection's queries.

```python
--8<-- "examples/sessions.py"
```

## How sessions are used

- The session takes precedence over `serve`'s simple and extended handlers
  for its connection. pywire calls its `do_query` for simple queries and its
  extended-query methods for prepared statements.
- `open` receives the final `LoginInfo`: user, database, client host, and
  on TLS connections the SNI name and client certificates.
- If the session has a synchronous `close()`, pywire calls it exactly once
  when the connection is released, after its statements and portals are
  closed.
- Raising from `open` rejects the connection. Raise an `AuthError` subclass
  such as `InvalidPassword` to fail it as an authentication error.

## Typing sessions

`SessionFactory` is generic in the session type, so checkers see the
concrete class:

```python
class Sessions(SessionFactory[Session]):
    async def open(self, login: LoginInfo) -> Session: ...
```

`pywire.session` defines structural protocols for what pywire calls:
`SimpleQuerySession`, `ExtendedQuerySession` and `ClosableSession`. Any
object with matching methods works, and you don't have to subclass the ABCs.

A session serving **both** protocols has one `do_query` that pgwire calls as
`do_query(sql)` for simple queries and as `do_query(portal, max_rows)` for
`Execute`. Declare it with overloads so it satisfies both protocols:

```python
from typing import overload

from pywire.query import Portal, Response


class Session:
    @overload
    async def do_query(self, query: str, /) -> list[Response]: ...
    @overload
    async def do_query(self, portal: Portal, max_rows: int, /) -> Response: ...
    async def do_query(
        self, target: str | Portal, max_rows: int = 0, /
    ) -> list[Response] | Response:
        if isinstance(target, str):
            return await self.simple(target)
        return await self.execute(target, max_rows)
```
