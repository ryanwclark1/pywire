<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/ryanwclark1/pywire/main/docs/assets/logo-wordmark-dark.svg">
    <img alt="pywire" src="https://raw.githubusercontent.com/ryanwclark1/pywire/main/docs/assets/logo-wordmark.svg" width="300">
  </picture>
</p>

<p align="center">
  <a href="https://github.com/ryanwclark1/pywire/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/ryanwclark1/pywire/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://ryanwclark1.github.io/pywire/"><img alt="Docs" src="https://github.com/ryanwclark1/pywire/actions/workflows/docs.yml/badge.svg"></a>
  <a href="https://codecov.io/gh/ryanwclark1/pywire"><img alt="codecov" src="https://codecov.io/gh/ryanwclark1/pywire/graph/badge.svg"></a>
</p>

Build PostgreSQL wire-protocol servers in Python. pywire wraps the Rust
[`pgwire`](https://crates.io/crates/pgwire) crate: pgwire handles framing,
the startup handshake, authentication, TLS and both query protocols, and
your async Python handlers decide what each query returns. Any PostgreSQL
client (`psql`, psycopg, JDBC, BI tools) can connect.

```python
import asyncio

from pywire import server
from pywire.query import FieldInfo, Response, SimpleQueryHandler
from pywire.types import Type


class Hello(SimpleQueryHandler):
    async def do_query(self, query: str) -> list[Response]:
        return [Response.query([FieldInfo("greeting", type_id=Type.TEXT)], [[b"hello, world"]])]


asyncio.run(server.serve(Hello(), "127.0.0.1:5433"))
```

```console
$ psql "host=127.0.0.1 port=5433 sslmode=disable" -c "SELECT 1"
```

## Features

- Simple and extended (prepared statement) query protocols, text and binary formats
- Streaming results from async iterables, with backpressure
- `COPY FROM STDIN` / `COPY TO STDOUT`
- Cleartext, SCRAM-SHA-256 and stored-verifier SCRAM auth, with channel binding
- TLS, required TLS and mutual TLS
- Per-connection sessions, cancel requests, graceful shutdown
- Any SQLSTATE through `ErrorInfo`; every pgwire error as a Python exception
- Wire-message codecs for proxies and protocol tests
- Fully typed: stubs verified against the runtime, mypy and pyright strict

## Documentation

The **[user guide and API reference](https://ryanwclark1.github.io/pywire/)**
cover every feature, with runnable [`examples/`](examples) that the test suite
executes.

## Installation

pywire is installed from this repository. The `pywire` name on PyPI belongs to
a different project. Pin a commit that passed CI:

```bash
pip install 'pywire @ git+https://github.com/ryanwclark1/pywire.git@<full-commit-sha>'
```

Building needs Python 3.11+ and a Rust toolchain. See
[installation](https://ryanwclark1.github.io/pywire/install/).

## Development

```bash
uv run maturin develop        # build the extension
make lint typecheck test      # ruff + clippy, mypy + stubtest + pyright, pytest
bash scripts/coverage.sh      # 100% Python and Rust line coverage gate
```

Project docs:

- [`VERSIONING.md`](VERSIONING.md): pywire `X.Y` tracks pgwire `X.Y`
- [`RELEASING.md`](RELEASING.md): tagged builds and Git commit pins
- [`BINDING_STRATEGY.md`](BINDING_STRATEGY.md): design rules for bindings (read before opening a binding PR)
- [`CHANGELOG.md`](CHANGELOG.md)
