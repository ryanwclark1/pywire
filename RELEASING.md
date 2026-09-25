# Releasing

pywire is installed from this repository at an immutable Git commit. The
`pywire` project on PyPI belongs to a different library; do not publish this
package there or recommend `pip install pywire`.

## Prepare a version

1. Check the [versioning policy](https://github.com/ryanwclark1/pywire/blob/main/VERSIONING.md) and the upstream pgwire changelog.
2. Update `Cargo.toml`, `Cargo.lock`, and `pyproject.toml` together. A new
   upstream minor starts at the matching pywire `X.Y.0`; binding-only changes
   increment pywire's patch version.
3. Update `CHANGELOG.md` and the documentation's version references.
4. Run the complete CI gate and build a wheel from a clean checkout.
5. Merge the reviewed change, then tag the exact commit as `vX.Y.Z`.

The tag runs `.github/workflows/release.yml`, which verifies the tagged source
and builds wheels and an sdist as GitHub Actions artifacts. Those artifacts
validate packaging; consumers continue to install from a pinned Git commit.
No workflow publishes to PyPI or creates a public GitHub Release.

## Update consumers

For Accent BI, update the `pywire` Git revision and version constraint in its
root `pyproject.toml`, regenerate its `uv.lock`, and run its SQL endpoint tests.
Pin a full commit SHA so builds remain reproducible. Revert both the revision
and constraint to the previous commit to roll back.

A direct pip installation uses the same source model:

```bash
pip install 'pywire @ git+https://github.com/ryanwclark1/pywire.git@<full-commit-sha>'
```

The machine building from Git needs Rust and Python 3.11 or newer.
