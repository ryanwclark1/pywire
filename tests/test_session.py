"""Typed sessions: `pywire.session` protocols and `SessionFactory[S]`."""

from __future__ import annotations

import asyncio
from typing import overload

import psycopg

from pywire import auth, query, server, session
from pywire.types import Type
from tests.test_server import _running_server

INT4 = query.FieldInfo("n", type_id=Type.INT4)


class Counter:
    """Serves both protocols from one object, as `pywire.session` documents."""

    def __init__(self, user: str | None) -> None:
        self.user = user
        self.calls = 0
        self.closed = False

    @overload
    async def do_query(self, query: str, /) -> list[query.Response]: ...
    @overload
    async def do_query(self, portal: query.Portal, max_rows: int, /) -> query.Response: ...
    async def do_query(
        self, target: str | query.Portal, max_rows: int = 0, /
    ) -> list[query.Response] | query.Response:
        self.calls += 1
        row: list[bytes | None] = [str(self.calls).encode()]
        if isinstance(target, str):
            return [query.Response.query([INT4], [row])]
        assert target.parameters == [b"7"]
        return query.Response.query([INT4], [row])

    async def parse_statement(
        self, name: str, sql: str, parameter_types: list[int], /
    ) -> query.PreparedStatement:
        return query.PreparedStatement(name, sql, parameter_types or [Type.INT4])

    async def describe_statement(
        self, statement: query.PreparedStatement, /
    ) -> query.DescribeStatementResponse:
        return query.DescribeStatementResponse(statement.parameter_types, [INT4])

    async def bind_portal(
        self,
        name: str,
        statement: query.PreparedStatement,
        parameters: list[bytes | None],
        parameter_formats: list[int],
        result_formats: list[int],
        /,
    ) -> query.Portal:
        return query.Portal(name, statement, parameters, parameter_formats, result_formats)

    async def describe_portal(self, portal: query.Portal, /) -> query.DescribePortalResponse:
        return query.DescribePortalResponse([INT4])

    async def close_statement(self, name: str, /) -> None:
        pass

    async def close_portal(self, name: str, /) -> None:
        pass

    def close(self) -> None:
        self.closed = True


class Factory(server.SessionFactory[Counter]):
    def __init__(self) -> None:
        self.sessions: list[Counter] = []

    async def open(self, login: auth.LoginInfo) -> Counter:
        self.sessions.append(Counter(login.user))
        return self.sessions[-1]


class _Unused(query.SimpleQueryHandler):
    async def do_query(self, q: str) -> list[query.Response]:  # pragma: no cover - sessions win
        raise AssertionError("the session handles every query")


def test_counter_satisfies_every_protocol() -> None:
    counter = Counter("alice")
    assert isinstance(counter, session.SimpleQuerySession)
    assert isinstance(counter, session.ExtendedQuerySession)
    assert isinstance(counter, session.ClosableSession)
    assert not isinstance(object(), session.SimpleQuerySession)


async def test_typed_session_serves_simple_and_extended_queries() -> None:
    factory = Factory()
    async with _running_server(_Unused(), session_factory=factory) as port:

        def run() -> list[int]:
            conninfo = f"host=127.0.0.1 port={port} user=alice sslmode=disable"
            with psycopg.connect(conninfo, autocommit=True) as connection:
                simple = connection.execute("SELECT n", prepare=False).fetchone()
                extended = connection.execute("SELECT n WHERE n = %s", ["7"]).fetchone()
                assert simple is not None and extended is not None
                return [simple[0], extended[0]]

        assert await asyncio.to_thread(run) == [1, 2]
        for _ in range(50):
            if factory.sessions and factory.sessions[0].closed:
                break
            await asyncio.sleep(0.02)
    [counter] = factory.sessions
    assert (counter.user, counter.calls, counter.closed) == ("alice", 2, True)
