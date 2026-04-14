---
name: release
description: 'Release a pg_ripple version. Use when: tagging a release, preparing release notes, running the release checklist, creating a GitHub release. Covers pre-release verification, changelog finalization, GitHub release creation.'
argument-hint: 'Specify the version to release, e.g., "v0.1.0" or "v0.3.0"'
---

# Release pg_ripple Version

## Authoritative Sources

Always read these before starting a release:

- [RELEASE.md](../../../RELEASE.md) — full release procedure with pre-release, release, and post-release checklists
- [ROADMAP.md](../../../ROADMAP.md) — exit criteria for the version being released
- [CHANGELOG.md](../../../CHANGELOG.md) — changelog to update
- [AGENTS.md](../../../AGENTS.md) — git workflow (especially PR creation rules)

## Procedure

### 1. Verify the version is ready

Run every check in the RELEASE.md pre-release checklist:

```bash
cargo fmt --all -- --check
cargo clippy --features pg18 -- -D warnings
cargo pgrx test pg18
cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"
```

All four must pass with zero warnings and zero failures. Do not proceed if any check fails.

### 2. Verify version numbers match

```bash
grep '^version' Cargo.toml
grep 'default_version' pg_ripple.control
```

Both must show the target version (e.g. `0.2.0`).

### 3. Review the ROADMAP exit criteria

Open the target version section in ROADMAP.md and verify each exit criterion explicitly. List them with pass/fail.

### 4. Update CHANGELOG.md

#### 4a. Gather changes from git

```bash
# If there's a previous tag:
git log --oneline v0.X.Y..HEAD

# If this is the first release:
git log --oneline
```

#### 4b. Write the changelog entry

Follow the style in RELEASE.md § Changelog Style:

1. **One-sentence summary** at the top — what this version delivers
2. **"What you can do"** section — user-facing capabilities in plain language
3. **"What happens behind the scenes"** section — non-technical explanation of internals
4. **"Technical Details"** section — in a `<details>` collapsible block for developers

Move the `[Unreleased]` content under the new version heading. Reset `[Unreleased]` to point at the next milestone.

#### 4c. Language rules

- Lead with what users can do, not how it was implemented
- Short sentences, bullet points
- Avoid jargon — "store and retrieve facts" not "triple CRUD via VP tables"
- Do not use emoji

### 5. Commit the changelog

```bash
git add CHANGELOG.md
git commit -m "docs: finalize changelog for vX.Y.Z"
git push origin main
```

### 6. Wait for CI

Verify CI passes on the pushed commit before tagging:

```bash
gh run list --limit 3
```

### 7. Do NOT tag

**Git tags are created manually by the maintainer.** The release skill must never run `git tag`. After completing steps 1–6, inform the user that the release is ready to tag with:

```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z — <version name from ROADMAP>"
git push origin vX.Y.Z
```

### 8. GitHub release is created automatically

Pushing the tag triggers `.github/workflows/release.yml`, which:

1. Runs the full test + regress suite on the tagged commit
2. Extracts the changelog entry for the version from `CHANGELOG.md`
3. Creates the GitHub release with that text as the body

**No manual `gh release create` step is needed.** Monitor the workflow run:

```bash
gh run list --limit 5
gh run view <run-id>
```

## Common Pitfalls

- **Do not create git tags** — tagging is always a manual step by the maintainer
- **Do not use shell heredocs for release notes** — they corrupt Unicode; always use `create_file`
- **Verify CI before tagging** — a tag on a broken commit is hard to undo
- **Check both Cargo.toml and pg_ripple.control** — version mismatches cause confusing install failures
- **pg_regress needs `--postgresql-conf "allow_system_table_mods=on"`** — the `pg_ripple` schema has a `pg_` prefix that PostgreSQL restricts by default
- **Reset [Unreleased] after release** — otherwise the next version's changes have nowhere to go
