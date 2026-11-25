---
title: pywire
---

<img class="pywire-hero pywire-hero--light" src="assets/logo-wordmark.svg" alt="pywire">
<img class="pywire-hero pywire-hero--dark" src="assets/logo-wordmark-dark.svg" alt="pywire">

[![CI](https://github.com/ryanwclark1/pywire/actions/workflows/ci.yml/badge.svg)](https://github.com/ryanwclark1/pywire/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/ryanwclark1/pywire/graph/badge.svg)](https://codecov.io/gh/ryanwclark1/pywire)

Python bindings for the Rust [`pgwire`](https://crates.io/crates/pgwire) crate.

pywire lets you build PostgreSQL-wire-protocol services in Python with the
performance and correctness of the underlying Rust implementation. The
binding surface tracks pgwire one-for-one — see
[Versioning](versioning.md) for the compatibility policy.

!!! warning "Status: pre-1.0"
    pywire follows pgwire's `0.x` versioning, so every minor release may
    contain breaking changes. See [Versioning](versioning.md).

## Why pywire?

PostgreSQL's wire protocol is an attractive target for tools that want to
look like a Postgres server (proxies, query routers, mock backends, custom
analytics engines) without reimplementing the protocol from scratch. The
Rust `pgwire` crate handles framing, message types, the startup handshake,
authentication, and both the simple and extended query protocols. pywire
makes that machinery callable from Python.

## Features

- **Simple and extended query protocols**: plain SQL, prepared statements,
  binary formats and portal suspension.
- **Streaming results** from async iterables, with wire-level backpressure.
- **COPY FROM STDIN / TO STDOUT** in text, CSV and binary formats.
- **Authentication**: cleartext, SCRAM-SHA-256 and SCRAM against stored
  PostgreSQL verifiers, with channel binding over TLS.
- **TLS**, including required TLS and mutual TLS with client certificates.
- **Per-connection sessions**, cancel requests and graceful shutdown.
- **Precise errors**: every pgwire error class, plus any SQLSTATE through
  `ErrorInfo`.
- **Wire codecs** for building proxies, tests and protocol tools.
- **Fully typed**: inline types, verified stubs, and mypy and pyright in
  strict mode.

## Get started

- [Install pywire](install.md)
- [Run the quickstart](quickstart.md)
- [Read the wire-protocol concepts](concepts.md)
- [Browse the user guide](guides/simple-queries.md)

## Reference

- [API reference](api.md)
- [Versioning policy](versioning.md)
- [Releasing](releasing.md)
- [Contributing](contributing.md)
