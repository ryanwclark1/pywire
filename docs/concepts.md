# Wire protocol concepts

PostgreSQL clients and servers exchange a stream of typed messages over a
plain TCP socket (optionally upgraded to TLS). pywire — through pgwire —
takes care of the bytes; you write the business logic.

## Two phases

```
┌─────────────────┐         ┌─────────────────┐
│   Startup       │ ──────▶ │  Steady state   │
│                 │         │                 │
│ • Version       │         │ • Simple query  │
│ • SSL upgrade   │         │ • Extended      │
│ • Auth          │         │   query         │
│ • Parameters    │         │ • COPY          │
└─────────────────┘         │ • Notifications │
                            └─────────────────┘
```

1. **Startup.** Client and server negotiate the protocol version, optionally
   negotiate TLS, authenticate, and exchange the initial parameter set.
2. **Steady state.** The session enters a request/response loop until either
   side closes the connection.

The canonical reference for the protocol itself is
[PostgreSQL: Frontend/Backend Protocol](https://www.postgresql.org/docs/current/protocol.html).

## Message framing

Every steady-state message starts with a one-byte type tag, followed by a
big-endian 32-bit length (inclusive of the length itself), followed by the
body. Startup messages omit the type tag for historical reasons.

pgwire decodes these into Rust enums; pywire surfaces them (when the
bindings land) as Python classes whose fields correspond to the protocol
fields one-to-one. There is no translation layer: the goal is to expose
pgwire's semantics, not to invent new ones.

## Layered API

pgwire — and therefore pywire — is layered:

| Layer       | What it is                                          | Best for                           |
| ----------- | --------------------------------------------------- | ---------------------------------- |
| `messages`  | Raw codecs for `PgWireFrontendMessage` / `PgWireBackendMessage` | tooling: proxies, fuzzers, dissectors |
| `api`       | Handler traits (auth, simple query, extended query, COPY) | servers: implement the traits, plug them into a runtime |
| `tokio`     | Server bootstrap on top of tokio                    | end-to-end use: bind a port and accept connections |

## What pywire does not give you

- **A database.** pywire is the protocol surface. The query execution, the
  storage, the catalog — those are yours to provide (or to delegate to
  another database).
- **A client.** Use `psycopg`, `asyncpg`, or any standard PostgreSQL
  client to talk *to* a pywire-powered server. pywire is the server side.
