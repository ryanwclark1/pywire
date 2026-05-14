# Contributing

Thanks for considering a contribution. This page explains the
local-development workflow; the
[versioning policy](versioning.md) and
[release process](releasing.md) live on their own pages.

## Development setup

```bash
git clone https://github.com/ryanwclark1/pywire
cd pywire
pip install -e '.[dev]'
```

That installs the package in editable mode plus `pytest`, `ruff`, `mypy`,
and `maturin`. You'll also need a Rust toolchain (`stable`) — the
`rust-toolchain.toml` at the repo root pins it.

## The check loop

The same commands CI runs, in the order CI runs them:

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test

# Python
ruff check .
ruff format --check .
mypy
pytest -q
```

After any change to the Rust source, rebuild the extension before re-running
pytest:

```bash
maturin develop --release
```

## Security and supply chain

Run `cargo deny check` locally before pushing. CI runs the same command.
`deny.toml` at the repo root configures allowed licenses, advisory policy,
and source registries.

## Workflow files

If you touch `.github/workflows/*.yml`, run
[actionlint](https://github.com/rhysd/actionlint) locally:

```bash
actionlint
```

CI runs the same binary; failing locally first is faster than waiting on a
red CI run.

## Submitting a change

1. Branch off `main`.
2. Make the change. Keep PRs focused: one concern per PR.
3. Add tests. New Rust code → `#[cfg(test)] mod tests`. New Python code →
   `tests/test_<thing>.py`.
4. Open a PR. CI will run the full lint + test matrix and the security
   audit.

## Reporting an issue

Open a [GitHub issue](https://github.com/ryanwclark1/pywire/issues). For
security-sensitive reports, use private disclosure on the same repo
(`Security` tab → `Report a vulnerability`).
