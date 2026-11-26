"""SCRAM-SHA-256 against stored verifiers, like PostgreSQL's `pg_authid`.

The server keeps only `SCRAM-SHA-256$<iterations>:<salt>$<StoredKey>:<ServerKey>`
strings, so a leaked store cannot be replayed as a password. Unknown users
get a mock exchange that fails exactly like a wrong password.
"""

from __future__ import annotations

import asyncio
import base64
import hashlib
import hmac
import os

from pywire import server
from pywire.auth import LoginInfo, ScramVerifier, ScramVerifierSource
from pywire.query import FieldInfo, Response, SimpleQueryHandler
from pywire.types import Type


def make_verifier(password: str, iterations: int = 4096) -> str:
    """What PostgreSQL stores in `rolpassword` for `password` (RFC 5802)."""
    salt = os.urandom(16)
    salted = hashlib.pbkdf2_hmac("sha256", password.encode(), salt, iterations)
    stored_key = hashlib.sha256(hmac.new(salted, b"Client Key", "sha256").digest()).digest()
    server_key = hmac.new(salted, b"Server Key", "sha256").digest()

    def b64(value: bytes) -> str:
        return base64.b64encode(value).decode()

    return f"SCRAM-SHA-256${iterations}:{b64(salt)}${b64(stored_key)}:{b64(server_key)}"


class Verifiers(ScramVerifierSource):
    def __init__(self, rolpasswords: dict[str, str]) -> None:
        self.rolpasswords = rolpasswords

    async def get_scram_verifier(self, login: LoginInfo) -> ScramVerifier | None:
        text = self.rolpasswords.get(login.user or "")
        return ScramVerifier.parse(text) if text else None


class One(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        return [Response.query([FieldInfo("one", type_id=Type.INT4)], [[b"1"]])]


async def main(addr: str) -> None:
    store = Verifiers({"alice": make_verifier("wonderland")})
    await server.serve(One(), addr, auth=store, auth_method="scram-sha-256-verifier")


if __name__ == "__main__":
    asyncio.run(main(os.environ.get("PYWIRE_ADDR", "127.0.0.1:5433")))
