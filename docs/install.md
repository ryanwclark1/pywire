# Installation

pywire is distributed from this Git repository. The `pywire` name on PyPI
belongs to a different library, so `pip install pywire` does not install these
PostgreSQL server bindings.

## Pin a source commit

Install from an immutable commit after the project's CI checks pass:

```bash
pip install 'pywire @ git+https://github.com/ryanwclark1/pywire.git@<full-commit-sha>'
```

For a uv project, declare `pywire` in dependencies and pin the same commit
under `[tool.uv.sources]`:

```toml
[tool.uv.sources]
pywire = { git = "https://github.com/ryanwclark1/pywire.git", rev = "<full-commit-sha>" }
```

A Git installation builds the native extension locally. It requires Python
3.11 or newer, a Rust toolchain (at least Rust 1.89 for pgwire 0.41), and a
working C compiler. The tagged build workflow also validates wheels for
Linux, macOS, and Windows, but those artifacts are not the distribution path.

## From a checkout

```bash
git clone https://github.com/ryanwclark1/pywire.git
cd pywire
pip install -e '.[dev]'
```

After changing Rust code, rerun `pip install -e .` to rebuild the extension.

## Verify

```bash
python -c 'import pywire; print(pywire.supported_protocol_range())'
```
