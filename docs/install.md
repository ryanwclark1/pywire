# Installation

pywire ships pre-built wheels for the following targets:

| OS      | Architectures             | libc       |
| ------- | ------------------------- | ---------- |
| Linux   | `x86_64`, `aarch64`       | glibc, musl |
| macOS   | `x86_64` + `arm64` (universal2) | — |
| Windows | `x86_64`                  | —          |

Python **`>=3.11`** is supported (3.11 / 3.12 / 3.13 / 3.14). Wheels use
the stable `abi3` interface (`abi3-py311`), so a single wheel per platform
covers every Python in the supported range. Older Python versions are
not supported: 3.9 reached end-of-life in October 2025 and 3.10 follows
in October 2026; pywire targets the asyncio.TaskGroup / Self / ExceptionGroup
surface that landed in 3.11.

## From PyPI

```bash
pip install pywire
```

To pin to the upstream pgwire minor we wrap, install an exact version per
the policy in [Versioning](versioning.md):

```bash
pip install 'pywire~=0.40.0'
```

## From source

You need a Rust toolchain (`stable`) and Python `>=3.11`:

```bash
git clone https://github.com/ryanwclark1/pywire
cd pywire
pip install -e '.[dev]'
```

The editable install runs `maturin develop` under the hood. For a release
build use:

```bash
maturin develop --release
```

## Verifying the install

```bash
python -c "import pywire; print(pywire.supported_protocol_range())"
```

You should see a tuple like `(3, 3)` — the range of PostgreSQL wire-protocol
versions the wrapped pgwire crate understands.
