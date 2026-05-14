#!/usr/bin/env bash
# Run the full Python + Rust coverage pipeline and fail if either falls
# below 100% line coverage.
#
# Rust coverage on a pyo3 extension is non-trivial because the public
# surface (the #[pyfunction] / #[pymodule] glue) is only exercised when
# Python imports the compiled .so. cargo-llvm-cov's `report` subcommand
# doesn't know how to pull coverage data out of a binary it didn't build.
# So we drive the LLVM tooling directly:
#
#   1. Set RUSTFLAGS=-Cinstrument-coverage and rebuild the .so via
#      pip install -e (which delegates to maturin). RUSTFLAGS propagates,
#      so the compiled extension is instrumented.
#   2. Run cargo test and pytest under the same env; both produce .profraw
#      files into $LLVM_PROFILE_FILE.
#   3. Merge the .profraws with llvm-profdata.
#   4. Report against BOTH the cargo test binary AND the .so, so coverage
#      of the binding glue is counted.
#
# We gate on line + function coverage at 100%. Region coverage is
# emitted but not gated: the few macro-expanded defensive `?` paths
# (e.g. inside `wrap_pyfunction!`) can't be exercised from happy-path
# tests and stable Rust has no clean inline exclusion for them.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PROF_DIR="$REPO_ROOT/target/cov-profraw"
PROFDATA="$REPO_ROOT/target/cov.profdata"

LLVM_PROFDATA=$(rustup which llvm-profdata 2>/dev/null || true)
LLVM_COV=$(rustup which llvm-cov 2>/dev/null || true)
if [[ -z "$LLVM_PROFDATA" ]]; then
  LLVM_PROFDATA=$(find ~/.rustup/toolchains -name llvm-profdata 2>/dev/null | head -1)
fi
if [[ -z "$LLVM_COV" ]]; then
  LLVM_COV=$(find ~/.rustup/toolchains -name llvm-cov 2>/dev/null | head -1)
fi
if [[ -z "$LLVM_PROFDATA" || -z "$LLVM_COV" ]]; then
  echo "ERROR: llvm-profdata / llvm-cov not found. Install via:" >&2
  echo "  rustup component add llvm-tools-preview" >&2
  exit 1
fi

rm -rf "$PROF_DIR" "$PROFDATA"
mkdir -p "$PROF_DIR"

export RUSTFLAGS="-Cinstrument-coverage"
export LLVM_PROFILE_FILE="$PROF_DIR/profile-%p-%m.profraw"
export CARGO_INCREMENTAL=0

echo "==> Cleaning prior binding artifacts"
# Stale non-instrumented binaries silently produce wrong coverage numbers.
# We wipe the .so and the pywire-* test binaries (not the whole target/
# so that cargo's cache and Swatinem/rust-cache remain useful in CI).
rm -f pywire/_pywire*.so
rm -f target/debug/deps/pywire-*
rm -f target/debug/deps/libpywire-*

echo "==> Rebuilding instrumented extension"
pip install --force-reinstall --no-deps --quiet -e .

echo "==> Running cargo test"
cargo test --quiet

echo "==> Running pytest"
python3 -m pytest -q

# Discover binaries to report against.
SO_PATH="$(find pywire -name '_pywire*.so' -type f | head -1)"
TEST_BIN="$(find target/debug/deps -name 'pywire-*' -type f -executable ! -name '*.d' ! -name '*.so' | head -1)"
if [[ -z "$SO_PATH" || -z "$TEST_BIN" ]]; then
  echo "ERROR: could not locate .so ($SO_PATH) or test binary ($TEST_BIN)" >&2
  exit 1
fi

echo "==> Merging .profraw files"
"$LLVM_PROFDATA" merge -sparse "$PROF_DIR"/*.profraw -o "$PROFDATA"

echo "==> Exporting LCOV for Codecov"
"$LLVM_COV" export \
  "$SO_PATH" \
  -object "$TEST_BIN" \
  -instr-profile="$PROFDATA" \
  -ignore-filename-regex='/\.cargo/|/rustc/' \
  -format=lcov > rust-cov.lcov

echo "==> Coverage summary"
"$LLVM_COV" report \
  "$SO_PATH" \
  -object "$TEST_BIN" \
  -instr-profile="$PROFDATA" \
  -ignore-filename-regex='/\.cargo/|/rustc/'

echo "==> Enforcing 100% line + function coverage"
JSON=$("$LLVM_COV" export \
  "$SO_PATH" \
  -object "$TEST_BIN" \
  -instr-profile="$PROFDATA" \
  -ignore-filename-regex='/\.cargo/|/rustc/' \
  -summary-only \
  -format=text 2>/dev/null)

python3 - <<PY
import json, sys
totals = json.loads("""$JSON""")["data"][0]["totals"]
lines = totals["lines"]["percent"]
funcs = totals["functions"]["percent"]
regions = totals["regions"]["percent"]
print(f"  lines:     {lines:.2f}%")
print(f"  functions: {funcs:.2f}%")
print(f"  regions:   {regions:.2f}% (informational)")
ok = lines >= 100.0 and funcs >= 100.0
sys.exit(0 if ok else 1)
PY
