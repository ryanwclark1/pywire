"""Serve prepared statements (the extended query protocol).

Clients that send parameters (`cursor.execute(sql, params)` in psycopg,
`$1` placeholders in most drivers) use Parse → Bind → Describe → Execute →
Sync. One `ExtendedQueryHandler` answers each step; pgwire does the
message bookkeeping.

This server knows one table, `users(id int8, name text)`, and answers
`SELECT name FROM users WHERE id = $1`.
"""

from __future__ import annotations

import asyncio
import os

from pywire import server
from pywire.errors import ErrorInfo
from pywire.query import (
    DescribePortalResponse,
    DescribeStatementResponse,
    ExtendedQueryHandler,
    FieldInfo,
    Portal,
    PreparedStatement,
    Response,
    SimpleQueryHandler,
)
from pywire.types import Format, Type

USERS = {1: "alice", 2: "bob"}


def decode_int(value: bytes, fmt: int) -> int:
    """Decode an integer parameter sent in text or binary format."""
    if fmt == Format.BINARY:
        return int.from_bytes(value, "big", signed=True)  # int2/int4/int8
    return int(value.decode())


def name_field(result_formats: list[int]) -> FieldInfo:
    # One format code applies to every column; none means text.
    fmt = result_formats[0] if result_formats else Format.TEXT
    return FieldInfo("name", type_id=Type.TEXT, format=fmt)


class Commands(SimpleQueryHandler):
    """Parameterless statements arrive here (`SET`, `BEGIN`, ...)."""

    async def do_query(self, query: str) -> list[Response]:
        return [Response.execution(query.split()[0].upper())]


class Users(ExtendedQueryHandler):
    async def parse_statement(
        self, name: str, query: str, parameter_types: list[int]
    ) -> PreparedStatement:
        # `0` means "client did not say"; we know $1 is an int8 id.
        types = [oid or Type.INT8 for oid in parameter_types] or [Type.INT8]
        return PreparedStatement(name, query, types)

    async def describe_statement(self, statement: PreparedStatement) -> DescribeStatementResponse:
        return DescribeStatementResponse(statement.parameter_types, [name_field([])])

    async def bind_portal(
        self,
        name: str,
        statement: PreparedStatement,
        parameters: list[bytes | None],
        parameter_formats: list[int],
        result_formats: list[int],
    ) -> Portal:
        return Portal(name, statement, parameters, parameter_formats, result_formats)

    async def describe_portal(self, portal: Portal) -> DescribePortalResponse:
        return DescribePortalResponse([name_field(portal.result_formats)])

    async def do_query(self, portal: Portal, max_rows: int) -> Response:
        [raw] = portal.parameters
        if raw is None:
            return Response.query([name_field(portal.result_formats)], [])
        fmt = portal.parameter_formats[0] if portal.parameter_formats else Format.TEXT
        user_id = decode_int(raw, fmt)
        if user_id not in USERS:
            return Response.error(ErrorInfo("ERROR", "P0002", f"no user with id {user_id}"))
        # A text value has the same bytes in text and binary format.
        return Response.query([name_field(portal.result_formats)], [[USERS[user_id].encode()]])


async def main(addr: str) -> None:
    await server.serve(Commands(), addr, extended=Users())


if __name__ == "__main__":
    asyncio.run(main(os.environ.get("PYWIRE_ADDR", "127.0.0.1:5433")))
