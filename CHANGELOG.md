# Changelog

This file is managed by
[release-please](https://github.com/googleapis/release-please) going
forward. Entries are generated from
[Conventional Commits](https://www.conventionalcommits.org/) on `main`.
The `0.40.0` entry below is hand-written because it summarizes the
ten-PR scaffolding-and-bindings push that brought pywire from empty
repo to publishable v0.40.0; subsequent entries are auto-generated.

## 0.40.0 — First public release

pywire `X.Y` mirrors pgwire `X.Y`; this release wraps
[`pgwire 0.40`](https://crates.io/crates/pgwire/0.40.0). See
[`VERSIONING.md`](VERSIONING.md) for the policy.

### Binding surface

- **`pywire.errors`** — every `PgWireError` variant mirrored as a
  Python exception under a three-level hierarchy (root `Error`,
  umbrellas `ProtocolError` / `AuthError`, then 33 concrete classes).
  `ErrorInfo` carries PostgreSQL's standard error-field set
  (severity, code, message, plus 15 optional metadata fields).
- **`pywire.messages`** — wire codecs for `Startup`, `Query`,
  `Terminate`, `ReadyForQuery`, `CommandComplete`, `RowDescription`
  (+ `FieldDescription`), `DataRow`, and `ErrorResponse`. Each class
  has `.encode()` and `.decode(bytes)` and round-trips with
  bit-for-bit fidelity. `TransactionStatus` enum for `ReadyForQuery`.
  Extended-query / COPY / handshake-extras messages remain for a
  follow-up.
- **`pywire.auth`** — `LoginInfo`, `Password`, and the
  `AuthSource` async ABC. Plus an internal Rust adapter so future
  startup handlers can call into Python auth policy.
- **`pywire.query`** — `SimpleQueryHandler` async ABC with the full
  `Response` tagged union (`empty()`, `execution()`, `query()`,
  `error()`). `ExtendedQueryHandler` ABC plus `PreparedStatement` /
  `Portal` / describe-response dataclasses for forward-compat.
  `FieldInfo` for column metadata.
- **`pywire.copy`** — `CopyHandler` async ABC for both `COPY FROM
  STDIN` and `COPY TO STDOUT` flows.
- **`pywire.server`** — `pywire.server.serve(simple_query, addr)`
  binds a TCP listener and runs the simple-query path end-to-end.
  Per-connection tokio tasks; multiple concurrent clients
  supported. No auth, no extended query, no COPY, no TLS at this
  release; all four planned for v0.40.1.
- **`pywire.sync`** — `serve_forever(handler, addr)` wraps the async
  server in `asyncio.run` for CLIs and one-off scripts.

### Infrastructure

- CI on Ubuntu / macOS / Windows × Python 3.9–3.13 with cargo
  fmt/clippy, ruff, mypy strict, cargo-deny (advisories + licenses +
  bans + sources), and actionlint.
- Release pipeline via `cibuildwheel` (manylinux + musllinux
  x86_64/aarch64, macOS universal2, Windows x86_64, sdist) with
  PyPI Trusted Publishing. A `verify` job re-runs the full gate
  on the tagged ref before any wheel is built.
- Coverage gate: **100% effective line coverage** on hand-written
  code, with pyo3 macro decoration and explicit `// LCOV_EXCL_LINE`
  defensive-path markers exempted. Codecov for PR-level visibility.
- Docs site at https://ryanwclark1.github.io/pywire/ (MkDocs Material
  + mkdocstrings).
- Conventional Commits and `release-please` drive subsequent
  version bumps and changelog entries.

### Roadmap onward

- v0.40.1: auth handlers (cleartext / MD5 / SCRAM), extended-query
  trait wiring, COPY trait wiring, TLS negotiation.
- v0.40.x: continued binding polish + bug fixes.
- v0.41.0: tracks the next pgwire minor bump.
