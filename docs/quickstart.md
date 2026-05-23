# Quickstart

Today pywire exposes a single function: the range of PostgreSQL
wire-protocol versions supported by the wrapped pgwire crate. The binding
surface will grow PR by PR — see the
[binding strategy](https://github.com/ryanwclark1/pywire/blob/main/BINDING_STRATEGY.md)
for the roadmap.

```python
import pywire

earliest, latest = pywire.supported_protocol_range()
print(f"pywire wraps a pgwire that speaks protocol versions {earliest}..{latest}")
```

That's it. As soon as more bindings land they'll show up under the
[API reference](api.md).

## What's coming

The next binding milestones, in priority order:

1. **Messages** — `PgWireBackendMessage` / `PgWireFrontendMessage` codecs.
   You'll be able to encode and decode wire frames from Python.
2. **Auth handlers** — a Python ABC mirroring pgwire's `StartupHandler`
   trait, so you can plug your own authentication into a server.
3. **Query handlers** — both the simple and extended query protocols.
4. **Server bootstrap** — `pywire.serve(handler, addr)` for end-to-end use.

If you need any of these now, file an issue on
[GitHub](https://github.com/ryanwclark1/pywire/issues) — interest helps us
prioritize.
