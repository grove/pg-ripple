# pg_ripple — Release Procedure

This document describes how to release a new version of pg_ripple.

Versions follow the milestones in [ROADMAP.md](ROADMAP.md). Each release corresponds to a completed roadmap version (e.g. v0.1.0, v0.2.0).

---

## Pre-Release Checklist

Complete every item before starting the release process.

- [ ] **All roadmap deliverables for the version are implemented**
  - Cross-check against the version's deliverables list in [ROADMAP.md](ROADMAP.md)
  - All deliverable checkboxes are ticked (`- [x]`) in ROADMAP.md — if any are unticked, tick them now before proceeding
- [ ] **All exit criteria in ROADMAP.md are satisfied**
  - Verify each criterion explicitly — do not rely on partial evidence
- [ ] **Tests pass**
  - `cargo fmt --all -- --check` (formatting)
  - `cargo clippy --features pg18 -- -D warnings` (lint, zero warnings)
  - `cargo pgrx test pg18` (unit + integration tests)
  - `cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"` (pg_regress suite, includes `schema_state` migration schema check)
  - `bash tests/test_migration_chain.sh` — **verify all migration SQL scripts apply cleanly in sequence** (requires `cargo pgrx start pg18` first; also run via `just test-migration`)
- [ ] **`Cargo.toml` version field matches the release version**
  - e.g. `version = "0.2.0"` for a v0.2.0 release
- [ ] **`pg_ripple.control` `default_version` matches the release version**
- [ ] **Extension migration script created** — **CRITICAL**
  - File: `sql/pg_ripple--X.(Y-1).Z--X.Y.Z.sql` where the previous version is X.(Y-1).Z
  - If there are schema changes (ALTER TABLE, CREATE INDEX, etc.), include them in the script
  - If there are no schema changes (new Rust functions), write only a comment header explaining what's new
  - See [Extension Versioning & Migration Scripts](AGENTS.md#extension-versioning--migration-scripts) in AGENTS.md for the checklist and examples
  - **Without this file, users on earlier versions cannot upgrade via `ALTER EXTENSION ... UPDATE`** — they must dump/restore
- [ ] **CHANGELOG.md is up to date**
  - The `[Unreleased]` section has been moved under the new version heading
  - Written in plain, accessible language (see [Changelog Style](#changelog-style) below)
  - All significant user-visible changes are included
  - Date is set to today's date
- [ ] **README.md is updated**
  - The "What works today (v0.X.Y)" section reflects the current release
  - Describes only functionality implemented and merged in this version
  - Remove planned features that haven't shipped yet
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
   bash tests/test_migration_chain.sh
   ```

   All five must pass with zero warnings and zero failures.

2. **Tag the release**

   Use an annotated tag with the version number:

   ```bash
   git tag -a v0.X.Y -m "Release v0.X.Y — <version name from ROADMAP>"
   git push origin v0.X.Y
   ```

   > **This step is done manually.** The release skill deliberately does not create tags.

3. **The GitHub release is created automatically**

   Pushing the tag triggers `.github/workflows/release.yml`, which:
   - Runs the full test + pg_regress suite on the tagged commit
   - Extracts the changelog entry for this version from `CHANGELOG.md`
   - Creates the GitHub release with the extracted notes

   Monitor the workflow:

   ```bash
   gh run list --limit 5
   gh run view <run-id>
   ```

---

## Post-Release Checklist

- [ ] **Verify the release workflow passed** — check `.github/workflows/release.yml` run on the tag
  - `gh run list --limit 5`
- [ ] **Verify the GitHub release page** looks correct
  - `gh release view v0.X.Y`
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
