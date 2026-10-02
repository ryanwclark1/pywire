import abc

__all__: list[str]

class LoginInfo:
    """User, database, and host extracted from the startup message."""

    user: str | None
    database: str | None
    host: str

    def __init__(
        self,
        *,
        user: str | None = None,
        database: str | None = None,
        host: str = "127.0.0.1",
    ) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

class Password:
    """Password material. `salt` is None for cleartext, bytes for hashed."""

    password: bytes
    salt: bytes | None

    def __init__(self, password: bytes, *, salt: bytes | None = None) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

class ScramVerifier:
    """Stored SCRAM-SHA-256 verifier (PostgreSQL ``rolpassword`` form)."""

    iterations: int
    salt: bytes
    stored_key: bytes
    server_key: bytes

    def __init__(
        self, iterations: int, salt: bytes, stored_key: bytes, server_key: bytes
    ) -> None: ...
    @staticmethod
    def parse(text: str) -> ScramVerifier: ...

class AuthSource(abc.ABC):
    """Abstract async source of password material."""

    @abc.abstractmethod
    async def get_password(self, login: LoginInfo) -> Password: ...

class ScramVerifierSource(abc.ABC):
    """Abstract async source of stored SCRAM-SHA-256 verifiers."""

    @abc.abstractmethod
    async def get_scram_verifier(self, login: LoginInfo) -> ScramVerifier | None: ...
