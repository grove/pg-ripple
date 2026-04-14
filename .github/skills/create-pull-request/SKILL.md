---
name: create-pull-request
description: 'Create a GitHub Pull Request for pg_ripple. Use when: opening a PR, submitting code for review, creating a draft PR, publishing a branch, proposing changes. Handles branch creation policy, PR description writing, Unicode-safe body file, and CODEOWNERS review requirements.'
argument-hint: 'Optionally specify a title, base branch, or whether to create as a draft'
---

# Create a pg_ripple Pull Request

## Authoritative Sources

Always read these before creating a PR:

- [AGENTS.md](../../../AGENTS.md) — branch policy, PR body-file workflow, commit message style
- [RELEASE.md](../../../RELEASE.md) — when a PR is part of a release

## Branch Policy

From AGENTS.md: **never create a new branch unless the current branch is `main`.**

- If on `main` with unpushed commits, create a feature branch first:
  ```bash
  git checkout -b <branch-name>
  git push -u origin <branch-name>
  ```
- If already on a feature branch, proceed directly.
- Branch names: lowercase, hyphen-separated, prefixed by type — e.g. `feat/sparql-basic`, `fix/dictionary-encode`, `docs/changelog`.

## Procedure

### 1. Verify the branch is pushed

```bash
git status
git log --oneline origin/main..HEAD   # commits ahead of main
```

If there are uncommitted changes, ask the user whether to commit them first.

Push if needed:
```bash
git push -u origin <branch-name>
```

### 2. Gather commit context

```bash
git log --oneline origin/main..HEAD
git diff --stat origin/main..HEAD
```

Use this to write the title and body — don't summarise from memory.

### 3. Write the PR description to a file

**Always use the `create_file` tool — never a shell heredoc or `echo`.** Shell heredocs silently corrupt Unicode and can pick up stale content.

```bash
rm -f /tmp/pr_<slug>.md
# use create_file tool to write /tmp/pr_<slug>.md
```

PR body structure:

```markdown
## Summary

One paragraph: what this PR does and why.

## Changes

- Bullet list of meaningful changes (not a commit log)

## Testing

- How the changes were tested
- Commands run, test results

## Notes

Any reviewer guidance, known limitations, or follow-up work.
```

### 4. Validate the file

```bash
python3 -c "
with open('/tmp/pr_<slug>.md') as f:
    body = f.read()
print('lines:', body.count(chr(10)))
print('ok:', '####' not in body)
print(body[:120])
"
```

### 5. Create the PR

```bash
gh pr create \
  --title "<imperative-mood title under 72 chars>" \
  --body-file /tmp/pr_<slug>.md \
  --base main
```

For a draft PR (work in progress):
```bash
gh pr create --draft --title "..." --body-file /tmp/pr_<slug>.md --base main
```

### 6. Verify the live PR body

```bash
gh pr view <number> --json body --jq '.body' | head -20
```

If the body is garbled, fix it:
```bash
gh pr edit <number> --body-file /tmp/pr_<slug>.md
```

### 7. Report the result

Give the user the PR number and URL:
```
PR #<N>: https://github.com/grove/pg-ripple/pull/<N>
```

## Title Style

- Imperative mood: "Add SPARQL basic engine" not "Added" or "Adding"
- Under 72 characters
- Prefix with type if helpful: `feat:`, `fix:`, `docs:`, `ci:`, `refactor:`
- Describes *what* the PR does, not *how*

## CODEOWNERS Review Requirements

Every PR targeting `main` requires review from a code owner (see [CODEOWNERS](../../../.github/CODEOWNERS)).

- `@grove/pg-ripple-maintainers` must approve all changes
- Rust source changes (`src/`) additionally require `@grove/pg-ripple-rust`
- CI/workflow changes (`.github/`) additionally require `@grove/pg-ripple-infra`

## Common Pitfalls

- **Never use `echo` or heredoc for PR bodies** — use the `create_file` tool only
- **Always delete the stale temp file first** with `rm -f` before writing a new one
- **Do not create branches from non-main** — branch from `main` only (per AGENTS.md)
- **Check for unpushed commits** before calling `gh pr create` — the remote must have the branch
- **Draft PRs need to be marked ready** — remind the user if creating a draft
