# Changelog

This file is managed by
[release-please](https://github.com/googleapis/release-please) going
forward. Entries are generated from
[Conventional Commits](https://www.conventionalcommits.org/) on `main`.
The `0.40.0` entry below is hand-written because it summarizes the
ten-PR scaffolding-and-bindings push that brought pywire from empty
repo to publishable v0.40.0; subsequent entries are auto-generated.

## [0.41.0](https://github.com/ryanwclark1/pywire/compare/v0.40.0...v0.41.0) (2026-05-30)


### ⚠ BREAKING CHANGES

* pywire now requires Python 3.11 or newer. Python 3.9 reached end-of-life in October 2025; 3.10 EOLs in October 2026.

### Features

* **auth:** bind LoginInfo, Password, and the AuthSource ABC ([66cd47d](https://github.com/ryanwclark1/pywire/commit/66cd47d9ce0bc2554a440459ccb20cd7147c5d48))
* bump min Python to 3.11, add 3.14 to CI matrix ([1016eff](https://github.com/ryanwclark1/pywire/commit/1016eff94f8a44323487c7ed39ed8d762762c220))
* **copy:** bind CopyHandler ABC + CopyInfo for the COPY sub-protocol ([77c0b8b](https://github.com/ryanwclark1/pywire/commit/77c0b8b3aecb9ec0dc00b74299dc5581fe7dac47))
* **errors:** bind PgWireError + ErrorInfo as pywire.errors ([ea5546f](https://github.com/ryanwclark1/pywire/commit/ea5546fd431ba5456e52f68db8ba5ca3fb93ab78))
* **messages:** bind 8 foundational PgWire message codecs ([d5a468a](https://github.com/ryanwclark1/pywire/commit/d5a468a7700f882f0a827ddef4ce39ad9622594b))
* pyo3 0.28 + release-please + 100% coverage gate ([04fe5ce](https://github.com/ryanwclark1/pywire/commit/04fe5ce11d319a50469c67486f7c81956e8ebb36))
* **query:** bind ExtendedQueryHandler ABC + portal/statement dataclasses ([ff634a0](https://github.com/ryanwclark1/pywire/commit/ff634a023d02e329b9ba110c5db3d7686f94b330))
* **query:** bind SimpleQueryHandler ABC + Response + FieldInfo ([bda0e13](https://github.com/ryanwclark1/pywire/commit/bda0e134aa977b4909f9d2e8e1c4b107a154846b))
* **runtime:** wire pyo3-async-runtimes + tokio for async bindings ([c037520](https://github.com/ryanwclark1/pywire/commit/c0375200d07ba35c32dac17cb4aad4edf5caff64))
* scaffold python bindings for rust pgwire ([2f1c1d1](https://github.com/ryanwclark1/pywire/commit/2f1c1d1f861492857697f08512c8cea6ade1074f))
* **server:** cleartext-password authentication via AuthSource ([1a1f9f8](https://github.com/ryanwclark1/pywire/commit/1a1f9f847b7092a2aaf3fb4dc782d70e99a3fc3a))
* **server:** wire pywire.server.serve end-to-end with simple-query ([5892055](https://github.com/ryanwclark1/pywire/commit/5892055d70efb8e9586de0c4de3771522b0659dd))
* **sync:** add pywire.sync.serve_forever wrapper for CLI/script use ([bb10ee8](https://github.com/ryanwclark1/pywire/commit/bb10ee848e61b1c5be53e923c5abe77274568199))


### Bug Fixes

* box PyStartupInner::Cleartext to silence clippy::large_enum_variant ([04c8e2c](https://github.com/ryanwclark1/pywire/commit/04c8e2cc50a0c21baadca1a02b227352b315c58f))
* **ci:** replace maturin develop with pip install -e and add pytest-asyncio ([4a153c0](https://github.com/ryanwclark1/pywire/commit/4a153c01a72038795947fe25474b5a638301bb16))
* install cargo-llvm-cov in CI; address P1 + P2 review comments ([c117ff7](https://github.com/ryanwclark1/pywire/commit/c117ff76270236161392d2d455c7e678bd87efec))

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
