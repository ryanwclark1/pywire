"""The smallest useful pywire server: answer every query with a greeting.

Run it, then connect with any PostgreSQL client:

    python examples/hello.py
    psql "host=127.0.0.1 port=5433 sslmode=disable" -c "SELECT 1; SET x = 1"
"""

from __future__ import annotations

import asyncio
import os

from pywire import server
from pywire.query import FieldInfo, Response, SimpleQueryHandler
from pywire.types import Type


class Hello(SimpleQueryHandler):
    """Answers each `;`-separated statement on its own."""

    async def do_query(self, query: str) -> list[Response]:
        statements = [part.strip() for part in query.split(";") if part.strip()]
        if not statements:
            return [Response.empty()]
        return [self.answer(statement) for statement in statements]

    def answer(self, statement: str) -> Response:
        keyword = statement.split()[0].upper()
        if keyword == "SELECT":
            return Response.query(
                fields=[FieldInfo("greeting", type_id=Type.TEXT)],
                rows=[[b"hello, world"]],
            )
        if keyword == "BEGIN":
            return Response.execution("BEGIN", transaction="start")
        if keyword in ("COMMIT", "ROLLBACK"):
            return Response.execution(keyword, transaction="end")
        # Any other command "succeeds" with its own tag, e.g. `SET`.
        return Response.execution(keyword)


async def main(addr: str) -> None:
    await server.serve(Hello(), addr)


if __name__ == "__main__":
    asyncio.run(main(os.environ.get("PYWIRE_ADDR", "127.0.0.1:5433")))
