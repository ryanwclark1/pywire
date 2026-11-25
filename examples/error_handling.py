"""Report errors to clients with real SQLSTATE codes.

Three ways out of a handler:

- raise a `pywire.errors.Error` subclass such as `QueryCanceled`, which
  aborts the whole query string with that class's SQLSTATE;
- raise `UserError(ErrorInfo(...))` to abort with any SQLSTATE and fields;
- return `Response.error(ErrorInfo(...))` for one statement, which lets the
  responses before it through.
"""

from __future__ import annotations

import asyncio
import os

from pywire import server
from pywire.errors import ErrorInfo, QueryCanceled, UserError
from pywire.query import FieldInfo, Response, SimpleQueryHandler
from pywire.types import Type

ONE = FieldInfo("one", type_id=Type.INT4)


class Strict(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        responses: list[Response] = []
        for statement in (part.strip() for part in query.split(";") if part.strip()):
            if statement.upper() == "SELECT 1":
                responses.append(Response.query([ONE], [[b"1"]]))
            elif statement.upper().startswith("CANCEL"):
                # SQLSTATE 57014, exactly as PostgreSQL reports a cancel.
                raise QueryCanceled("cancelled by request")
            elif statement.upper().startswith("INSERT"):
                raise UserError(
                    ErrorInfo(
                        "ERROR",
                        "23505",  # unique_violation
                        'duplicate key value violates unique constraint "items_pkey"',
                        detail="Key (id)=(1) already exists.",
                        constraint="items_pkey",
                    )
                )
            else:
                info = ErrorInfo(
                    "ERROR",
                    "42601",  # syntax_error
                    f"unsupported statement: {statement}",
                    hint="This server only understands SELECT 1.",
                )
                responses.append(Response.error(info))
                break  # PostgreSQL stops at the first failing statement.
        return responses


async def main(addr: str) -> None:
    await server.serve(Strict(), addr)


if __name__ == "__main__":
    asyncio.run(main(os.environ.get("PYWIRE_ADDR", "127.0.0.1:5433")))
