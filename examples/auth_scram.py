"""Password authentication with SCRAM-SHA-256.

pgwire runs the SCRAM exchange; your `AuthSource` returns the *salted*
password (PBKDF2-HMAC-SHA-256 of the password) and its salt. The iteration
count must match `serve(scram_iterations=...)`. To avoid holding even
salted passwords, see `scram_verifier.py`.
"""

from __future__ import annotations

import asyncio
import hashlib
import os

from pywire import server
from pywire.auth import AuthSource, LoginInfo, Password
from pywire.errors import InvalidPassword
from pywire.query import FieldInfo, Response, SimpleQueryHandler
from pywire.types import Type

ITERATIONS = 4096


class Users(AuthSource):
    def __init__(self, passwords: dict[str, str]) -> None:
        self.salted: dict[str, Password] = {}
        for user, password in passwords.items():
            salt = os.urandom(16)
            key = hashlib.pbkdf2_hmac("sha256", password.encode(), salt, ITERATIONS)
            self.salted[user] = Password(key, salt=salt)

    async def get_password(self, login: LoginInfo) -> Password:
        try:
            return self.salted[login.user or ""]
        except KeyError:
            raise InvalidPassword(login.user or "") from None


class WhoAmI(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        return [Response.query([FieldInfo("ok", type_id=Type.BOOL)], [[b"t"]])]


async def main(addr: str) -> None:
    await server.serve(
        WhoAmI(),
        addr,
        auth=Users({"alice": "wonderland"}),
        auth_method="scram-sha-256",
        scram_iterations=ITERATIONS,
    )


if __name__ == "__main__":
    asyncio.run(main(os.environ.get("PYWIRE_ADDR", "127.0.0.1:5433")))
