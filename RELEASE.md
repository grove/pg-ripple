# pg_ripple — Release Procedure

This document describes how to release a new version of pg_ripple.

Versions follow the milestones in [ROADMAP.md](ROADMAP.md). Each release corresponds to a completed roadmap version (e.g. v0.1.0, v0.2.0).

---

## Pre-Release Checklist

Complete every item before starting the release process.

- [ ] **All roadmap deliverables for the version are implemented**
  - Cross-check against the version's deliverables list in [ROADMAP.md](ROADMAP.md)
- [ ] **All exit criteria in ROADMAP.md are satisfied**
  - Verify each criterion explicitly — do not rely on partial evidence
- [ ] **Tests pass**
  - `cargo fmt --all -- --check` (formatting)
  - `cargo clippy --features pg18 -- -D warnings` (lint, zero warnings)
  - `cargo pgrx test pg18` (unit + integration tests)
  - `cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"` (pg_regress suite)
- [ ] **`Cargo.toml` version field matches the release version**
  - e.g. `version = "0.2.0"` for a v0.2.0 release
- [ ] **`pg_ripple.control` `default_version` matches the release version**
- [ ] **CHANGELOG.md is up to date**
  - The `[Unreleased]` section has been moved under the new version heading
  - Written in plain, accessible language (see [Changelog Style](#changelog-style) below)
  - All significant user-visible changes are included
  - Date is set to today's date
- [ ] **No uncommitted changes** — `git status` is clean
- [ ] **Main branch is up to date** — `git pull origin main`

---

## Release Checklist

Perform these steps in order.

1. **Final test run**

   ```bash
   cargo fmt --all -- --check
   cargo clippy --features pg18 -- -D warnings
   cargo pgrx test pg18
   cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"
   ```

   All four must pass with zero warnings and zero failures.

2. **Tag the release**

   Use an annotated tag with the version number:

   ```bash
   git tag -a v0.X.Y -m "Release v0.X.Y — <version name from ROADMAP>"
   ```

   > **This step is done manually.** The release skill deliberately does not create tags.

3. **Push the tag**

   ```bash
   git push origin v0.X.Y
   ```

4. **Create a GitHub release**

   ```bash
   gh release create v0.X.Y --title "v0.X.Y — <version name>" --notes-file /tmp/release_notes.md
   ```

   The release notes file should contain the CHANGELOG entry for this version.

---

## Post-Release Checklist

- [ ] **Verify the GitHub release page** looks correct
  - `gh release view v0.X.Y`
- [ ] **Verify CI passed on the tagged commit**
  - Check the Actions tab or `gh run list --limit 3`
- [ ] **Update the `[Unreleased]` section in CHANGELOG.md**
  - Add an empty `[Unreleased]` section above the just-released version
  - Commit: `git commit -am "docs: start unreleased section after v0.X.Y"`
- [ ] **Announce the release** (if applicable)
  - Post to relevant channels, update project website, etc.
- [ ] **Verify the extension installs cleanly from the release**
  - On a fresh PostgreSQL 18 instance: `CREATE EXTENSION pg_ripple;`

---

## Changelog Style

The CHANGELOG.md should be written so that someone without deep knowledge of Rust, PostgreSQL internals, or RDF can understand what changed. Guidelines:

- **Lead with what users can do**, not how it was implemented
- Use short sentences and bullet points
- Avoid jargon — say "store and retrieve facts" instead of "triple CRUD via VP tables"
- Technical implementation details go in a separate "Technical Details" subsection for those who want them
- Each version section should open with a one-sentence summary

---

## Version Numbering

| Range | Meaning |
|-------|---------|
| 0.x.y | Pre-1.0 development milestones — features may change |
| 1.0.0 | Production release — stable API, standards compliance |
| 1.x.y | Post-1.0 enhancements (federation, Cypher/GQL, etc.) |
