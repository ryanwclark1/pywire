# pywire

[![CI](https://github.com/ryanwclark1/pywire/actions/workflows/ci.yml/badge.svg)](https://github.com/ryanwclark1/pywire/actions/workflows/ci.yml)

Python bindings for the Rust [`pgwire`](https://crates.io/crates/pgwire) library.

## Development

```bash
pip install -e '.[dev]'
pytest -q
```

See [`VERSIONING.md`](VERSIONING.md) for the version compatibility policy
with upstream pgwire.
