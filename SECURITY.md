# Security policy

## Supported versions

pywire is pre-1.0; security fixes ship in the **latest minor only**.
For example, when `0.40.x` is current, fixes do not get backported to
`0.39.x` or earlier. The pywire major/minor mirrors upstream pgwire
per [`VERSIONING.md`](VERSIONING.md), so a security fix that requires
a pgwire upgrade may itself bump a pywire minor.

## Reporting a vulnerability

**Please do not open a public GitHub issue for security reports.**

Use GitHub's
[private vulnerability reporting](https://github.com/ryanwclark1/pywire/security/advisories/new)
on this repository. The maintainer is notified directly; the report
stays private until a fix ships.

Include:

- A description of the vulnerability and its impact.
- Steps to reproduce (proof-of-concept code is ideal).
- Affected version(s) of pywire (and pgwire, if known).
- Any disclosure constraints (CVE coordination, vendor embargoes,
  etc.).

You can expect:

- An acknowledgement within **2 working days**.
- A triage note within **5 working days** with a rough timeline and a
  classification of severity.
- A fix release as quickly as the complexity permits — see
  [`RELEASING.md`](RELEASING.md) for the publish flow.

## Scope

In scope:

- pywire itself (the Rust extension, Python facades, and CI tooling
  in this repository).
- Wheel build / release pipeline (anything that could ship malicious
  bits to PyPI).

Out of scope (please report upstream):

- Vulnerabilities in [`pgwire`](https://github.com/sunng87/pgwire),
  [`pyo3`](https://github.com/PyO3/pyo3), or other crates we depend
  on. We monitor `cargo audit` advisories in CI and bump as needed,
  but the original report should go to the upstream maintainers.
- Vulnerabilities in PostgreSQL itself.

## Hardening

pywire's CI pipeline runs:

- `cargo deny check advisories licenses sources bans` against the full
  dependency tree on every push.
- `actionlint` against the workflow files.
- Dependabot watches `Cargo.toml` and the workflow `uses:` lines.

If you notice gaps in any of the above, an issue or PR is welcome.
