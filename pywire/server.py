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

import abc
from dataclasses import dataclass
from typing import Any, Literal

from pywire._pywire import serve as _serve
from pywire.auth import AuthSource, LoginInfo
from pywire.query import ExtendedQueryHandler, SimpleQueryHandler


@dataclass(frozen=True)
class TLSConfig:
    """PEM certificate/key pair used for PostgreSQL TLS negotiation."""

    cert: str
    key: str
    require: bool = True


class SessionFactory(abc.ABC):
    """Create one Python handler/session object per authenticated connection.

    The returned object takes precedence over the process-wide simple and
    extended handlers for that connection. If it has a synchronous ``close``
    method, pywire calls it when the connection is released.
    """

    @abc.abstractmethod
    async def open(self, login: LoginInfo) -> Any:
        """Return an object implementing the configured query handler methods."""


async def serve(
    simple_query: SimpleQueryHandler,
    addr: str,
    *,
    auth: AuthSource | None = None,
    extended: ExtendedQueryHandler | None = None,
    session_factory: SessionFactory | None = None,
    auth_method: Literal["trust", "cleartext", "scram-sha-256"] = "cleartext",
    tls: TLSConfig | None = None,
    scram_iterations: int = 4096,
) -> None:
    """Async wrapper around the Rust accept loop.

    Wrapping in a coroutine (rather than re-exporting the
    `pyo3-async-runtimes` Future directly) makes the entry point work
    with `asyncio.create_task` and `asyncio.run` without
    `ensure_future`.
    """
    await _serve(
        simple_query,
        addr,
        auth=auth,
        extended=extended,
        session_factory=session_factory,
        auth_method=auth_method,
        tls_cert=tls.cert if tls else None,
        tls_key=tls.key if tls else None,
        require_tls=tls.require if tls else False,
        scram_iterations=scram_iterations,
    )


__all__ = ["SessionFactory", "TLSConfig", "serve"]
