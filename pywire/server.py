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
from pywire.types import Type


class Hello(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        return [
            Response.query(
                fields=[FieldInfo("greeting", type_id=Type.TEXT)],
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
import asyncio
from dataclasses import dataclass
from typing import Generic, Literal, TypeAlias, TypeVar

from pywire._pywire import serve as _serve
from pywire.auth import AuthSource, LoginInfo, ScramVerifierSource
from pywire.query import ExtendedQueryHandler, SimpleQueryHandler
from pywire.session import Session

AuthMethod: TypeAlias = Literal["trust", "cleartext", "scram-sha-256", "scram-sha-256-verifier"]
"""Values accepted by `serve(auth_method=...)`."""

S_co = TypeVar("S_co", bound=Session, covariant=True)


@dataclass(frozen=True)
class TLSConfig:
    """PEM certificate/key pair used for PostgreSQL TLS negotiation.

    `cert` and `key` are paths to the server's PEM certificate chain and
    private key. With `require=True` (the default) plaintext connections
    are refused after startup. Set `client_ca` to a PEM bundle path to
    require mutual TLS: every client must present a certificate that
    chains to one of those CAs, and the verified chain is exposed to auth
    and session code as `LoginInfo.client_certificates`.
    """

    cert: str
    key: str
    require: bool = True
    client_ca: str | None = None


class SessionFactory(abc.ABC, Generic[S_co]):
    """Create one Python handler/session object per authenticated connection.

    The returned object takes precedence over the process-wide simple and
    extended handlers for that connection: pywire calls its `do_query` (and,
    for prepared statements, its extended-query methods) instead. If it has
    a synchronous ``close`` method, pywire calls it when the connection is
    released. See `pywire.session` for the structural types.

    Parameterize the factory with your session class to keep it typed:
    ``class Factory(SessionFactory[MySession])``.
    """

    @abc.abstractmethod
    async def open(self, login: LoginInfo) -> S_co:
        """Return the session object for the connection described by `login`."""


async def serve(
    simple_query: SimpleQueryHandler,
    addr: str,
    *,
    auth: AuthSource | ScramVerifierSource | None = None,
    extended: ExtendedQueryHandler | None = None,
    session_factory: SessionFactory[Session] | None = None,
    auth_method: AuthMethod = "cleartext",
    tls: TLSConfig | None = None,
    scram_iterations: int = 4096,
) -> None:
    """Async wrapper around the Rust accept loop.

    Wrapping in a coroutine (rather than re-exporting the
    `pyo3-async-runtimes` Future directly) makes the entry point work
    with `asyncio.create_task` and `asyncio.run` without
    `ensure_future`.

    Cancellation stops accepting connections and waits for their async
    portal/statement cleanup before returning. Cleanup callbacks must finish;
    repeated cancellation does not interrupt them.
    """
    shutdown = asyncio.Event()
    running = _serve(
        simple_query,
        addr,
        auth=auth,
        extended=extended,
        session_factory=session_factory,
        auth_method=auth_method,
        tls_cert=tls.cert if tls else None,
        tls_key=tls.key if tls else None,
        require_tls=tls.require if tls else False,
        tls_client_ca=tls.client_ca if tls else None,
        scram_iterations=scram_iterations,
        _shutdown=shutdown,
    )
    try:
        await asyncio.shield(running)
    except asyncio.CancelledError:
        shutdown.set()
        while not running.done():
            try:
                await asyncio.shield(running)
            except asyncio.CancelledError:
                continue
        running.result()
        raise


__all__ = ["AuthMethod", "SessionFactory", "TLSConfig", "serve"]
