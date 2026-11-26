from collections.abc import Awaitable, Sequence
from typing import final

__all__ = [
    "LoginInfo",
    "Password",
    "ScramVerifier",
    "_test_call_get_password",
]

@final
class LoginInfo:
    """Who is connecting: startup parameters plus TLS details.

    `user` and `database` come from the startup message, `host` is the
    client's IP address. On TLS connections `sni_server_name` is the server
    name the client asked for and `client_certificates` is the verified
    client chain (DER, leaf first) when `TLSConfig.client_ca` is set.
    """

    @property
    def user(self) -> str | None: ...
    @property
    def database(self) -> str | None: ...
    @property
    def host(self) -> str: ...
    @property
    def sni_server_name(self) -> str | None: ...
    @property
    def client_certificates(self) -> list[bytes]: ...
    def __new__(
        cls,
        *,
        user: str | None = None,
        database: str | None = None,
        host: str = "127.0.0.1",
        sni_server_name: str | None = None,
        client_certificates: Sequence[bytes] = (),
    ) -> LoginInfo: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __hash__(self) -> int: ...

@final
class Password:
    """Password material. `salt` is None for cleartext, bytes for hashed."""

    @property
    def password(self) -> bytes: ...
    @property
    def salt(self) -> bytes | None: ...
    def __new__(cls, password: bytes, *, salt: bytes | None = None) -> Password: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __hash__(self) -> int: ...

@final
class ScramVerifier:
    """Stored SCRAM-SHA-256 verifier (PostgreSQL ``rolpassword`` form)."""

    @property
    def iterations(self) -> int: ...
    @property
    def salt(self) -> bytes: ...
    @property
    def stored_key(self) -> bytes: ...
    @property
    def server_key(self) -> bytes: ...
    def __new__(
        cls, iterations: int, salt: bytes, stored_key: bytes, server_key: bytes
    ) -> ScramVerifier: ...
    @staticmethod
    def parse(text: str) -> ScramVerifier:
        """Parse ``SCRAM-SHA-256$<iterations>:<salt>$<StoredKey>:<ServerKey>``."""

def _test_call_get_password(
    auth_source: object, user: str | None, database: str | None, host: str
) -> Awaitable[Password]: ...
