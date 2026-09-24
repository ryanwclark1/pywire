# Binding strategy

pywire exposes the server side of the Rust [`pgwire`](https://crates.io/crates/pgwire)
crate to Python. Public handlers are asyncio classes with typed stubs; the
native extension owns socket processing and calls Python handlers through
`pyo3-async-runtimes`.

## Design rules

- Follow upstream protocol and trait semantics where Python can express them.
  Keep wire messages and query results owned across the Rust/Python boundary.
- Preserve PostgreSQL type OIDs and format codes on the wire, including custom
  nonzero OIDs that `postgres-types` does not know by name.
- Hold connection-specific Python state per connection. Parsed statements and
  bound portals survive the matching extended-query phases; Close, Sync, and
  disconnect release the corresponding state.
- Keep public `.pyi` files aligned with runtime behavior and verify them with
  strict type checking.
- Match the upstream pgwire major/minor version. Binding-only fixes increment
  pywire's patch version; consumers pin a tested Git commit.

## Current server surface

`pywire.server.serve` supports TCP connections, simple queries, optional
extended-query handlers, per-connection sessions, cleartext and SCRAM-SHA-256
authentication, TLS, cancel requests, and streaming query responses. The
synchronous `pywire.sync.serve_forever` is a convenience wrapper. `pywire.errors`
mirrors upstream error variants; `pywire.messages` exposes a foundational
subset of frontend and backend codecs.

An extended-query client requires an `ExtendedQueryHandler` or a session object
that implements its methods. There is no automatic forwarding from extended
query to the simple-query handler. Python `bind_portal` runs once for each Bind;
Describe and Execute receive that same Python portal object.

## Follow-up bindings

- `pywire.copy.CopyHandler` currently defines the Python callback shape only.
  Connect COPY FROM/TO messages to the server handler and add socket tests
  before documenting COPY as supported.
- Add remaining extended-query, COPY, and startup-handshake message codecs
  with encode/decode round-trip tests.
- Expand shared protocol types only when a public binding needs them; retain
  the existing flat module layout (`auth`, `query`, `copy`, `messages`).

Every binding change needs focused Rust tests for nontrivial conversion and
Python socket tests for behavior visible to PostgreSQL clients. The CI gate
checks formatting, lint, strict types, coverage, and cross-platform builds.
