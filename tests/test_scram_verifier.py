"""SCRAM-SHA-256 against stored verifiers (auth_method="scram-sha-256-verifier")."""

from __future__ import annotations

import asyncio
import base64
import contextlib
import hashlib
import hmac
import struct
from pathlib import Path

import psycopg
import pytest

from pywire import errors, messages, query, server
from pywire.auth import LoginInfo, ScramVerifier, ScramVerifierSource
from tests.test_server import _await_with_data, _running_server

FIXTURES = Path(__file__).parent / "fixtures"


def make_verifier(password: str, salt: bytes = b"0123456789abcdef", iterations: int = 4096) -> str:
    """PostgreSQL's rolpassword text for `password` (RFC 5802 §3)."""
    salted = hashlib.pbkdf2_hmac("sha256", password.encode(), salt, iterations)
    client_key = hmac.new(salted, b"Client Key", "sha256").digest()
    stored_key = hashlib.sha256(client_key).digest()
    server_key = hmac.new(salted, b"Server Key", "sha256").digest()

    def b64(value: bytes) -> str:
        return base64.b64encode(value).decode()

    return f"SCRAM-SHA-256${iterations}:{b64(salt)}${b64(stored_key)}:{b64(server_key)}"


class Verifiers(ScramVerifierSource):
    def __init__(self, users: dict[str, str]) -> None:
        self.users = users
        self.logins: list[LoginInfo] = []

    async def get_scram_verifier(self, login: LoginInfo) -> ScramVerifier | None:
        self.logins.append(login)
        text = self.users.get(login.user or "")
        return None if text is None else ScramVerifier.parse(text)


class One(query.SimpleQueryHandler):
    async def do_query(self, q: str) -> list[query.Response]:
        return [query.Response.query([query.FieldInfo("one", type_id=23)], [[b"1"]])]


USERS = {
    "alice": make_verifier("secret"),
    # Per-user iteration counts come from the verifier, not the server.
    "bob": make_verifier("hunter2", salt=b"another-salt", iterations=8192),
}


def _connect(
    port: int, user: str, password: str, extra: str = "sslmode=disable"
) -> list[tuple[int]]:
    conninfo = f"host=127.0.0.1 port={port} user={user} password={password} {extra}"
    with psycopg.connect(conninfo) as connection:
        return connection.execute("SELECT 1").fetchall()


async def _auth_error(port: int, user: str, password: str, extra: str = "sslmode=disable") -> str:
    with pytest.raises(psycopg.OperationalError) as error:
        await asyncio.to_thread(_connect, port, user, password, extra)
    return str(error.value)


def test_scram_verifier_type() -> None:
    verifier = ScramVerifier.parse(USERS["bob"])
    assert verifier.iterations == 8192
    assert verifier.salt == b"another-salt"
    assert len(verifier.stored_key) == len(verifier.server_key) == 32
    assert repr(verifier) == "ScramVerifier(iterations=8192, <12-byte salt>)"
    rebuilt = ScramVerifier(8192, b"another-salt", verifier.stored_key, verifier.server_key)
    assert rebuilt.stored_key == verifier.stored_key

    with pytest.raises(ValueError, match="32-byte"):
        ScramVerifier(4096, b"salt", b"short", verifier.server_key)
    with pytest.raises(ValueError, match="not a SCRAM-SHA-256"):
        ScramVerifier.parse("md5" + "0" * 32)


async def test_verifier_auth_with_psycopg() -> None:
    source = Verifiers(USERS)
    async with _running_server(
        One(), auth_source=source, auth_method="scram-sha-256-verifier"
    ) as port:
        assert await asyncio.to_thread(_connect, port, "alice", "secret") == [(1,)]
        assert await asyncio.to_thread(_connect, port, "bob", "hunter2") == [(1,)]
        assert [login.user for login in source.logins] == ["alice", "bob"]

        wrong = await _auth_error(port, "alice", "not-the-password")
        unknown = await _auth_error(port, "mallory", "whatever")
        assert 'Password authentication failed for user "alice"' in wrong
        # No user-existence oracle: an unknown user fails the same way.
        assert unknown == wrong.replace('"alice"', '"mallory"')


