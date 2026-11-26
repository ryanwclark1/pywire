"""Bulk load and export with COPY.

`COPY items FROM STDIN` appends tab-separated rows to an in-memory table;
`COPY items TO STDOUT` streams them back; `SELECT count(*)` counts them.

    psql "host=127.0.0.1 port=5433 sslmode=disable" \\
        -c "\\copy items from 'items.tsv'" -c "COPY items TO STDOUT"
"""

from __future__ import annotations

import asyncio
import os
from collections.abc import AsyncIterator

from pywire import server
from pywire.copy import CopyHandler
from pywire.errors import ErrorInfo
from pywire.query import FieldInfo, Response, SimpleQueryHandler
from pywire.types import Type

Row = tuple[str, str]


class Load(CopyHandler):
    """Receives one COPY FROM STDIN. Chunks may split rows anywhere."""

    def __init__(self, table: list[Row]) -> None:
        self.table = table
        self.buffer = bytearray()

    async def on_copy_data(self, chunk: bytes) -> None:
        self.buffer += chunk

    async def on_copy_done(self) -> int:
        rows: list[Row] = []
        for line in self.buffer.decode().splitlines():
            sku, name = line.split("\t")
            rows.append((sku, name))
        self.table.extend(rows)  # all-or-nothing, like a real COPY
        return len(rows)

    async def on_copy_fail(self, message: str) -> None:
        self.buffer.clear()  # the client aborted; keep nothing


async def export(table: list[Row]) -> AsyncIterator[bytes]:
    for sku, name in list(table):
        yield f"{sku}\t{name}\n".encode()  # one row per chunk


class Items(SimpleQueryHandler):
    def __init__(self) -> None:
        self.table: list[Row] = []

    async def do_query(self, query: str) -> list[Response]:
        sql = " ".join(query.lower().split())
        if sql == "copy items from stdin":
            return [Response.copy_in(Load(self.table), columns=2)]
        if sql == "copy items to stdout":
            return [Response.copy_out(export(self.table), columns=2)]
        if sql == "select count(*) from items":
            count = FieldInfo("count", type_id=Type.INT8)
            return [Response.query([count], [[str(len(self.table)).encode()]])]
        return [Response.error(ErrorInfo("ERROR", "0A000", f"not supported: {query}"))]


async def main(addr: str) -> None:
    await server.serve(Items(), addr)


if __name__ == "__main__":
    asyncio.run(main(os.environ.get("PYWIRE_ADDR", "127.0.0.1:5433")))
