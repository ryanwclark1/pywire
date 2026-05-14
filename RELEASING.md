# Releasing

pywire is published to [PyPI](https://pypi.org/project/pywire/) via
GitHub Actions using
[PyPI Trusted Publishing](https://docs.pypi.org/trusted-publishers/) (OIDC).
No long-lived API tokens are stored anywhere; the workflow exchanges its
GitHub OIDC token for a short-lived PyPI upload token at publish time.

The version policy lives in [`VERSIONING.md`](VERSIONING.md). Read it before
cutting a release.

## One-time setup (project maintainers)

1. Configure pywire as a [trusted publisher on PyPI](https://pypi.org/manage/account/publishing/):
   - PyPI Project Name: `pywire`
   - Owner: `ryanwclark1`
   - Repository: `pywire`
   - Workflow filename: `release.yml`
   - Environment name: `pypi`
2. Same for TestPyPI at
   [test.pypi.org](https://test.pypi.org/manage/account/publishing/) with
   Environment name: `testpypi`.
3. Create `pypi` and `testpypi`
   [GitHub Environments](https://docs.github.com/en/actions/deployment/targeting-different-environments/managing-environments-for-deployment)
   in this repo. Add required reviewers to the `pypi` environment so production
   releases require human approval.

## Per-release checklist

1. Decide the new version per [`VERSIONING.md`](VERSIONING.md).
2. Update `Cargo.toml` and `pyproject.toml` to the new version. Both must
   match the tag, or the release workflow's guard step fails.
3. Update `Cargo.lock` (`cargo update -p pywire`) and `CHANGELOG.md` if
   present.
4. Commit, open a PR, get it merged to `main`.
5. From `main`:
   ```bash
   git pull
   git tag -s "vX.Y.Z" -m "vX.Y.Z"
   git push origin "vX.Y.Z"
   ```
6. The `Release` workflow:
   - builds wheels for manylinux + musllinux (x86_64 + aarch64), macOS
     universal2, Windows x86_64, plus an sdist;
   - publishes everything to PyPI via Trusted Publishing;
   - creates a GitHub Release with auto-generated notes.
7. Verify the release on
   [PyPI](https://pypi.org/project/pywire/#history) and install it in a
   clean venv:
   ```bash
   python -m venv /tmp/pywire-verify && source /tmp/pywire-verify/bin/activate
   pip install "pywire==X.Y.Z"
   python -c "import pywire; print(pywire.supported_protocol_range())"
   ```

## Dry-run to TestPyPI

Before a real release, smoke-test the pipeline:

1. Bump the version in `Cargo.toml` and `pyproject.toml` to something not
   yet on TestPyPI (e.g. append `.devN`).
2. Trigger the **Release** workflow manually:
   `Actions` → `Release` → `Run workflow` → `target: testpypi`.
3. Confirm the artifacts land on
   [test.pypi.org/project/pywire](https://test.pypi.org/project/pywire/).

The TestPyPI publish job does not require a tag and uses the `testpypi`
environment, so production credentials are never involved.

## Yanking a release

If a published release is broken, yank it from PyPI immediately
([PyPI yank docs](https://pypi.org/help/#yanked)) and ship a corrected
patch release per [`VERSIONING.md`](VERSIONING.md). We do not maintain
release branches for older minors.
