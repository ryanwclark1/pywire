"""Python bindings for the Rust ``pgwire`` crate.

pywire exposes the PostgreSQL wire-protocol primitives implemented by
the [`pgwire`](https://crates.io/crates/pgwire) Rust crate to Python. See
the [docs](https://ryanwclark1.github.io/pywire/) for the user guide.
"""

from pywire import auth, copy, errors, messages, query, server, sync, types
from pywire._pywire import supported_protocol_range
from pywire.types import Format, Type

__all__ = [
    "Format",
    "Type",
    "auth",
    "copy",
    "errors",
    "messages",
    "query",
    "server",
    "supported_protocol_range",
    "sync",
    "types",
]
