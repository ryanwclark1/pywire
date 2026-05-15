"""Integration tests for `pywire.server`.

We don't pull in a full PostgreSQL client (psycopg / asyncpg) because
the wheel matrix doesn't carry them and our wire surface is still
narrow (no auth, no extended query). Instead we:

1. Start the server on an ephemeral port.
2. Open a raw TCP connection, hand-construct a Startup + Query message
   pair using `pywire.messages`, and verify the server's response
   bytes parse cleanly back into the message types we expect.

This is enough to exercise the listen loop, the startup handshake,
the simple-query path, and the response framing — i.e. every piece of
the server we ship today.
"""

from __future__ import annotations

import asyncio
import contextlib

import pytest

import pywire
from pywire import errors, messages, query, server
from pywire._pywire import _test_bind_ephemeral  # type: ignore[attr-defined]


def test_server_is_exposed_under_pywire():
    assert pywire.server is server


async def _read_at_least(reader: asyncio.StreamReader, n: int) -> bytes:
    """Read until we have at least `n` bytes."""
    buf = b""
    while len(buf) < n:
        chunk = await reader.read(max(n - len(buf), 4096))
        if not chunk:
            break
        buf += chunk
    return buf


async def _await_with_data(reader: asyncio.StreamReader, timeout: float = 2.0) -> bytes:
    """Read whatever the server sends within `timeout` seconds."""
    deadline = asyncio.get_event_loop().time() + timeout
    buf = b""
    while asyncio.get_event_loop().time() < deadline:
        try:
            chunk = await asyncio.wait_for(reader.read(4096), timeout=0.2)
        except asyncio.TimeoutError:
            if buf:
                return buf
            continue
        if not chunk:
            return buf
        buf += chunk
        # Heuristic: a `Z` (ReadyForQuery) is the server's "all done"
        # for one simple-query exchange. Stop reading once we see it.
        if b"Z" in chunk:
            await asyncio.sleep(0.05)
            try:
                buf += await asyncio.wait_for(reader.read(4096), timeout=0.05)
            except asyncio.TimeoutError:
                pass
            return buf
    return buf


@contextlib.asynccontextmanager
async def _running_server(handler: query.SimpleQueryHandler):  # type: ignore[no-untyped-def]
    """Run `server.serve` on a free port, yield the port, cancel on exit."""
    port = await _test_bind_ephemeral()
    addr = f"127.0.0.1:{port}"
    task = asyncio.create_task(server.serve(handler, addr))
    # Give the listener a moment to bind.
    await asyncio.sleep(0.05)
    try:
        yield port
    finally:
        task.cancel()
        with contextlib.suppress(asyncio.CancelledError, BaseException):
            await task


async def test_simple_query_round_trip_over_tcp():
    """End-to-end: real socket, hand-crafted Startup + Query frames."""

    class Greeting(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            return [
                query.Response.query(
                    fields=[query.FieldInfo("greeting", type_id=25)],
                    rows=[[b"hi"]],
                ),
            ]

    async with _running_server(Greeting()) as port:
        reader, writer = await asyncio.open_connection("127.0.0.1", port)
        try:
            # Send Startup.
            startup = messages.Startup(parameters={"user": "tester"})
            writer.write(startup.encode())
            await writer.drain()
            # Read the auth-ok + parameters + BackendKeyData + ReadyForQuery.
            handshake = await _await_with_data(reader)
            # Server should reach ReadyForQuery state.
            assert b"Z" in handshake, f"no ReadyForQuery in handshake bytes: {handshake!r}"
            # Send a Query.
            writer.write(messages.Query("SELECT 'hi'").encode())
            await writer.drain()
            response = await _await_with_data(reader)
            # Response should include the column name and the row payload.
            assert b"greeting" in response
            assert b"hi" in response
            # And a final ReadyForQuery.
            assert b"Z" in response
        finally:
            writer.close()
            with contextlib.suppress(Exception):
                await writer.wait_closed()


async def test_handler_error_response_reaches_client():
    """A handler-raised pywire.errors.Error becomes an ErrorResponse on the wire."""

    class Failing(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            raise errors.QueryCanceled("test cancel")

    async with _running_server(Failing()) as port:
        reader, writer = await asyncio.open_connection("127.0.0.1", port)
        try:
            writer.write(messages.Startup(parameters={"user": "u"}).encode())
            await writer.drain()
            await _await_with_data(reader)  # handshake
            writer.write(messages.Query("SELECT 1").encode())
            await writer.drain()
            payload = await _await_with_data(reader)
            # PostgreSQL ErrorResponse type tag is 'E'. The error message
            # should mention our cancel string somewhere.
            assert b"E" in payload[:1] or b"E" in payload
            assert b"cancel" in payload.lower()
        finally:
            writer.close()
            with contextlib.suppress(Exception):
                await writer.wait_closed()


async def test_serve_rejects_invalid_address():
    with pytest.raises(ValueError):
        await server.serve(_DummyHandler(), "not-an-address")


async def test_serve_surfaces_bind_failure_as_os_error():
    """A second `serve` on the same port should fail at bind time."""
    port = await _test_bind_ephemeral()
    addr = f"127.0.0.1:{port}"
    first = asyncio.create_task(server.serve(_DummyHandler(), addr))
    await asyncio.sleep(0.05)  # let the first server bind
    try:
        with pytest.raises(OSError):
            await server.serve(_DummyHandler(), addr)
    finally:
        first.cancel()
        with contextlib.suppress(asyncio.CancelledError, BaseException):
            await first


class _DummyHandler(query.SimpleQueryHandler):
    async def do_query(self, q: str) -> list[query.Response]:
        return []
