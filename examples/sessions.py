"""Keep per-connection state with a `SessionFactory`.

`open` runs once per authenticated connection and returns the object that
answers that connection's queries, so state on `self` is private to one
client. `close()` runs when the connection goes away.
"""

from __future__ import annotations

import asyncio
import os

from pywire import server
from pywire.auth import LoginInfo
from pywire.query import FieldInfo, Response, SimpleQueryHandler
from pywire.server import SessionFactory
from pywire.types import Type


class Session(SimpleQueryHandler):
    """Counts this connection's queries; `SELECT` returns the count."""

    def __init__(self, user: str) -> None:
        self.user = user
        self.queries = 0

    async def do_query(self, query: str) -> list[Response]:
        self.queries += 1
        fields = [FieldInfo("user", type_id=Type.TEXT), FieldInfo("queries", type_id=Type.INT4)]
        return [Response.query(fields, [[self.user.encode(), str(self.queries).encode()]])]

    def close(self) -> None:
        print(f"{self.user} disconnected after {self.queries} queries")


class Sessions(SessionFactory[Session]):
    async def open(self, login: LoginInfo) -> Session:
        return Session(login.user or "anonymous")


class Unused(SimpleQueryHandler):
    """Required positional handler; the session answers instead."""

    async def do_query(self, query: str) -> list[Response]:
        raise NotImplementedError


async def main(addr: str) -> None:
    await server.serve(Unused(), addr, session_factory=Sessions())


if __name__ == "__main__":
    asyncio.run(main(os.environ.get("PYWIRE_ADDR", "127.0.0.1:5433")))
