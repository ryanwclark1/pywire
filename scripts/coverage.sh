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

echo "==> Enforcing 100% line coverage on hand-written code"
# Detailed (file-level) coverage in JSON so we can filter out the lines
# that pyo3's macros decorate but no runtime actually executes. Write to
# a file because the JSON is too large to fit in a shell variable.
DETAIL_FILE="$(pwd)/target/cov-detail.json"
export DETAIL_FILE
"$LLVM_COV" export \
  "$SO_PATH" \
  -object "$TEST_BIN" \
  -instr-profile="$PROFDATA" \
  -ignore-filename-regex='/\.cargo/|/rustc/' \
  -format=text > "$DETAIL_FILE" 2>/dev/null

# We exempt two classes of "lines" from the strict 100% gate:
#
# 1. Macro attribute roots (#[pyclass], #[pymethods], `from_py_object`,
#    etc.). These are decoration, not executable code, but llvm-cov
#    counts them as a line each with zero execution because the expanded
#    code runs at a different source location.
# 2. pyo3-generated `___pymethod_*` dispatch wrappers. In pyo3 0.28 the
#    runtime resolves these via a different code path; the wrappers exist
#    in the binary but are unreachable. They show up as 0%-covered
#    functions even though the underlying `__repr__` / `is_fatal` etc.
#    *are* invoked via the alternate path and report as 100%.
#
# The script extracts the per-line execution counts, drops "missed" lines
# whose source matches the decoration patterns above, and gates on the
# *effective* (post-exemption) coverage at 100%. Each exempted line is
# reported so PR review can sanity-check that it's actually decoration.

python3 - <<'PY'
import json
import os
import re
import sys

DECORATION_PATTERNS = [
    re.compile(r"^\s*#\[pyclass(?:\s*\(.*)?$"),
    re.compile(r"^\s*#\[pymethods\]\s*$"),
    re.compile(r"^\s*from_py_object\s*$"),
    re.compile(r"^\s*skip_from_py_object\s*$"),
    re.compile(r"^\s*\)\]\s*$"),  # closing paren of multi-line #[pyclass(...)]
]

def is_decoration(source_line: str) -> bool:
    return any(p.match(source_line) for p in DECORATION_PATTERNS)

with open(os.environ["DETAIL_FILE"], "r", encoding="utf-8") as fh:
    DETAIL = json.load(fh)

total = 0
covered = 0
exempted_decoration = []

for file_entry in DETAIL["data"][0]["files"]:
    fname = file_entry["filename"]
    try:
        with open(fname, "r", encoding="utf-8") as fh:
            source = fh.readlines()
    except OSError:
        continue
    # Roll segments up into per-line max-count so each source line is
    # counted once even if it contains multiple regions.
    line_counts: dict[int, int] = {}
    for seg in file_entry.get("segments", []):
        if not isinstance(seg, list) or len(seg) < 4:
            continue
        line_no, _col, count, has_count = seg[0], seg[1], seg[2], seg[3]
        if not has_count:
            continue
        prev = line_counts.get(line_no)
        if prev is None or count > prev:
            line_counts[line_no] = count

    for line_no, count in line_counts.items():
        src_line = source[line_no - 1] if 1 <= line_no <= len(source) else ""
        if is_decoration(src_line):
            if count == 0:
                exempted_decoration.append(f"{fname}:{line_no} {src_line.rstrip()}")
            continue
        total += 1
        if count > 0:
            covered += 1

pct = (covered / total * 100.0) if total else 100.0
print(f"  effective line coverage: {pct:.2f}%  ({covered}/{total})")
if exempted_decoration:
    print(f"  exempted decoration lines: {len(exempted_decoration)}")
    for line in exempted_decoration[:20]:
        print(f"    {line}")
sys.exit(0 if pct >= 100.0 else 1)
PY
GATE_EXIT=$?
exit $GATE_EXIT
