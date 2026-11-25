"""COPY-protocol bindings for pywire.

PostgreSQL's bulk `COPY` runs as a sub-protocol of a query. A handler's
`do_query` starts one by returning a COPY response:

- `COPY ... FROM STDIN` (client → server): return
  `Response.copy_in(handler, columns=...)`. pywire sends `CopyInResponse`,
  then calls `handler.on_copy_data(chunk)` for each `CopyData` message the
  client sends and `handler.on_copy_done()` when it finishes; the row count
  that returns becomes the `COPY n` command tag. If the client aborts,
  `handler.on_copy_fail(message)` runs instead and the client gets an error.
- `COPY ... TO STDOUT` (server → client): return
  `Response.copy_out(chunks, columns=...)`, where `chunks` is an async
  iterable of `bytes`. Each chunk is sent as one `CopyData` message and
  counts as one row, so yield one row per chunk.

The `CopyHandler` passed to `copy_in` belongs to that one COPY, so keep its
state on `self`:

```python
from pywire.copy import CopyHandler
from pywire.query import Response, SimpleQueryHandler


class Collect(CopyHandler):
    def __init__(self) -> None:
        self.data = bytearray()

    async def on_copy_data(self, chunk: bytes) -> None:
        self.data += chunk

    async def on_copy_done(self) -> int:
        return self.data.count(b"\\n")


class Handler(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        return [Response.copy_in(Collect(), columns=2)]
```

Chunks are opaque: pywire does not parse text, CSV or binary COPY data. A
chunk boundary need not fall on a row boundary for `COPY FROM`.
"""

from __future__ import annotations

import abc


class CopyHandler(abc.ABC):
    """Receives the data of one `COPY ... FROM STDIN`.

    Mirrors pgwire's `CopyHandler` trait: `on_copy_data`, `on_copy_done`
    and `on_copy_fail`. Raising from `on_copy_data` or `on_copy_done`
    aborts the COPY and reports the error to the client (raise a
    `pywire.errors.Error` subclass to choose the SQLSTATE).
    """

    @abc.abstractmethod
    async def on_copy_data(self, chunk: bytes) -> None:
        """Process one `CopyData` payload from the client."""

    @abc.abstractmethod
    async def on_copy_done(self) -> int:
        """The client sent `CopyDone`; return the number of rows copied."""

    async def on_copy_fail(self, message: str) -> None:  # noqa: B027 - default no-op
        """The client sent `CopyFail` with `message`. Default: no-op."""


__all__ = ["CopyHandler"]
