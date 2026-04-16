---
name: implement-version
description: 'Implement a specific pg_ripple roadmap version. Use when: implementing a milestone like v0.2.0, v0.3.0; delivering roadmap features; building SPARQL engine, SHACL, Datalog, HTAP, bulk loading, federation. Covers Rust/pgrx 0.17, PostgreSQL 18, VP storage, dictionary encoding, SPARQL translation.'
argument-hint: 'Specify the version to implement, e.g., "v0.3.0" or "SPARQL Basic"'
---

# Implement pg_ripple Roadmap Version

## Autonomous Execution Contract

This skill runs **end-to-end without pausing for approval** unless a genuine decision blocker is hit (see "When to pause" below). The agent:

- Commits and pushes code without asking first
- Runs fmt/clippy/test and self-heals failures before each commit
- Monitors CI after each push and loads the `fix-ci` skill to resolve failures autonomously
- Only stops when CI is green and all exit criteria are met

**When to pause (genuine blockers only):**
- An architectural trade-off with no clear answer in ROADMAP.md or implementation_plan.md
- A failing test that is caused by an ambiguity in the spec (not a bug)
- A destructive migration (DROP TABLE, breaking API change) that was not in the spec

Everything else — compiler errors, clippy warnings, test failures, CI failures — is resolved autonomously. Do not ask for permission to fix these.

---

## Authoritative Sources

Always read these before writing any code:

- [ROADMAP.md](../../../ROADMAP.md) — deliverables, exit criteria, test file names, effort estimates, version prerequisites
- [plans/implementation_plan.md](../../../plans/implementation_plan.md) — schemas, API signatures, algorithms, crate choices, GUC parameters, §14 documentation conventions
- [plans/documentation.md](../../../plans/documentation.md) — docs site structure, tooling, and the full milestone-by-milestone list of pages to create or update
- [AGENTS.md](../../../AGENTS.md) — code conventions, build/test commands, git workflow

## Procedure

### 1. Read the version section in ROADMAP.md

Locate the target version. Read its full section — deliverables checklist, plain-language explanation, notes, and exit criteria.

### 2. Cross-reference implementation_plan.md

For each deliverable, look up the corresponding section in the implementation plan for exact schemas, function signatures, and algorithm details. The plan is authoritative when ROADMAP.md and the plan disagree.

### 3. Audit existing code

```bash
ls -la src/
grep -rn "pg_extern" src/ --include="*.rs"
cargo pgrx test pg18 2>&1 | tail -20
```

Understand what already exists before adding anything.

### 4. Implement deliverables in order

Items in the ROADMAP.md checklist are listed in dependency order — implement them top to bottom. For each deliverable:

1. Write the Rust implementation
2. Add SQL to `sql/` if needed
3. Write `#[pg_test]` integration tests
4. Write the pg_regress `.sql` file
5. **Tick the checkbox in ROADMAP.md** — change `- [ ]` to `- [x]` for that deliverable immediately after it is implemented and tested; do not batch this at the end

### 5. Self-healing pre-commit loop

**Run this before every `git commit` and fix all failures before committing:**

```bash
# Step A: format (auto-fix)
cargo fmt --all

# Step B: lint (auto-fix then verify)
cargo clippy --fix --allow-dirty --features pg18
cargo clippy --features pg18 -- -D warnings        # must be zero warnings

# Step C: unit + integration tests
cargo pgrx test pg18                               # must be zero failures

# Step D: pg_regress
cargo pgrx regress pg18 --postgresql-conf "allow_system_table_mods=on"
```

If Step B emits warnings after `--fix`, fix them manually — common patterns:
- `#![allow(dead_code)]` for WIP modules not yet called from `pg_extern`
- `std::slice::from_ref(x)` instead of `&[x.clone()]`
- Let-chains (`if let ... && condition`) instead of nested if-let

If Step C or D fails, fix the root cause before committing — do **not** suppress test failures with `#[ignore]` or `should_panic` unless that is the correct semantic.

### 6. Commit and push discipline

Group related changes into logical commits. Commit message style: lowercase first word, imperative mood, no trailing period. Push immediately after each commit:

```bash
git add <files>
git commit -m "feat: <description>"
git push origin main
```

### 7. Monitor CI and self-heal

After pushing, check CI status with:

```bash
gh run list --limit 3
```

