---
name: release
description: 'Release a pg_ripple version. Use when: tagging a release, preparing release notes, running the release checklist, creating a GitHub release. Covers pre-release verification, changelog finalization, GitHub release creation.'
argument-hint: 'Specify the version to release, e.g., "v0.1.0" or "v0.3.0"'
---

# Release pg_ripple Version

## Autonomous Execution Contract

This skill runs **end-to-end without pausing for approval** once invoked. The agent:

- Runs all verification checks and self-heals failures
- Updates CHANGELOG.md, commits, and pushes
- Waits for CI and resolves any failures via the `fix-ci` skill
- Presents the final `git tag` command for the user to run

**The only step the agent does NOT perform is `git tag`.** Tagging is a manual, irreversible act by the maintainer. Everything leading up to it is automated.

**When to pause (genuine blockers only):**
- A pre-release check fails that cannot be fixed without a new implementation commit (e.g. a failing test exposing a real bug)
- The CHANGELOG entry requires information only the maintainer knows (external user reports, marketing context)

---

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

If any check fails, **fix it autonomously** using the same self-healing loop as in the implement-version skill. Do not stop and ask the user — these are mechanical failures. Load the `fix-ci` skill if CI-specific patterns appear.

All four must pass with zero warnings and zero failures before proceeding.

### 2. Verify version numbers match

```bash
grep '^version' Cargo.toml
grep 'default_version' pg_ripple.control
```

Both must show the target version (e.g. `0.2.0`). If they don't match, fix the discrepancy and commit before continuing.

### 3. Review the ROADMAP exit criteria

Open the target version section in ROADMAP.md and verify each exit criterion explicitly. List them with pass/fail in the final summary.

### 4. Update CHANGELOG.md

#### 4a. Gather changes from git

```bash
# If there's a previous tag:
git log --oneline v0.X.(Y-1)..HEAD

# If no previous tag exists:
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

### 5. Verify migration script exists

```bash
ls sql/pg_ripple--*.sql | sort
```

There must be a `sql/pg_ripple--X.(Y-1).Z--X.Y.Z.sql` file. If it is missing, create it now (see AGENTS.md § Extension Versioning). This is a hard blocker — PostgreSQL cannot upgrade without it.

### 6. Commit all release prep in one commit

```bash
git add CHANGELOG.md
git commit -m "docs: finalize release notes for vX.Y.Z"
git push origin main
```

If other files were touched (migration script, version bumps), include them in the same commit.

### 7. Wait for CI and resolve any failures

```bash
gh run list --limit 3
```

Poll until the run for the HEAD commit has a final status. If it fails, **immediately load the `fix-ci` skill** and resolve the failure. Push the fix and wait again. Repeat until CI is green.

Do not proceed to step 8 while CI is still running or red.

### 8. Present the tag command

When CI is green, output the exact commands for the maintainer to run:

```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z — <version name from ROADMAP>"
git push origin vX.Y.Z
```

Explain that pushing the tag triggers `.github/workflows/release.yml`, which runs the full test suite on the tagged commit and creates the GitHub release automatically using the CHANGELOG entry.

## Common Pitfalls

- **Do not create git tags** — tagging is always a manual step by the maintainer
- **Do not use shell heredocs for release notes** — they corrupt Unicode; always use `create_file`
- **Verify CI before informing the user to tag** — a tag on a broken commit is hard to undo
- **Check both Cargo.toml and pg_ripple.control** — version mismatches cause confusing install failures
- **pg_regress needs `--postgresql-conf "allow_system_table_mods=on"`** — the `pg_ripple` schema has a `pg_` prefix that PostgreSQL restricts by default
- **Reset [Unreleased] after release** — otherwise the next version's changes have nowhere to go
- **Migration script is a hard blocker** — never release without `sql/pg_ripple--prev--next.sql` in the repo
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
