"""Run a server from synchronous code with `pywire.sync.serve_forever`.

It owns its own event loop and blocks until Ctrl-C. Use the async
`pywire.server.serve` inside applications that already run asyncio.
"""

from __future__ import annotations

import os

from pywire.query import FieldInfo, Response, SimpleQueryHandler
from pywire.sync import serve_forever
from pywire.types import Type


class Hello(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        return [Response.query([FieldInfo("greeting", type_id=Type.TEXT)], [[b"hi"]])]


def main(addr: str) -> None:
    serve_forever(Hello(), addr)


if __name__ == "__main__":
    main(os.environ.get("PYWIRE_ADDR", "127.0.0.1:5433"))