If CI fails, **immediately load the `fix-ci` skill** and resolve the failure autonomously. Do not pause and ask the user. Common CI-specific failures not caught locally:

- Linux linker errors (GNU ld vs. lld flag differences)
- Missing apt-get dependencies in the CI runner
- pg_regress expected output mismatches due to platform differences

When CI is green on the pushed commit, continue to the next deliverable.

### 8. Verify exit criteria

Before closing a version, check every exit criterion in ROADMAP.md explicitly. Do not mark a version done on partial evidence.

### 9. Write documentation

Every ROADMAP.md version section contains a `### Documentation` subsection. Treat those checkboxes exactly like code deliverables.

1. Read the `### Documentation` subsection for the target version in ROADMAP.md.
2. Cross-reference [plans/documentation.md](../../../plans/documentation.md) for the full page specification (content, structure, examples required).
3. Create or expand each listed `docs/src/` page.
4. Verify the page is wired into `docs/src/SUMMARY.md`.
5. Run `mdbook build docs` locally to confirm the site builds without errors.
6. **Tick each documentation checkbox in ROADMAP.md** — same discipline as code checkboxes.

> **v0.5.0 special case**: v0.5.0 carries the full catch-up backlog for v0.1.0–v0.4.0 (site scaffold, all released SQL API pages, playground, best practices, FAQ) in addition to its own pages. The site must be live on GitHub Pages before v0.5.0 is tagged.

## Common Pitfalls

These are the mistakes most likely to produce silent bugs:

- **String comparisons in VP tables are a bug** — always encode to `i64` first; the integer-join invariant is load-bearing
- **Encode FILTER constants at translation time** — never at execution time
- **Batch decode query results** — collect all output IDs, decode with `WHERE id = ANY(...)`, then emit rows; never decode per-row
- **Document-scope blank nodes** — use `load_generation` prefix; `_:b0` from two different loads must get different IDs
- **ANALYZE after bulk loads** — planner statistics must be current for generated SQL join plans to be correct
- **Table names via OID lookup** — look up `table_oid` from `_pg_ripple.predicates`; never concatenate raw predicate IDs into SQL strings
- **CYCLE clause for property paths** — use PG18's `CYCLE` clause, not array-based visited tracking

## Implementation Checklist Template

```markdown
## vX.Y.Z Implementation Checklist

### Prerequisites
- [ ] All prior version tests pass: `cargo pgrx test pg18`
- [ ] Any blocking prerequisites resolved (check ROADMAP.md version section)
- [ ] New crate dependencies pinned in Cargo.toml

### Deliverables
(copy checklist items verbatim from ROADMAP.md, add test item for each)

### Testing
- [ ] Unit tests pass: `cargo test`
- [ ] Integration tests pass: `cargo pgrx test pg18`
- [ ] pg_regress suite passes: `cargo pgrx regress pg18`
- [ ] Adversarial inputs tested: SQL metacharacters, malformed RDF, Unicode edge cases
- [ ] Concurrent operations tested where applicable

### Exit Criteria
(copy exit criteria verbatim from ROADMAP.md, check each explicitly)

### Pre-Release
- [ ] **Extension migration script created** — **CRITICAL** (see [Extension Versioning & Migration Scripts](../../../AGENTS.md#extension-versioning--migration-scripts))
  - File: `sql/pg_ripple--X.(Y-1).Z--X.Y.Z.sql`
  - Include schema changes (ALTER TABLE, CREATE INDEX, etc.) if any exist
  - Otherwise, write a comment header explaining what functionality is new
  - Without this file, users on earlier versions cannot upgrade via `ALTER EXTENSION ... UPDATE`
- [ ] **Documentation deliverables complete** (see `### Documentation` in ROADMAP.md for this version)
  - All listed `docs/src/` pages created or expanded
  - All documentation checkboxes in ROADMAP.md ticked (`- [x]`)
  - `docs/src/SUMMARY.md` updated to include any new pages
  - `mdbook build docs` passes without errors
- [ ] Verify `Cargo.toml` version field matches X.Y.Z
- [ ] Verify `pg_ripple.control` `default_version` matches X.Y.Z

### Git
- [ ] All ROADMAP.md deliverable checkboxes for this version are ticked (`- [x]`)
- [ ] CHANGELOG.md updated
- [ ] All commits pushed to `origin/main`
- [ ] CI is green on the latest pushed commit (`gh run list --limit 1`)

