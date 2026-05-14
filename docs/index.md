# pywire

Python bindings for the Rust [`pgwire`](https://crates.io/crates/pgwire) crate.

pywire lets you build PostgreSQL-wire-protocol services in Python with the
performance and correctness of the underlying Rust implementation. The
binding surface tracks pgwire one-for-one — see
[Versioning](versioning.md) for the compatibility policy.

!!! warning "Status: experimental"
    pywire is in active development. The binding surface is small today
    and grows in tandem with PRs. See the
    [binding strategy](https://github.com/ryanwclark1/pywire/blob/main/BINDING_STRATEGY.md)
    for what's planned.

## Why pywire?

PostgreSQL's wire protocol is an attractive target for tools that want to
look like a Postgres server (proxies, query routers, mock backends, custom
analytics engines) without reimplementing the protocol from scratch. The
Rust `pgwire` crate handles framing, message types, the startup handshake,
authentication, and both the simple and extended query protocols. pywire
makes that machinery callable from Python.

## Get started

- [Install pywire](install.md)
- [Run the quickstart](quickstart.md)
- [Read the wire-protocol concepts](concepts.md)

## Reference

- [API reference](api.md)
- [Versioning policy](versioning.md)
- [Releasing](releasing.md)
- [Contributing](contributing.md)
