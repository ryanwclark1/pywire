"""COPY FROM STDIN / TO STDOUT over a real socket (`pywire.copy`)."""

from __future__ import annotations

import asyncio
import struct
from collections.abc import AsyncIterator

import psycopg
import pytest

import pywire
from pywire import copy, errors, messages, query
from pywire._pywire.query import _test_drive_handler
from pywire.types import Format, Type
from tests.test_server import _await_with_data, _running_server


class Collect(copy.CopyHandler):
    """Buffers COPY data; one row per newline."""

    def __init__(self, fail_on: bytes | None = None, fail_done: bool = False) -> None:
        self.data = bytearray()
        self.failed: str | None = None
        self.fail_on = fail_on
        self.fail_done = fail_done

    async def on_copy_data(self, chunk: bytes) -> None:
        if self.fail_on is not None and self.fail_on in chunk:
            raise errors.QueryCanceled("rejected row")
        self.data += chunk

    async def on_copy_done(self) -> int:
        if self.fail_done:
            raise errors.UserError("cannot commit copy")
        return self.data.count(b"\n")

    async def on_copy_fail(self, message: str) -> None:
        self.failed = message


async def _rows(*rows: bytes) -> AsyncIterator[bytes]:
    for row in rows:
        await asyncio.sleep(0)
        yield row


class _Chunks:
    """An `AsyncIterable` that is not itself an iterator (has only `__aiter__`)."""

    def __init__(self, *rows: bytes) -> None:
        self.rows = rows

    def __aiter__(self) -> AsyncIterator[bytes]:
        return _rows(*self.rows)


class CopyServer(query.SimpleQueryHandler):
    def __init__(self) -> None:
        self.copies: list[Collect] = []

    async def do_query(self, q: str) -> list[query.Response]:
        sql = q.strip().lower()
        if sql == "begin":
            return [query.Response.execution("BEGIN", transaction="start")]
        if sql in ("commit", "rollback"):
            return [query.Response.execution(sql.upper(), transaction="end")]
        if sql.startswith("copy t from"):
            self.copies.append(
                Collect(
                    fail_on=b"bad" if "reject" in sql else None,
                    fail_done="nocommit" in sql,
                )
            )
            return [query.Response.copy_in(self.copies[-1], columns=2)]
        if sql == "copy t to stdout":
            return [query.Response.copy_out(_rows(b"1\ta\n", b"2\tb\n"), columns=2)]
        if sql == "copy t to stdout (format iterable)":
            return [query.Response.copy_out(_Chunks(b"3\tc\n"), columns=2)]
        if sql == "copy t to stdout (format binary)":
            header = b"PGCOPY\n\xff\r\n\x00" + struct.pack("!ii", 0, 0)
            row = struct.pack("!hii", 1, 4, 7)
            return [query.Response.copy_out(_rows(header + row), columns=1, format=Format.BINARY)]
        return [query.Response.query([query.FieldInfo("one", type_id=Type.INT4)], [[b"1"]])]


def _conninfo(port: int) -> str:
    return f"host=127.0.0.1 port={port} sslmode=disable"


def test_copy_is_exposed_under_pywire() -> None:
    assert pywire.copy is copy


def test_copy_handler_is_abstract() -> None:
    with pytest.raises(TypeError):
        copy.CopyHandler()  # type: ignore[abstract]


def test_copy_responses_validate_and_describe_themselves() -> None:
    copy_in = query.Response.copy_in(Collect(), columns=2)
    assert copy_in.kind == "copy_in"
    assert repr(copy_in) == "Response.copy_in(columns=2, format=0)"
    copy_out = query.Response.copy_out(_rows(), columns=1, format=Format.BINARY)
    assert copy_out.kind == "copy_out"
    assert repr(copy_out) == "Response.copy_out(columns=1, format=1)"
    with pytest.raises(ValueError, match="COPY format"):
        query.Response.copy_in(Collect(), columns=1, format=2)
    with pytest.raises(ValueError, match="COPY format"):
        query.Response.copy_out(_rows(), columns=1, format=-1)
    with pytest.raises(TypeError):
        query.Response.copy_out(b"not async", columns=1)  # type: ignore[arg-type]


async def test_copy_from_stdin_delivers_data_and_reports_rows() -> None:
    handler = CopyServer()
    async with _running_server(handler) as port:

        def run() -> tuple[str | None, int]:
            with psycopg.connect(_conninfo(port)) as connection, connection.cursor() as cursor:
                with cursor.copy("COPY t FROM STDIN") as copy_in:
                    copy_in.write(b"1\ta\n2\t")
                    copy_in.write(b"b\n3\tc\n")
                return cursor.statusmessage, cursor.rowcount

        assert await asyncio.to_thread(run) == ("COPY 3", 3)
    assert bytes(handler.copies[0].data) == b"1\ta\n2\tb\n3\tc\n"


async def test_copy_to_stdout_streams_chunks() -> None:
    async with _running_server(CopyServer()) as port:

        def run(sql: str) -> tuple[bytes, int]:
            with psycopg.connect(_conninfo(port)) as connection, connection.cursor() as cursor:
                with cursor.copy(sql) as copy_out:
                    data = b"".join(bytes(chunk) for chunk in copy_out)
                return data, cursor.rowcount

        assert await asyncio.to_thread(run, "COPY t TO STDOUT") == (b"1\ta\n2\tb\n", 2)
        iterable = await asyncio.to_thread(run, "COPY t TO STDOUT (FORMAT iterable)")
        assert iterable == (b"3\tc\n", 1)
        binary, rows = await asyncio.to_thread(run, "COPY t TO STDOUT (FORMAT binary)")
        assert binary.startswith(b"PGCOPY\n\xff\r\n\x00")
        assert binary.endswith(b"\xff\xff")  # pgwire appends the trailer
        assert rows == 1


