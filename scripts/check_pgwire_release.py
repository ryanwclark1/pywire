#!/usr/bin/env python3
"""Check whether pgwire has a new release on crates.io.

Reads the pinned pgwire version from ``Cargo.toml`` and compares it against
the latest non-yanked, non-prerelease version on crates.io.

Outputs ``current``, ``latest``, ``status``, and ``action_needed`` to stdout
and, when running inside GitHub Actions, to ``$GITHUB_OUTPUT`` so downstream
steps can decide whether to open a tracking issue.

``status`` is one of:

- ``up-to-date``           : crates.io latest is at or below the pinned version.
- ``patch-available``      : new patch within the pinned minor; Dependabot
                             handles this via the cargo SemVer range.
- ``minor-or-major-available`` : new minor or major upstream; outside the
                             cargo SemVer range, so a manual migration is
                             required. ``action_needed=true``.

Exit code is always 0 so the workflow can read the outputs uniformly.
"""

from __future__ import annotations

import json
import os
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

CRATES_API = "https://crates.io/api/v1/crates/pgwire"
USER_AGENT = "pywire-upstream-tracker (https://github.com/ryanwclark1/pywire)"
VERSION_RE = re.compile(
    r'pgwire\s*=\s*\{[^}]*version\s*=\s*"([^"]+)"', re.MULTILINE
)


def read_pinned_version(cargo_toml: Path) -> str:
    text = cargo_toml.read_text(encoding="utf-8")
    match = VERSION_RE.search(text)
    if match is None:
        raise SystemExit(f"Could not find pgwire version pin in {cargo_toml}")
    return match.group(1)


def fetch_latest_stable() -> str:
    req = urllib.request.Request(CRATES_API, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.load(resp)
    except urllib.error.URLError as exc:
        raise SystemExit(f"crates.io request failed: {exc}") from exc

    for version in data.get("versions", []):
        num = version.get("num", "")
        if version.get("yanked"):
            continue
        if "-" in num:  # prerelease
            continue
        return num
    raise SystemExit("No stable pgwire version found on crates.io")


def parse(version: str) -> tuple[int, int, int]:
    parts = version.split(".")
    if len(parts) < 3:
        raise SystemExit(f"Unexpected version format: {version!r}")
    return int(parts[0]), int(parts[1]), int(parts[2])


def classify(current: str, latest: str) -> tuple[str, bool]:
    cur = parse(current)
    lat = parse(latest)
    if lat <= cur:
        return "up-to-date", False
    if (lat[0], lat[1]) == (cur[0], cur[1]):
        return "patch-available", False
    return "minor-or-major-available", True


def emit(values: dict[str, str]) -> None:
    for key, val in values.items():
        print(f"{key}={val}")
    gh_output = os.environ.get("GITHUB_OUTPUT")
    if gh_output:
        with open(gh_output, "a", encoding="utf-8") as fh:
            for key, val in values.items():
                fh.write(f"{key}={val}\n")


def main() -> int:
    cargo_toml = Path(__file__).resolve().parent.parent / "Cargo.toml"
    current = read_pinned_version(cargo_toml)
    latest = fetch_latest_stable()
    status, action_needed = classify(current, latest)
    emit(
        {
            "current": current,
            "latest": latest,
            "status": status,
            "action_needed": "true" if action_needed else "false",
        }
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
