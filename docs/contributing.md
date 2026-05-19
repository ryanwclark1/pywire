# Contributing

Thanks for considering a contribution. This page explains the
local-development workflow; the
[versioning policy](versioning.md) and
[release process](releasing.md) live on their own pages, and the
[binding strategy](https://github.com/ryanwclark1/pywire/blob/main/BINDING_STRATEGY.md)
explains how new pgwire surface should be exposed to Python — read it
before opening a PR that adds a new binding.

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

# Combined coverage gate (also enforced in CI)
bash scripts/coverage.sh
```

After any change to the Rust source, rebuild the extension before re-running
pytest:

```bash
maturin develop --release
```

## Coverage policy

Both Python and Rust must maintain **100% line and function coverage**.
The gate is enforced in CI by `scripts/coverage.sh` and re-verified in the
release workflow before any wheel is published.

If a line genuinely cannot be reached from a test, mark it `# pragma: no
cover` on the Python side and explain why in a one-line comment that gets
reviewed in the PR diff. Rust currently lacks a clean stable-channel
inline exclusion mechanism; we work around it by exempting region
coverage from the gate (the macro-expanded `?` paths inside
`wrap_pyfunction!`/`pymodule` can't be reached from happy-path tests on
stable Rust). Region coverage is reported as informational only.

## Commit message style

This repo follows
[Conventional Commits](https://www.conventionalcommits.org/). The release
tooling (`release-please`) reads commit subjects to compute the next
version and to generate `CHANGELOG.md`. Use these subject prefixes:

- `feat:` — new public surface (any binding addition)
- `fix:` — bug fix
- `docs:` — docs-only change
- `test:` — test-only change
- `ci:` — CI/workflow change
- `chore:` — release/scaffolding/bookkeeping
- `refactor:` / `perf:` — internal change

Breaking changes go in the body, prefixed `BREAKING CHANGE:` (or use
`feat!:` / `fix!:` shorthand). release-please bumps the major (or, in our
pre-1.0 phase, the minor) on `BREAKING CHANGE:` commits.

## Security and supply chain

Run `cargo deny check` locally before pushing. CI runs the same command.
`deny.toml` at the repo root configures allowed licenses, advisory policy,
and source registries.

## Pre-commit hooks

A `.pre-commit-config.yaml` at the repo root mirrors the CI lint job
(`ruff`, `ruff format`, `cargo fmt`, `cargo clippy`, `mypy`, plus a
few hygienic checks). Optional but recommended:

```bash
pip install pre-commit
pre-commit install
```

`pre-commit` then runs on every commit. Run against the full tree with
`pre-commit run --all-files`.

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