async def _session(port: int) -> tuple[asyncio.StreamReader, asyncio.StreamWriter]:
    reader, writer = await asyncio.open_connection("127.0.0.1", port)
    writer.write(messages.Startup(parameters={"user": "u"}).encode())
    await writer.drain()
    assert (await _await_with_data(reader, timeout=1.0)).endswith(b"Z\x00\x00\x00\x05I")
    return reader, writer


def _frame(tag: bytes, body: bytes) -> bytes:
    return tag + struct.pack("!i", len(body) + 4) + body


async def test_client_copy_fail_reaches_handler_and_connection_recovers() -> None:
    handler = CopyServer()
    async with _running_server(handler) as port:
        reader, writer = await _session(port)
        try:
            writer.write(messages.Query("COPY t FROM STDIN").encode())
            await writer.drain()
            assert (await _await_with_data(reader, timeout=1.0)).startswith(b"G")
            writer.write(_frame(b"d", b"1\ta\n") + _frame(b"f", b"oops\x00"))
            await writer.drain()
            reply = await _await_with_data(reader, timeout=1.0)
            # pgwire's CopyFail error, then ReadyForQuery.
            assert b"CXX000" in reply and b"terminated by the user: oops" in reply
            assert reply.endswith(b"Z\x00\x00\x00\x05I")
            writer.write(messages.Query("SELECT 1").encode())
            await writer.drain()
            assert b"SELECT 1" in await _await_with_data(reader, timeout=1.0)
        finally:
            writer.close()
            await writer.wait_closed()
    assert handler.copies[0].failed == "oops"
    assert bytes(handler.copies[0].data) == b"1\ta\n"


@pytest.mark.parametrize(
    ("sql", "error"),
    [
        ("COPY t FROM STDIN -- reject", psycopg.errors.QueryCanceled),
        ("COPY t FROM STDIN -- nocommit", psycopg.errors.InternalError_),
    ],
)
async def test_handler_errors_abort_the_copy(sql: str, error: type[Exception]) -> None:
    async with _running_server(CopyServer()) as port:

        def run() -> list[tuple[int]]:
            with psycopg.connect(_conninfo(port), autocommit=True) as connection:
                with pytest.raises(error), connection.cursor().copy(sql) as copy_in:
                    copy_in.write(b"1\tbad\n")
            with psycopg.connect(_conninfo(port)) as connection:
                return connection.execute("SELECT 1").fetchall()

        assert await asyncio.to_thread(run) == [(1,)]


async def test_copy_out_iterator_errors_fail_the_copy() -> None:
    class Broken(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            if not q.lower().startswith("copy"):
                return [query.Response.execution(q.upper(), transaction="start")]

            async def chunks() -> AsyncIterator[bytes]:
                yield b"1\n"
                raise errors.QueryCanceled("source went away")

            async def not_bytes() -> AsyncIterator[object]:
                yield 42

            source = not_bytes() if "types" in q else chunks()
            return [query.Response.copy_out(source, columns=1)]  # type: ignore[arg-type]

    async with _running_server(Broken()) as port:
        for sql, expected in [
            ("COPY t TO STDOUT", b"C57014"),  # the iterator's QueryCanceled
            ("COPY t TO STDOUT -- types", b"CXX000"),  # 42 is not bytes
        ]:
            reader, writer = await _session(port)
            try:
                writer.write(messages.Query(sql).encode())
                await writer.drain()
                reply = b""
                while not reply.endswith(b"Z\x00\x00\x00\x05I"):
                    reply += await _await_with_data(reader, timeout=1.0)
            finally:
                writer.close()
                await writer.wait_closed()
            # CopyOutResponse, then (upstream pgwire) CopyFail and the error.
            assert reply.startswith(b"H")
            assert b"\x00f" in reply or b"f\x00" in reply
            assert expected in reply


async def test_copy_messages_after_a_failed_copy_are_rejected() -> None:
    """Once a handler error ends the COPY, its receiver is gone: further
    CopyData or CopyDone for that COPY is answered with a protocol error."""
    async with _running_server(CopyServer()) as port:
        reader, writer = await _session(port)
        try:
            writer.write(messages.Query("COPY t FROM STDIN -- reject").encode())
            await writer.drain()
            assert (await _await_with_data(reader, timeout=1.0)).startswith(b"G")
            writer.write(_frame(b"d", b"bad\n"))
            await writer.drain()
            reply = await _await_with_data(reader, timeout=1.0)
            assert b"C57014" in reply  # QueryCanceled's SQLSTATE
            # A second COPY that completes normally reports COPY 0.
            writer.write(messages.Query("COPY t FROM STDIN").encode())
            await writer.drain()
            assert (await _await_with_data(reader, timeout=1.0)).startswith(b"G")
            writer.write(_frame(b"c", b""))
            await writer.drain()
            assert b"COPY 0" in await _await_with_data(reader, timeout=1.0)
        finally:
            writer.close()
            await writer.wait_closed()


async def test_copy_responses_through_the_handler_adapter() -> None:
    class Both(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            return [
                query.Response.copy_in(Collect(), columns=2),
                query.Response.copy_out(_rows(b"x\n"), columns=3),
            ]

    assert await _test_drive_handler(Both(), "COPY") == [
        ("copy_in", "copy in columns=2"),
        ("copy_out", "copy out columns=3"),
    ]
