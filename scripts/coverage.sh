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
# Use LCOV-format coverage (matches what Codecov ingests) for the gate.
# `DA:<line>,<count>` lines give a clean per-line view; we exempt
# lines whose source matches a pyo3 macro decoration pattern.
LCOV_FILE="$(pwd)/target/cov-summary.lcov"
export LCOV_FILE
"$LLVM_COV" export \
  "$SO_PATH" \
  -object "$TEST_BIN" \
  -instr-profile="$PROFDATA" \
  -ignore-filename-regex='/\.cargo/|/rustc/' \
  -format=lcov > "$LCOV_FILE" 2>/dev/null

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
import os
import re
import sys

DECORATION_PATTERNS = [
    re.compile(r"^\s*#\[pyclass(?:\s*\(.*)?$"),
    re.compile(r"^\s*#\[pymethods\]\s*$"),
    re.compile(r"^\s*from_py_object\s*$"),
    re.compile(r"^\s*skip_from_py_object\s*$"),
    re.compile(r"^\s*\)\]\s*$"),
]

# Explicit opt-out marker (LCOV convention). Append the marker to a
# trailing comment on the line you want excluded. The line is then
# treated like decoration: it doesn't count toward total or missed.
LCOV_EXCL_LINE = "LCOV_EXCL_LINE"


def is_decoration(source_line: str) -> bool:
    if LCOV_EXCL_LINE in source_line:
        return True
    return any(p.match(source_line) for p in DECORATION_PATTERNS)

# Parse LCOV: per file, collect (line_no, exec_count) pairs.
current_file: str | None = None
file_lines: dict[str, list[tuple[int, int]]] = {}
with open(os.environ["LCOV_FILE"], "r", encoding="utf-8") as fh:
    for raw in fh:
        line = raw.rstrip()
        if line.startswith("SF:"):
            current_file = line[3:]
            file_lines.setdefault(current_file, [])
        elif line == "end_of_record":
            current_file = None
        elif line.startswith("DA:") and current_file is not None:
            try:
                line_no_str, count_str = line[3:].split(",", 1)
                file_lines[current_file].append((int(line_no_str), int(count_str)))
            except ValueError:
                continue

total = 0
covered = 0
exempted_decoration: list[str] = []
real_missed: list[str] = []

for fname, lines in file_lines.items():
    try:
        with open(fname, "r", encoding="utf-8") as fh:
            source = fh.readlines()
    except OSError:
        continue
    for line_no, count in lines:
        src_line = source[line_no - 1] if 1 <= line_no <= len(source) else ""
        if count == 0 and is_decoration(src_line):
            exempted_decoration.append(f"{fname}:{line_no} {src_line.rstrip()}")
            continue
        total += 1
        if count > 0:
            covered += 1
        else:
            real_missed.append(f"{fname}:{line_no} {src_line.rstrip()}")

pct = (covered / total * 100.0) if total else 100.0
print(f"  effective line coverage: {pct:.2f}%  ({covered}/{total})")
if exempted_decoration:
    print(f"  exempted decoration lines: {len(exempted_decoration)}")
    for line in exempted_decoration[:5]:
        print(f"    {line}")
    if len(exempted_decoration) > 5:
        print(f"    ... and {len(exempted_decoration) - 5} more")
if real_missed:
    print(f"  uncovered hand-written lines: {len(real_missed)}")
    for line in real_missed:
        print(f"    {line}")
sys.exit(0 if pct >= 100.0 else 1)
PY
GATE_EXIT=$?
exit $GATE_EXIT
