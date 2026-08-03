from dataclasses import dataclass
from typing import Literal

from pywire.auth import AuthSource
from pywire.query import ExtendedQueryHandler, SimpleQueryHandler

__all__: list[str]

@dataclass(frozen=True)
class TLSConfig:
    cert: str
    key: str
    require: bool = True

async def serve(
    simple_query: SimpleQueryHandler,
    addr: str,
    *,
    auth: AuthSource | None = None,
    extended: ExtendedQueryHandler | None = None,
    auth_method: Literal["trust", "cleartext", "scram-sha-256"] = "cleartext",
    tls: TLSConfig | None = None,
    scram_iterations: int = 4096,
) -> None: ...