## Completion Report

When the implementation checklist is complete and all tests pass, generate a completion report that includes:

### Report Structure

1. **Version Summary**
   - Version number and delivery name
   - List of completed deliverables (major features only, not every checkbox)
   - Lines of code added (via `git diff`)

2. **Remaining Work**
   - **Blockedby next version** — List any deliverables in the next ROADMAP.md version that depend on this version, to clarify the critical path
   - **Deferred items** — Any items from the original ROADMAP.md spec that were descoped or moved to a future version, with brief justification
   - **Outstanding PRs or issues** — Any related GitHub issues or PRs that are still open and non-blocking
   - **Technical debt** — Any known limitations, TODOs in code, or performance concerns that should be addressed in the next version

3. **Next Steps for the Next Version**
   - Prerequisites to unblock (e.g., "SPARQL optimizer should be moved to v0.5 to allow v0.6 property-paths to land")
   - Suggested focus areas for the next milestone
   - Links to relevant ROADMAP.md section and implementation_plan.md sections

### How to Generate the Report

1. **Completed deliverables**: Extract from ROADMAP.md for this version; count `[x]` checkboxes in code and documentation sections.
2. **Next version blockers**: Read ROADMAP.md section for next version; identify deliverables with explicit prerequisites (e.g., "requires v0.4 storage layer").
3. **Deferred items**: Search ROADMAP.md and git commit history for items marked "descoped for vX.Y.Z" or similar; include reason.
4. **Outstanding items**: Run `gh issue list -R grove/pg-ripple --label "defer" --label "backlog"` and `gh pr list -R grove/pg-ripple --draft` to find open items.
5. **Code TODOs**: Run `grep -rn "TODO\|FIXME\|XXX" src/ --include="*.rs" | head -20` to surface code-level concerns.
6. **Git stats**: Run `git log vX.(Y-1).Z..HEAD --oneline | wc -l` and `git diff vX.(Y-1).Z HEAD --stat --summary | tail -3` for diff stats.

### Example Report

```
## Completion Report — v0.4.0 (SPARQL Intermediate)

### Version Summary
Delivered:
- ✓ FILTER algebra and SQL codegen (12+ data types, 20+ SPARQL functions)
- ✓ UNION, OPTIONAL, property paths, aggregates, ORDER BY, LIMIT
- ✓ Query plan caching via LRU (25K plan limit)
- ✓ Full regression test suite (28 new tests)

Commits: 34 | Files: 16 | +3,847 −1,292 lines

### Remaining Work

**Blocked by v0.4.0 (Prerequisites for v0.5.0)**
- Discovery of blank-node scope in SPARQL (needed for CONSTRUCT)
- Bulk-load triples (load_generation prefix for blank-node scoping)
- RDF-star support in dictionary (qt_s, qt_p, qt_o columns)

**Deferred to v0.6.0+**
- SPARQL CONSTRUCT (requires blank-node scoping from v0.5.0)
- Graph metrics (EXPLAIN for VP table cardinality)
- Datalog ← Pushed to v0.7.0 to sequence SHACL first

**Outstanding & Non-Blocking**
- PR #45: Optimize filter constants (draft, can land anytime)
- Issue #102: Case-insensitive string comparison (backlog)

**Code-Level Concerns**
- TODO in sqlgen.rs line 421: "Optimize self-join elimination for star patterns"
- XXX in property_path.rs: "CYCLE clause cycles detection may panic on large graphs; add timeout"

### Next Up: v0.5.0 (SPARQL Advanced)
**Must unblock first:**
- Blank-node scoping design doc (RFC in plans/)
- Decision: load_generation vs. document-local blank nodes

**Success metrics:**
- 100% SPARQL 1.1 query feature coverage (minus FEDERATED SPARQL)
- Bulk load 1M+ triples in <30 seconds
- All 8 SHACL shapes in test suite validate correctly

**See also:**
- [ROADMAP.md § v0.5.0](../ROADMAP.md#v050-sparql-advanced-storage-serialization--write)
- [plans/implementation_plan.md § 4.4 Blank Nodes](../plans/implementation_plan.md#44-blank-node-scoping)
```

**Generate this report at version completion as a summary message to the user.** It provides visibility into progress, highlights what depends on this version, and frames the next milestone clearly.
```
