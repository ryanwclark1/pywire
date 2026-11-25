"""Require TLS, and optionally client certificates (mutual TLS).

    PYWIRE_TLS_CERT=server.crt PYWIRE_TLS_KEY=server.key python examples/tls.py
    psql "host=localhost port=5433 sslmode=require"

Set `PYWIRE_TLS_CLIENT_CA` to a CA bundle to demand client certificates;
the verified chain then shows up in `LoginInfo.client_certificates`.
"""

from __future__ import annotations

import asyncio
import os

from pywire import server
from pywire.auth import AuthSource, LoginInfo, Password
from pywire.errors import InvalidPassword
from pywire.query import FieldInfo, Response, SimpleQueryHandler
from pywire.server import TLSConfig
from pywire.types import Type


class CertificateUsers(AuthSource):
    """Cleartext passwords are safe here: the channel is encrypted."""

    async def get_password(self, login: LoginInfo) -> Password:
        if login.user != "alice":
            raise InvalidPassword(login.user or "")
        print(f"alice via SNI {login.sni_server_name!r}, {len(login.client_certificates)} certs")
        return Password(b"wonderland")


class Hello(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        return [Response.query([FieldInfo("secure", type_id=Type.BOOL)], [[b"t"]])]


async def main(addr: str, *, cert: str, key: str, client_ca: str | None = None) -> None:
    tls = TLSConfig(cert, key, require=True, client_ca=client_ca)
    await server.serve(Hello(), addr, auth=CertificateUsers(), tls=tls)


if __name__ == "__main__":
    asyncio.run(
        main(
            os.environ.get("PYWIRE_ADDR", "127.0.0.1:5433"),
            cert=os.environ.get("PYWIRE_TLS_CERT", "server.crt"),
            key=os.environ.get("PYWIRE_TLS_KEY", "server.key"),
            client_ca=os.environ.get("PYWIRE_TLS_CLIENT_CA"),
        )
    )
