# Versioning policy

pywire's version mirrors the upstream
[`pgwire`](https://crates.io/crates/pgwire) crate's major and minor numbers.
The patch component is independent and tracks binding-only changes.

## Format

```
pywire X.Y.Z
       │ │ └── pywire patch: independent. Bumps for binding fixes, doc updates,
       │ │    packaging fixes, and other changes that do not change behavior
       │ │    or upstream version.
       │ └─── pgwire minor: matches the upstream pgwire minor we wrap.
       └──── pgwire major: matches the upstream pgwire major we wrap.
```

Examples:

- `pywire 0.40.0` wraps `pgwire 0.40.x`
- `pywire 0.40.1` is a binding-only fix on top of the same `pgwire 0.40.x`
- `pywire 0.41.0` migrates to `pgwire 0.41.x`

The first release cut under this policy will be `0.40.0`. Versions prior to
that (the `0.1.x` scaffolding releases, if any) predate the policy and should
not be relied on for compatibility.

## Pre-1.0 caveat

pgwire is still in the `0.x` series. Under SemVer, every `0.y` bump may contain
breaking changes. pywire inherits that instability: until pgwire stabilizes at
`1.0`, every minor pywire release may also be breaking. **We do not offer
stability stronger than upstream.**

## Response to upstream releases

| Upstream change         | Action on pywire side                                                                                                              |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Patch (`0.Y.z+1`)       | Dependabot opens a PR. Merge, run tests, release pywire `0.Y.Z+1`.                                                                 |
| Minor (`0.Y+1.0`)       | The [upstream tracker workflow](.github/workflows/upstream-tracker.yml) opens an issue. Plan the migration, address API breakage, release pywire `0.Y+1.0`. |
| Major (`X+1.0.0`)       | Same as minor: tracker issue, migration, release pywire `X+1.0.0`.                                                                 |

## Yanking and security fixes

Bugs serious enough to warrant a yank are fixed on the most recent minor only.
We do not maintain release branches for older minors.

## How upstream releases are detected

Two complementary mechanisms:

1. **Dependabot** (`.github/dependabot.yml`) watches the cargo manifest and
   opens PRs for any new pgwire version inside the current SemVer range.
2. **Upstream tracker workflow**
   (`.github/workflows/upstream-tracker.yml`) runs weekly, hits the crates.io
   API, and opens a tracking issue when a new minor or major is published
   (those are outside the cargo SemVer range and Dependabot will not propose
   them).

The detection logic lives in
[`scripts/check_pgwire_release.py`](scripts/check_pgwire_release.py) and can
be run locally:

```bash
python scripts/check_pgwire_release.py
```
