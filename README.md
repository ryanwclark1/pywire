<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/ryanwclark1/pywire/main/docs/assets/logo-wordmark-dark.svg">
    <img alt="pywire" src="https://raw.githubusercontent.com/ryanwclark1/pywire/main/docs/assets/logo-wordmark.svg" width="300">
  </picture>
</p>

<p align="center">
  <a href="https://github.com/ryanwclark1/pywire/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/ryanwclark1/pywire/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://ryanwclark1.github.io/pywire/"><img alt="Docs" src="https://github.com/ryanwclark1/pywire/actions/workflows/docs.yml/badge.svg"></a>
  <a href="https://codecov.io/gh/ryanwclark1/pywire"><img alt="codecov" src="https://codecov.io/gh/ryanwclark1/pywire/graph/badge.svg"></a>
</p>

Python bindings for the Rust [`pgwire`](https://crates.io/crates/pgwire) library.

User documentation lives at **[ryanwclark1.github.io/pywire](https://ryanwclark1.github.io/pywire/)**.

## Development

```bash
pip install -e '.[dev]'
pytest -q
```

Project docs:

- [`VERSIONING.md`](VERSIONING.md) — version compatibility policy with upstream pgwire
- [`RELEASING.md`](RELEASING.md) — tagged build and Git commit pin workflow
- [`BINDING_STRATEGY.md`](BINDING_STRATEGY.md) — how we plan to surface pgwire to Python (read before opening a binding PR)
