# Binding strategy

This document explains how pywire decides which parts of the
[`pgwire`](https://crates.io/crates/pgwire) Rust crate to surface to Python,
and how those bindings are shaped. It is the source of truth for
every binding PR after it lands.

## Goals

1. **Mirror, don't reinvent.** A Python user who understands pgwire should
   recognize pywire one-for-one. Type names, message field names, and
   trait shapes follow upstream.
2. **Async by default.** PostgreSQL servers are I/O bound. pgwire is tokio.
   Python's analogue is asyncio. The binding bridges the two.
3. **Type-safe.** Every public surface ships `.pyi` stubs and is verified
   with mypy strict.
4. **Mechanical updates.** When pgwire bumps a minor, the bindings should
   need only the changes the upstream diff demands — no architectural
   rewrite per release.

## The async bridge

pgwire is built on tokio. Python's standard concurrency runtime is asyncio.
The two share an evented-I/O philosophy but are otherwise incompatible.

**Decision.** Use
[`pyo3-async-runtimes`](https://github.com/PyO3/pyo3-async-runtimes) (the
successor to `pyo3-asyncio`) to bridge them. Concretely:

- Rust spawns a tokio runtime owned by the extension module.
- Rust functions that return a Rust `Future` are exposed to Python as
  awaitables via `pyo3_async_runtimes::tokio::future_into_py`.
- Python coroutines passed into Rust handlers are awaited from Rust via
  `pyo3_async_runtimes::tokio::into_future`.

A user-facing handler looks like this:

```python
class MyQueryHandler(pywire.SimpleQueryHandler):
    async def do_query(self, client, query: str) -> list[pywire.Response]:
        # ... your logic here ...
        return [pywire.Response.row_description(...), ...]
```

**Why not sync wrappers as a primary API?** A sync API hides the very
property — concurrent connections — that makes a wire-protocol server
worth building. Once the async API exists, a sync convenience layer
(`pywire.sync.serve`) can wrap it for testing or for users who don't want
asyncio. That convenience layer is explicitly *not* a goal of v1.

**Why not trio or anyio?** asyncio is what the stdlib ships, what
`psycopg`, `asyncpg`, and FastAPI default to, and what mypy types well. We
can adopt anyio later if there is demand — `pyo3-async-runtimes` does not
preclude it.

## Module layout

```
pywire/
├── __init__.py            # re-exports the public surface
├── _pywire.pyi            # stubs for the compiled extension
├── py.typed
├── errors.py / .pyi       # exception hierarchy
├── messages/              # the codec layer
│   ├── __init__.py
│   ├── backend.py / .pyi  # PgWireBackendMessage variants
│   ├── frontend.py / .pyi # PgWireFrontendMessage variants
│   └── types.py / .pyi    # shared protocol types (DataRow, Tag, Oid, ...)
├── auth.py / .pyi         # StartupHandler / AuthSource analogues
├── query.py / .pyi        # SimpleQueryHandler / ExtendedQueryHandler ABCs
├── copy.py / .pyi         # CopyHandler ABCs
└── server.py / .pyi       # high-level pywire.serve(handler, addr)
```

The submodule names mirror pgwire's `messages`, `api::auth`, `api::query`,
and `api::copy`. The flat `auth.py`/`query.py`/`copy.py` collapses one
level relative to upstream so that imports stay short
(`from pywire.query import SimpleQueryHandler`).

## Error model

pgwire returns `Result<T, PgWireError>`. We map errors as follows:

| Rust                                  | Python                              |
| ------------------------------------- | ----------------------------------- |
| `PgWireError::ApiError(_)`            | `pywire.errors.ApiError`            |
| `PgWireError::UserError(ErrorInfo)`   | `pywire.errors.UserError`           |
| `PgWireError::InvalidStartupMessage`  | `pywire.errors.ProtocolError`       |
| `PgWireError::IoError(_)`             | propagated as Python `OSError`      |
| anything new in a future pgwire minor | new subclass of `pywire.errors.Error` |

All pywire-defined exceptions inherit from `pywire.errors.Error` so callers
can `except pywire.errors.Error:` if they want.

## Lifetimes and ownership

The binding boundary is the wrong place to be clever about lifetimes.
Rules:

- Message structs cross the boundary **owned**. We never expose
  borrow-typed handles to Python. `Bytes`/`BytesMut` are cloned at the
  boundary; Python objects are `Py<T>` (owned references).
- Mutable state on a handler lives in Python. The Rust side calls back
  into Python via `pyo3-async-runtimes`; the Python side owns its
  instance attributes. No shared `Arc<Mutex<_>>` across the boundary.
- One tokio runtime per extension module, held in a `OnceLock`. We do
  not let callers bring their own runtime.

These rules trade a small amount of cloning at the boundary for a much
simpler binding implementation. Performance budgeting is something we
will revisit once there is a binding-level benchmark to point at.

## Roadmap

Each item is one PR with tests and docs.

1. ✅ **Errors** (`pywire.errors`). The exception hierarchy + `ErrorInfo`
   + the `pywire_to_py_err` boundary helper. Shipped — every future
   binding calls `pywire_to_py_err` from its `PgWireResult` shim.
2. 🟡 **Messages: backend & frontend codecs** (`pywire.messages`).
   Foundational variants shipped: `Startup`, `Query`, `Terminate`
   (frontend); `ReadyForQuery`, `CommandComplete`, `RowDescription`,
   `DataRow`, `ErrorResponse` (backend). Per-class `encode()`/`decode()`
   with hypothesis round-trip property tests. Extended-query / COPY /
   handshake-extras messages remain.
3. **Shared types** (`pywire.messages.types`). `DataRow`,
   `FieldDescription`, `Tag`, `Oid`, etc. Trivial dataclass-shaped
   wrappers.
4. **Auth** (`pywire.auth`). The `StartupHandler` and `AuthSource`
   analogues, surfaced as Python ABCs the user subclasses. Cleartext +
   MD5 + SCRAM-SHA-256 first.
5. **Simple query** (`pywire.query.SimpleQueryHandler`). The smaller of
   the two query protocols. Implement first; extended query reuses the
   same response types.
6. **Extended query** (`pywire.query.ExtendedQueryHandler`). Prepared
   statements + portals.
7. **COPY** (`pywire.copy`). The bulk-transfer protocol.
8. **Server** (`pywire.server`). The high-level
   `await pywire.serve(handler, "127.0.0.1:5432")` entry point.
9. **Sync convenience** (`pywire.sync`). Optional layer wrapping the
   async API for users who don't want asyncio.

Items 1–3 are small and mechanical. 4–7 are the real work. 8 is glue.

## Testing each binding

Every binding PR ships:

- **Rust unit tests** (`#[cfg(test)] mod tests`) for any non-trivial Rust
  helper introduced.
- **Python unit tests** for the public Python surface.
- **Hypothesis property tests** wherever a round-trip identity exists
  (encode/decode, parse/format, etc.).
- **Integration tests** for the `server` PR: boot pywire, connect with
  `psycopg` and `asyncpg`, run real queries, assert results.
- **Documentation**: a page or section under `docs/` plus a docstring on
  every public symbol so mkdocstrings renders something useful.

## Interaction with versioning

[`VERSIONING.md`](VERSIONING.md) commits pywire's major.minor to track
pgwire's major.minor. When upstream bumps a minor, the corresponding
pywire release adjusts the bindings to the upstream diff and ships under
the matching version. Binding-only bug fixes ship as a pywire patch.

Breaking changes inside pgwire's `0.x` series translate into breaking
changes in pywire of the same series. We do not paper over upstream
breaks; we surface them honestly.

## Out of scope

The following are explicitly *not* binding goals:

- A query executor or storage layer.
- A Python *client* for talking to PostgreSQL. Use `psycopg` or `asyncpg`.
- An ORM, query builder, or DSL.
- Compatibility with PostgreSQL wire-protocol major 4 (does not exist).
- Compatibility with Rust async runtimes other than tokio.

## Open questions

- **`pyo3-stub-gen`** to auto-derive `.pyi` from Rust attribute macros vs.
  hand-written stubs. The latter is what we do today; the former buys us
  less drift but adds another build step. Revisit once we have more than
  five exposed types.
- **`trio` / `anyio` support**. Not blocking. Add if and when a user
  files an issue.
- **GIL release** during long-running handler calls. pyo3 lets us drop
  the GIL inside Rust; the binding boundary should default to dropping it
  for any blocking-style call. Will be addressed per-binding.
