# Server lifecycle

## Starting

`pywire.server.serve(...)` is a coroutine that binds the address and accepts
connections until it is cancelled. Each connection runs as its own task on
pywire's Tokio runtime, and your async handlers run on the asyncio loop that
awaited `serve`.

```python
import asyncio

from pywire import server


async def main() -> None:
    task = asyncio.create_task(server.serve(handler, "0.0.0.0:5433"))
    ...
    task.cancel()
    try:
        await task
    except asyncio.CancelledError:
        pass
```

`serve` raises immediately for configuration mistakes: `ValueError` for a
bad address, an unknown `auth_method` or unreadable PEM data, and `OSError`
for missing certificate files or a port already in use.

## Stopping

Cancelling the task stops accepting connections, then **waits** for the
async cleanup of every connection's statements and portals (your
`close_statement` and `close_portal` callbacks) before it re-raises
`CancelledError`. Cancelling it again does not cut that cleanup short, so
keep those callbacks quick. Session `close()` runs as each connection is
released.

## From synchronous code

`pywire.sync.serve_forever(handler, addr, **options)` runs `serve` in a
fresh event loop with the same options, and returns on Ctrl-C:

```python
--8<-- "examples/sync_server.py"
```

## Cancel requests

PostgreSQL clients cancel a running query by opening a second connection and
sending a `CancelRequest` with the key from `BackendKeyData`. pywire handles
this for you: the matching query's Python coroutine is cancelled and the
client receives `57014 query_canceled`. Handlers that hold resources should
release them in `finally` blocks.
