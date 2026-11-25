"""Native extension module built from `src/` with PyO3.

The public API lives in the pure-Python `pywire.*` modules, which re-export
these classes. Names starting with `_test_` are test hooks, not public API.
"""

import asyncio
from collections.abc import Awaitable

from pywire._pywire import auth as auth
from pywire._pywire import errors as errors
from pywire._pywire import messages as messages
from pywire._pywire import query as query
from pywire._pywire import types as types
from pywire.auth import AuthSource, ScramVerifierSource
from pywire.query import ExtendedQueryHandler, SimpleQueryHandler
from pywire.server import SessionFactory

__all__ = [
    "supported_protocol_range",
    "errors",
    "messages",
    "auth",
    "query",
    "serve",
    "types",
    "_test_bind_ephemeral",
    "_test_async_sleep",
    "_test_await_python_coro",
    "_test_async_add",
]

def supported_protocol_range() -> tuple[int, int]:
    """Return the (earliest, latest) PostgreSQL protocol versions supported by pgwire."""

def serve(
    simple_query: SimpleQueryHandler,
    addr: str,
    *,
    auth: AuthSource | ScramVerifierSource | None = None,
    extended: ExtendedQueryHandler | None = None,
    session_factory: SessionFactory | None = None,
    auth_method: str = "cleartext",
    tls_cert: str | None = None,
    tls_key: str | None = None,
    require_tls: bool = False,
    tls_client_ca: str | None = None,
    scram_iterations: int = 4096,
    _shutdown: asyncio.Event | None = None,
) -> asyncio.Future[None]:
    """Bind `addr` and run the accept loop; use `pywire.server.serve` instead."""

def _test_bind_ephemeral() -> Awaitable[int]: ...
def _test_async_sleep(seconds: float) -> Awaitable[float]: ...
def _test_await_python_coro(coro: Awaitable[object]) -> Awaitable[object]: ...
def _test_async_add(a: int, b: int) -> Awaitable[int]: ...
