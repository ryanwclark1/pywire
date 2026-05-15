"""High-level server bootstrap for pywire.

`pywire.serve(simple_query, addr)` binds a TCP listener and accepts
PostgreSQL wire-protocol connections, dispatching each to the
pgwire-side server loop. The returned awaitable runs the accept loop
forever; cancel the `asyncio.Task` it lives in to stop the server.

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

!!! warning "First-server caveats"
    This first iteration of the server has narrow scope:

    - **No authentication.** Every client is trusted. Cleartext / MD5 /
      SCRAM startup handlers ship in v0.40.1.
    - **No extended query.** A client that tries `Parse`/`Bind`
      against this server will get a protocol error. `SimpleQueryHandler`
      only.
    - **No COPY.** Same — protocol error if attempted.
    - **No TLS.** Plain TCP only.

    These are scoped follow-ups, not architectural blockers — the
    Python ABCs and the Rust adapters for each are already in place
    (PRs E, G, H). The remaining work is the connection-state machine
    wiring on the pgwire side.
"""

from __future__ import annotations

from pywire._pywire import serve as _serve
from pywire.query import SimpleQueryHandler


async def serve(simple_query: SimpleQueryHandler, addr: str) -> None:
    """Async wrapper around the Rust accept loop.

    Wrapping in a coroutine (rather than re-exporting the
    `pyo3-async-runtimes` Future directly) makes the entry point work
    with `asyncio.create_task` and `asyncio.run` without
    `ensure_future`.
    """
    await _serve(simple_query, addr)


__all__ = ["serve"]
