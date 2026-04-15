---
name: implement-version
description: 'Implement a specific pg_ripple roadmap version. Use when: implementing a milestone like v0.2.0, v0.3.0; delivering roadmap features; building SPARQL engine, SHACL, Datalog, HTAP, bulk loading, federation. Covers Rust/pgrx 0.17, PostgreSQL 18, VP storage, dictionary encoding, SPARQL translation.'
argument-hint: 'Specify the version to implement, e.g., "v0.3.0" or "SPARQL Basic"'
---

# Implement pg_ripple Roadmap Version

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

### 5. Verify exit criteria

Before closing a version, check every exit criterion in ROADMAP.md explicitly. Do not mark a version done on partial evidence.

### 6. Write documentation

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
- [ ] Commit staged (do not run `git commit` without user approval)
```
