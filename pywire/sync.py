"""Sync convenience wrappers around pywire's async surface.

The async API in `pywire.server`, `pywire.auth`, and `pywire.query` is
the source of truth — pywire is built on tokio + asyncio and expects
coroutines. This module is a small helper for scripts and one-off
utilities that don't want to manage their own event loop.

```python
from pywire.sync import serve_forever
from pywire.query import FieldInfo, Response, SimpleQueryHandler
from pywire.types import Type


class Hello(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        return [Response.query(
            fields=[FieldInfo("greeting", type_id=Type.TEXT)],
            rows=[[b"hi"]],
        )]


# Blocks until KeyboardInterrupt / SIGINT.
serve_forever(Hello(), "127.0.0.1:5433")
```

The async API remains available; use it for tests, larger
applications, and anywhere you already have an event loop running.
"""

from __future__ import annotations

import asyncio
import contextlib

from pywire import server
from pywire.auth import AuthSource, ScramVerifierSource
from pywire.query import ExtendedQueryHandler, SimpleQueryHandler
from pywire.server import AuthMethod, SessionFactory, TLSConfig
from pywire.session import Session


def serve_forever(
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
    """Run `pywire.server.serve` to completion in a fresh asyncio loop.

    Takes the same arguments as `pywire.server.serve`. Blocks until the
    loop is interrupted (e.g. via SIGINT in a CLI). Cancellation is
    swallowed because the typical exit path is "Ctrl-C stops the server";
    surfacing `CancelledError` would just clutter the caller's traceback.
    """
    with contextlib.suppress(KeyboardInterrupt, asyncio.CancelledError):
        asyncio.run(
            server.serve(
                simple_query,
                addr,
                auth=auth,
                extended=extended,
                session_factory=session_factory,
                auth_method=auth_method,
                tls=tls,
                scram_iterations=scram_iterations,
            )
        )


__all__ = ["serve_forever"]
