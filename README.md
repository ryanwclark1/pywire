# pywire

[![CI](https://github.com/ryanwclark1/pywire/actions/workflows/ci.yml/badge.svg)](https://github.com/ryanwclark1/pywire/actions/workflows/ci.yml)
[![Docs](https://github.com/ryanwclark1/pywire/actions/workflows/docs.yml/badge.svg)](https://ryanwclark1.github.io/pywire/)
[![codecov](https://codecov.io/gh/ryanwclark1/pywire/graph/badge.svg)](https://codecov.io/gh/ryanwclark1/pywire)

Python bindings for the Rust [`pgwire`](https://crates.io/crates/pgwire) library.

User documentation lives at **[ryanwclark1.github.io/pywire](https://ryanwclark1.github.io/pywire/)**.

## Development

```bash
pip install -e '.[dev]'
pytest -q
```

Project docs:

- [`VERSIONING.md`](VERSIONING.md) — version compatibility policy with upstream pgwire
- [`RELEASING.md`](RELEASING.md) — release flow, PyPI Trusted Publishing setup
- [`BINDING_STRATEGY.md`](BINDING_STRATEGY.md) — how we plan to surface pgwire to Python (read before opening a binding PR)
