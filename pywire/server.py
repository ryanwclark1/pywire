"""High-level server bootstrap for pywire.

`pywire.server.serve(simple_query, addr, *, auth=None)` binds a TCP
listener and accepts PostgreSQL wire-protocol connections, dispatching
each to the pgwire-side server loop. The returned awaitable runs the
accept loop forever; cancel the `asyncio.Task` it lives in to stop the
server.

```python
import asyncio
import pywire
from pywire.auth import AuthSource, LoginInfo, Password
from pywire.errors import InvalidPassword
from pywire.query import FieldInfo, Response, SimpleQueryHandler


class Hello(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        return [
            Response.query(
                fields=[FieldInfo("greeting", type_id=25)],
                rows=[[b"hello, world"]],
            ),
        ]


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


asyncio.run(main())
```

Pass `auth=None` (the default) to disable authentication; every
client is then trusted.
"""

from __future__ import annotations

from pywire._pywire import serve as _serve
from pywire.auth import AuthSource
from pywire.query import SimpleQueryHandler


async def serve(
    simple_query: SimpleQueryHandler,
    addr: str,
    *,
    auth: AuthSource | None = None,
) -> None:
    """Async wrapper around the Rust accept loop.

    Wrapping in a coroutine (rather than re-exporting the
    `pyo3-async-runtimes` Future directly) makes the entry point work
    with `asyncio.create_task` and `asyncio.run` without
    `ensure_future`.
    """
    await _serve(simple_query, addr, auth=auth)


__all__ = ["serve"]
