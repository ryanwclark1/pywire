<!--
Thanks for opening a PR. The boilerplate below is a checklist, not a
rule book — feel free to delete sections that don't apply.

Use Conventional Commits in your commit subjects (`feat:`, `fix:`,
`docs:`, ...). `release-please` reads them to compute the next version
and the changelog. See `docs/contributing.md`.
-->

## What

<!-- One-or-two-sentence summary of what this changes. -->

## Why

<!-- The problem this fixes or the capability it adds. Link issues. -->

## How

<!-- The shape of the change — files touched, types added, design
notes that don't belong in code comments. -->

## Tests

<!-- What you added / changed. The CI gate is **100% line coverage on
hand-written Rust** (`scripts/coverage.sh`) **and** 100% Python
coverage. New code with no tests will fail the gate. -->

- [ ] New Rust code has `#[cfg(test)] mod tests` cases.
- [ ] New Python surface has pytest coverage.
- [ ] Hypothesis property tests added where an encode/decode or
      parse/format identity exists.
- [ ] `scripts/coverage.sh` passes locally.

## Docs

- [ ] `docs/api/*.md` updated for any public surface change.
- [ ] `BINDING_STRATEGY.md` roadmap status updated if a binding moved.
- [ ] `RELEASING.md` / `VERSIONING.md` updated if release flow changed.
