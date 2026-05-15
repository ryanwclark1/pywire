"""Sync convenience wrappers around pywire's async surface.

The async API in `pywire.server`, `pywire.auth`, and `pywire.query` is
the source of truth — pywire is built on tokio + asyncio and expects
coroutines. This module is a small helper for scripts and one-off
utilities that don't want to manage their own event loop.

```python
from pywire.sync import serve_forever
from pywire.query import FieldInfo, Response, SimpleQueryHandler


class Hello(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        return [Response.query(
            fields=[FieldInfo("greeting", type_id=25)],
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
from pywire.query import SimpleQueryHandler


def serve_forever(simple_query: SimpleQueryHandler, addr: str) -> None:
    """Run `pywire.server.serve` to completion in a fresh asyncio loop.

    Blocks until the loop is interrupted (e.g. via SIGINT in a CLI).
    Cancellation is swallowed because the typical exit path is "Ctrl-C
    stops the server" — surfacing `CancelledError` would just clutter
    the caller's traceback.
    """
    with contextlib.suppress(KeyboardInterrupt, asyncio.CancelledError):
        asyncio.run(server.serve(simple_query, addr))


__all__ = ["serve_forever"]
