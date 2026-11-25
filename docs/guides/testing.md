# Testing your server

Treat your pywire server like any PostgreSQL server in tests: start it on a
free port inside the test's event loop and connect with a real driver.

```python
import asyncio
import contextlib
import socket

import psycopg
import pytest

from pywire import server
from myapp import MyHandler


def free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


@pytest.fixture
async def dsn():
    port = free_port()
    task = asyncio.create_task(server.serve(MyHandler(), f"127.0.0.1:{port}"))
    await asyncio.sleep(0.05)  # let it bind
    yield f"host=127.0.0.1 port={port} sslmode=disable"
    task.cancel()
    with contextlib.suppress(asyncio.CancelledError):
        await task


async def test_select(dsn: str) -> None:
    def query() -> list[tuple[object, ...]]:
        with psycopg.connect(dsn, autocommit=True) as conn:
            return conn.execute("SELECT 1").fetchall()

    assert await asyncio.to_thread(query) == [(1,)]
```

Tips:

- **Run blocking drivers in a thread** (`asyncio.to_thread`) so the server
  keeps serving on the test's loop. Async drivers (`psycopg.AsyncConnection`,
  `asyncpg`) can be awaited directly.
- **Use `autocommit=True`** unless you are testing transactions. Otherwise
  psycopg sends an implicit `BEGIN` that your handler must answer.
- **Test both protocols.** `conn.execute(sql)` with no parameters uses the
  simple protocol, while `conn.execute(sql, params)` and `prepare=True` use
  the extended protocol.
- **Assert on SQLSTATEs**, not message text: `pytest.raises(psycopg.errors.UniqueViolation)`.
- **Test below the driver** when you need to, with
  [wire messages](messages.md) over a raw socket.

pywire's own suite does all of this: see `tests/test_examples.py`, which
runs every example in this guide.
