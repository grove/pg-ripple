# pg_ripple — Overall Assessment Prompt Template
# PROMPT-01 (v0.84.0): This file anchors automated assessments to the latest
# tagged release, preventing prompt-vs-reality gaps like the one found in A13.

## Purpose

This file is used as the system prompt (or initial context) when running an
automated deep-analysis assessment of the pg_ripple codebase.  It is versioned
alongside the code so that the prompt tracks the actual state of the project.

**Assessment anchor rule**: Every assessment prompt MUST reference the latest
**tagged and released** version of pg_ripple, not the planned next version.
Before running an assessment, update `CURRENT_VERSION` below to match the
latest `git tag --sort=-version:refname | head -1`.

---

## Assessment Variables (update before each run)

```
CURRENT_VERSION     = v0.84.0           # ← latest released git tag
CURRENT_EXT_VERSION = 0.84.0            # ← pg_ripple.control default_version
CURRENT_HTTP_VER    = 0.84.0            # ← pg_ripple_http/Cargo.toml version
PREVIOUS_ASSESSMENT = Assessment #13    # ← most recent completed assessment
PREVIOUS_SNAPSHOT   = 142d8f21a2bd1b30 # ← commit SHA used in previous run
ASSESSMENT_NUMBER   = 14                # ← increment for each new assessment
```

---

## Standard Assessment Prompt

Copy the block below (with variables substituted) as the opening message when
invoking a GitHub Copilot deep-analysis session:

---

**pg_ripple Overall Assessment #{ASSESSMENT_NUMBER}**

You are conducting a deep, multi-area technical quality assessment of
**pg_ripple v{CURRENT_EXT_VERSION}** (extension) and **pg_ripple_http
v{CURRENT_HTTP_VER}** (HTTP companion).  The assessment anchors at the
`v{CURRENT_EXT_VERSION}` git tag — do not evaluate planned or unreleased
features; if a feature appears only in a roadmap document but has no
implementing code, flag it as a documentation gap rather than evaluating it
as live functionality.

**Context from {PREVIOUS_ASSESSMENT}** (please verify rather than assume):
- The previous assessment snapshot was `{PREVIOUS_SNAPSHOT}`.
- All A13 findings that were marked RESOLVED should be re-verified by reading
  the actual source, not by trusting the previous assessment's conclusions.
- Open A13 findings and their status are listed in
  `plans/PLAN_OVERALL_ASSESSMENT_13.md`.

**Scope of this assessment ({ASSESSMENT_NUMBER} areas)**:

1. Correctness — SPARQL semantics, edge cases, regression coverage
2. Security — OWASP Top 10, SECURITY DEFINER, SQL injection, auth
3. Performance — query latency, plan cache, HTAP merge, WCOJ
4. Scalability — Citus sharding, parallel workers, VP promotion
5. Reliability — background workers, crash recovery, CDC
6. Observability — metrics, explain, tracing
7. Operability — migration continuity, upgrade path, Docker
8. API quality — SPARQL 1.1 compliance, HTTP protocol
9. Code quality — module size, unsafe usage, dead code
10. Test coverage — pg_regress, proptest, fuzz, W3C conformance
11. Documentation — README, docs/, blog/, CHANGELOG accuracy
12. Dependency health — audit.toml, deny.toml, CVE exposure
13. HTTP companion — version sync, COMPAT-01, /health endpoints
14. Datalog & SHACL — inference correctness, SHACL shapes
15. Federation — circuit breaker, SSRF, result decoder
16. Process — roadmap accuracy, assessment tooling, PROMPT-01

For each finding, state:
- **ID**: A{ASSESSMENT_NUMBER}-{Area}-{Seq} (e.g. A14-S-01)
- **Severity**: Critical | High | Medium | Low
- **File + line** where the issue is or where the fix should go
- **Recommended action** (specific, actionable)

At the end, provide:
- A **World-Class Quality Score** (0.0–5.0) with justification
- A **Top 5 Critical Actions** list for the pre-v1.0.0 milestone
- An updated `plans/PLAN_OVERALL_ASSESSMENT_{ASSESSMENT_NUMBER}.md` draft

---

## Process Rules

1. **Always anchor at the latest tagged release.**
   Run `git tag --sort=-version:refname | head -1` to confirm the current tag
   before starting.  Never run an assessment against an unreleased branch unless
   the user explicitly requests a pre-release review.

2. **Re-verify previous findings.**
   Do not copy-paste previous resolution statuses.  Read the source code to
   confirm each claimed fix is present and correct.

3. **Flag prompt-vs-reality gaps immediately.**
   If the roadmap, AGENTS.md, or any plan document claims a feature at the
   current version that is not present in source, create a finding with
   severity HIGH and ID A{N}-PR-01 (prompt-reality gap).

4. **Do not over-count.**
   Minor style issues (trailing whitespace, comment typos) are Low severity.
   Only escalate to High/Critical for defects with real-world impact.

5. **Save output as `plans/PLAN_OVERALL_ASSESSMENT_{ASSESSMENT_NUMBER}.md`.**
   Commit the file to the repo immediately after the session so it is
   part of the version history.

---

## Change Log

| Date       | Version | Change                                    |
|------------|---------|-------------------------------------------|
| 2026-05-07 | v0.84.0 | PROMPT-01: initial template created       |
