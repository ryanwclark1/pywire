"""Integration tests for `pywire.server`.

We don't pull in a full PostgreSQL client (psycopg / asyncpg) because
the wheel matrix doesn't carry them and our wire surface is still
narrow. Instead we:

1. Start the server on an ephemeral port.
2. Open a raw TCP connection, hand-construct Startup / Query / Password
   frames, and verify the server's response bytes parse cleanly back
   into the message types we expect.

This is enough to exercise the listen loop, the startup handshake,
the simple-query path, the response framing, and (now) the
cleartext-password auth flow — i.e. every piece of the server we ship
today.
"""

from __future__ import annotations

import asyncio
import contextlib

import pytest

import pywire
from pywire import auth, errors, messages, query, server
from pywire._pywire import _test_bind_ephemeral  # type: ignore[attr-defined]


def test_server_is_exposed_under_pywire():
    assert pywire.server is server


def _password_message(password: bytes) -> bytes:
    """Hand-construct the wire bytes for a PostgreSQL PasswordMessage.

    Format: type byte 'p' + i32 length (incl. itself) + cstring(password).
    """
    body = password + b"\x00"
    length = 4 + len(body)
    return b"p" + length.to_bytes(4, "big") + body


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
        except TimeoutError:
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
            except TimeoutError:
                pass
            return buf
    return buf


@contextlib.asynccontextmanager
async def _running_server(  # type: ignore[no-untyped-def]
    handler: query.SimpleQueryHandler,
    *,
    auth_source: auth.AuthSource | None = None,
):
    """Run `server.serve` on a free port, yield the port, cancel on exit."""
    port = await _test_bind_ephemeral()
    addr = f"127.0.0.1:{port}"
    task = asyncio.create_task(server.serve(handler, addr, auth=auth_source))
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
    """A handler-raised `pywire.errors.QueryCanceled` becomes an
    ErrorResponse on the wire with the right `57014` SQLSTATE — not the
    generic `XX000` we'd see if every PyErr were flattened to
    `PgWireError::ApiError`."""

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
            # PostgreSQL ErrorResponse type tag is 'E'.
            assert b"E" in payload[:1] or b"E" in payload
            assert b"cancel" in payload.lower()
            # ErrorResponse fields are tag-byte + cstring pairs;
            # SQLSTATE rides on field tag 'C'. Look for the
            # QueryCanceled SQLSTATE (57014) verbatim.
            assert b"57014" in payload, (
                f"expected QueryCanceled SQLSTATE 57014 in wire payload (got: {payload!r})"
            )
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


# ---- cleartext authentication ---------------------------------------


class _StaticAuth(auth.AuthSource):
    """Test fixture: in-memory username -> password map.

    Raises `pywire.errors.InvalidPassword` when the user is unknown,
    which routes through `py_err_to_pywire` -> `PgWireError::InvalidPassword`
    -> SQLSTATE 28P01 on the wire.
    """

    def __init__(self, users: dict[str, bytes]) -> None:
        self.users = users

    async def get_password(self, login: auth.LoginInfo) -> auth.Password:
        try:
            return auth.Password(self.users[login.user or ""])
        except KeyError:
            raise errors.InvalidPassword(login.user or "") from None


async def test_cleartext_auth_accepts_correct_password():
    """Full happy-path: Startup -> AuthenticationCleartextPassword ->
    PasswordMessage -> AuthenticationOk + ReadyForQuery -> Query -> Row."""

    class Greeting(query.SimpleQueryHandler):
        async def do_query(self, q: str) -> list[query.Response]:
            return [
                query.Response.query(
                    fields=[query.FieldInfo("greeting", type_id=25)],
                    rows=[[b"hi"]],
                ),
            ]

    auth_source = _StaticAuth({"alice": b"hunter2"})
    async with _running_server(Greeting(), auth_source=auth_source) as port:
        reader, writer = await asyncio.open_connection("127.0.0.1", port)
        try:
            # Startup.
            writer.write(messages.Startup(parameters={"user": "alice"}).encode())
            await writer.drain()
            # Server should reply with AuthenticationCleartextPassword ('R' + length
            # + i32(3)). It does NOT send ReadyForQuery yet.
            auth_request = await _await_with_data(reader, timeout=1.0)
            assert auth_request[0:1] == b"R", (
                f"expected AuthenticationCleartextPassword, got: {auth_request!r}"
            )
            # Body of 'R' is i32(method); 3 means cleartext.
            assert auth_request[5:9] == b"\x00\x00\x00\x03"
            # No ReadyForQuery yet.
            assert b"Z" not in auth_request

            # Send PasswordMessage with the right password.
            writer.write(_password_message(b"hunter2"))
            await writer.drain()
            handshake = await _await_with_data(reader)
            assert b"Z" in handshake, f"expected ReadyForQuery after auth ok, got: {handshake!r}"

            # Now run a query.
            writer.write(messages.Query("SELECT 'hi'").encode())
            await writer.drain()
            response = await _await_with_data(reader)
            assert b"greeting" in response
            assert b"hi" in response
            assert b"Z" in response
        finally:
            writer.close()
            with contextlib.suppress(Exception):
                await writer.wait_closed()


async def test_cleartext_auth_rejects_wrong_password():
    """A wrong password should produce an ErrorResponse carrying
    SQLSTATE 28P01 (invalid_password). The server then closes the
    connection — i.e. we don't reach ReadyForQuery."""
    auth_source = _StaticAuth({"alice": b"hunter2"})
    async with _running_server(_DummyHandler(), auth_source=auth_source) as port:
        reader, writer = await asyncio.open_connection("127.0.0.1", port)
        try:
            writer.write(messages.Startup(parameters={"user": "alice"}).encode())
            await writer.drain()
            await _await_with_data(reader, timeout=1.0)  # auth request
            writer.write(_password_message(b"WRONG"))
            await writer.drain()
            payload = await _await_with_data(reader, timeout=1.0)
            assert b"E" in payload[:1] or b"E" in payload
            assert b"28P01" in payload, f"expected InvalidPassword SQLSTATE 28P01 in {payload!r}"
        finally:
            writer.close()
            with contextlib.suppress(Exception):
                await writer.wait_closed()


async def test_cleartext_auth_rejects_unknown_user():
    """An unknown user should land as InvalidPassword (we raise it from
    `_StaticAuth.get_password` on KeyError) — SQLSTATE 28P01."""
    auth_source = _StaticAuth({"alice": b"hunter2"})
    async with _running_server(_DummyHandler(), auth_source=auth_source) as port:
        reader, writer = await asyncio.open_connection("127.0.0.1", port)
        try:
            writer.write(messages.Startup(parameters={"user": "bob"}).encode())
            await writer.drain()
            await _await_with_data(reader, timeout=1.0)
            writer.write(_password_message(b"anything"))
            await writer.drain()
            payload = await _await_with_data(reader, timeout=1.0)
            assert b"E" in payload[:1] or b"E" in payload
            assert b"28P01" in payload
        finally:
            writer.close()
            with contextlib.suppress(Exception):
                await writer.wait_closed()
