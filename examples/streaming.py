"""Stream a large result lazily with `Response.stream`.

Rows come from an async iterable and are pulled only as fast as pgwire can
write them, so memory stays flat no matter how many rows the client reads.
"""

from __future__ import annotations

import asyncio
import os
from collections.abc import AsyncIterator

from pywire import server
from pywire.query import FieldInfo, Response, SimpleQueryHandler
from pywire.types import Type


async def numbers(count: int) -> AsyncIterator[list[bytes | None]]:
    for n in range(1, count + 1):
        # Text format: each cell is the value's PostgreSQL text form.
        yield [str(n).encode(), None if n % 2 else b"even"]
        if n % 1000 == 0:
            await asyncio.sleep(0)  # let other connections run


class Series(SimpleQueryHandler):
    """`SELECT <n>` returns rows 1..n as (n int8, parity text)."""

    async def do_query(self, query: str) -> list[Response]:
        count = int(query.split()[-1])
        fields = [FieldInfo("n", type_id=Type.INT8), FieldInfo("parity", type_id=Type.TEXT)]
        return [Response.stream(fields, numbers(count))]


async def main(addr: str) -> None:
    await server.serve(Series(), addr)


if __name__ == "__main__":
    asyncio.run(main(os.environ.get("PYWIRE_ADDR", "127.0.0.1:5433")))