async def test_unknown_user_gets_stable_mock_salt() -> None:
    """An unknown user sees a salt that is stable across attempts."""

    async def server_first(port: int, user: str) -> bytes:
        reader, writer = await asyncio.open_connection("127.0.0.1", port)
        try:
            writer.write(messages.Startup(parameters={"user": user}).encode())
            await writer.drain()
            await _await_with_data(reader, timeout=0.5)
            writer.write(_sasl_initial("SCRAM-SHA-256", b"n,,n=,r=clientnonce"))
            await writer.drain()
            reply = await _await_with_data(reader, timeout=0.5)
        finally:
            writer.close()
            with contextlib.suppress(Exception):
                await writer.wait_closed()
        salt = reply.split(b",s=", 1)[1].split(b",", 1)[0]
        assert reply.split(b",i=", 1)[1].startswith(b"4096")
        return salt

    async with _running_server(
        One(), auth_source=Verifiers({}), auth_method="scram-sha-256-verifier"
    ) as port:
        ghost = await server_first(port, "ghost")
        assert ghost == await server_first(port, "ghost")
        assert ghost != await server_first(port, "phantom")


def _sasl_initial(mechanism: str, data: bytes | None) -> bytes:
    body = mechanism.encode() + b"\0"
    body += struct.pack("!i", -1) if data is None else struct.pack("!i", len(data)) + data
    return b"p" + struct.pack("!i", len(body) + 4) + body


@pytest.mark.parametrize(
    ("mechanism", "data", "expected"),
    [
        ("SCRAM-SHA-256", None, b"empty SCRAM client-first-message"),
        ("SCRAM-SHA-256-PLUS", b"p=tls-server-end-point,,n=,r=abc", b"negotiation error"),
        ("SCRAM-SHA-256", b"n,,m=ext,n=,r=abc", b"malformed SCRAM client-first-message"),
    ],
)
async def test_protocol_violations_are_rejected(
    mechanism: str, data: bytes | None, expected: bytes
) -> None:
    async with _running_server(
        One(), auth_source=Verifiers(USERS), auth_method="scram-sha-256-verifier"
    ) as port:
        reader, writer = await asyncio.open_connection("127.0.0.1", port)
        try:
            writer.write(messages.Startup(parameters={"user": "alice"}).encode())
            await writer.drain()
            advertised = await _await_with_data(reader, timeout=0.5)
            # Without TLS only the non-PLUS mechanism is offered.
            assert b"SCRAM-SHA-256\0" in advertised
            assert b"SCRAM-SHA-256-PLUS" not in advertised
            writer.write(_sasl_initial(mechanism, data))
            await writer.drain()
            reply = await _await_with_data(reader, timeout=0.5)
        finally:
            writer.close()
            with contextlib.suppress(Exception):
                await writer.wait_closed()
        assert reply.startswith(b"E")
        assert expected in reply


async def test_channel_binding_over_tls() -> None:
    tls = server.TLSConfig(str(FIXTURES / "server.crt"), str(FIXTURES / "server.key"))
    async with _running_server(
        One(), auth_source=Verifiers(USERS), auth_method="scram-sha-256-verifier", tls=tls
    ) as port:
        # channel_binding=require makes libpq use SCRAM-SHA-256-PLUS with
        # tls-server-end-point and fail if the server doesn't support it.
        required = "sslmode=require channel_binding=require"
        assert await asyncio.to_thread(_connect, port, "alice", "secret", required) == [(1,)]
        # A client that skips binding still works (no downgrade needed: it
        # sends "n", not "y").
        disabled = "sslmode=require channel_binding=disable"
        assert await asyncio.to_thread(_connect, port, "bob", "hunter2", disabled) == [(1,)]
        assert "Password authentication failed" in await _auth_error(
            port, "alice", "wrong", required
        )


async def test_callback_errors_reach_the_client() -> None:
    class Broken(ScramVerifierSource):
        async def get_scram_verifier(self, login: LoginInfo) -> ScramVerifier | None:
            if login.user == "raises":
                raise errors.InvalidPassword(login.user)
            return "not a verifier"  # type: ignore[return-value]

    async with _running_server(
        One(), auth_source=Broken(), auth_method="scram-sha-256-verifier"
    ) as port:
        assert "Password authentication failed" in await _auth_error(port, "raises", "x")
        assert await _auth_error(port, "wrong-type", "x")


async def test_serve_validates_verifier_configuration() -> None:
    with pytest.raises(ValueError, match="requires auth=ScramVerifierSource"):
        await server.serve(One(), "127.0.0.1:1", auth_method="scram-sha-256-verifier")

    # Channel binding needs a certificate hash algorithm; Ed25519 has none.
    with pytest.raises(ValueError, match="not supported"):
        await server.serve(
            One(),
            "127.0.0.1:1",
            auth=Verifiers({}),
            auth_method="scram-sha-256-verifier",
            tls=server.TLSConfig(str(FIXTURES / "ed25519.crt"), str(FIXTURES / "ed25519.key")),
        )
