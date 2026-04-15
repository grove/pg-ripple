# Cypher/GQL → SPARQL Transpilation: Reassessment

> **Status**: Decision analysis — April 2026.
> Follows from practical experience building the `cypher-algebra` → SPARQL algebra transpiler
> and running it against the openCypher TCK.

---

## 1. Context

The original plan ([cypher_lpg_analysis.md](cypher_lpg_analysis.md), [query_language_landscape.md](query_language_landscape.md)) concluded that a static Cypher → SPARQL algebra transpiler was the correct architecture, reusing the existing `src/sparql/` SQL emission path. Work on this transpiler has now progressed far enough to evaluate that conclusion against reality.

**Summary of original thesis**: openCypher/GQL read-path maps cleanly to SPARQL algebra; a `cypher-algebra` crate produces an IR closely related to spargebra; pg_ripple's SQL emission path is reused at zero extra cost.

**What we found**: The mapping is broadly correct for the happy path, but the transpiler has been _extremely_ difficult to build to TCK-passing quality. Two TCK scenarios are provably impossible without abandoning the static transpilation model entirely.

---

## 2. Difficulty Encountered

### 2.1 Semantic mismatches that compound

The original analysis correctly identified several mismatches (variable scoping, null semantics, list operations, path objects). What it underestimated was the _compounding_ effect of these mismatches when they interact. Each mismatch is individually tractable; together they produce a combinatorial explosion of edge cases:

| Mismatch | Isolated difficulty | Combined difficulty |
|---|---|---|
| `WITH` scope reset vs. SPARQL subquery scoping | Medium | High — interacts with `OPTIONAL MATCH` null propagation |
| Cypher null propagation vs. SPARQL UNDEF | Medium | High — interacts with `WHERE` filters, aggregation, `CASE` |
| List literals / comprehensions | Medium | High — interacts with `UNWIND`, `WITH`, variadic path returns |
| Path objects as first-class values | High | Very high — interacts with shortest-path, variable-length paths, aggregation over paths |

Each of these produces a long tail of corner cases that the TCK explicitly tests. Fixing one case breaks another because the underlying semantic models diverge in assumptions about bound vs. unbound values, bag vs. set semantics, and identity vs. value equality.

### 2.2 Developer time spent

The transpiler effort has significantly exceeded the original estimate for the read-path alone. The algebra mapping is mechanically straightforward; the semantic patching to handle the mismatches is where the time goes. Each TCK failure requires:

1. Diagnosis of which semantic mismatch is at play
2. Design of a compensating transformation in the algebra IR
3. Verification that the compensation does not regress other passing tests

This is a fundamentally different engineering profile from the SPARQL → SQL pipeline, where the source algebra and target semantics are designed to align.

---

## 3. Fundamental Limitations — Provably Impossible Scenarios

Two remaining TCK failures are not engineering difficulty but **fundamental incompatibilities** between the LPG model and the RDF model. No amount of transpiler sophistication can resolve them within a static Cypher → SPARQL → SQL pipeline.

### 3.1 Runtime list as path constraint — Match4[8]

**TCK scenario**: `MATCH (a)-[rs*]->(b)` where `rs` is a runtime-computed list used to constrain which relationship types are traversed.

**Why it fails**: SPARQL property paths are compiled statically at query parse time. The set of predicates in a path expression must be known before execution begins — they become fixed VP table references in the generated SQL. A runtime list (`rs` bound during execution, possibly from a `WITH` clause or parameter) requires:

1. Execute a first phase to resolve `rs` to a concrete list of relationship types
2. Dynamically construct a property path expression from that list
3. Execute a second phase using the dynamically constructed path

This is a **multi-phase execution model** that a static transpiler cannot produce. The SPARQL algebra has no operator for "resolve this variable, then use its value as a path constructor." The SQL emission path would need to generate a prepared statement template, execute it to resolve `rs`, then generate and execute a second query — fundamentally breaking the single-pass algebra → SQL model.

**Severity**: This pattern is rare in practice (most path constraints use literal type names), but it is a legitimate Cypher feature and the TCK tests it explicitly.

### 3.2 Undirected paths with parallel edges on multigraphs — Match6[14]

**TCK scenario**: Undirected variable-length path `*3..3` where the graph contains parallel edges (multiple edges of the same type between the same pair of nodes).

**Why it fails**: RDF is a **set-based** model at the triple level. The triple `(s, p, o)` either exists or it does not — there is no concept of two distinct edges with the same subject, predicate, and object. This is a foundational RDF design choice:

- LPG: edges have identity. `(alice)-[:KNOWS]->(bob)` can appear twice, and each instance is a distinct edge with its own ID.
- RDF: `(:alice :knows :bob)` is a single fact. Inserting it twice is idempotent. VP tables enforce this via `(s, o)` uniqueness within a predicate table.

When the TCK scenario traverses an undirected path of exactly length 3 through parallel edges, LPG semantics count each parallel edge as a distinct traversal step. RDF semantics collapse the parallel edges into one triple, making the path impossible to traverse at the required length.

**This is not a storage limitation** — pg_ripple _could_ store duplicate triples by adding a synthetic edge ID column and removing the uniqueness constraint. But doing so would violate the RDF data model that the entire system is built on: the dictionary encoder, the SPARQL engine, the SHACL validator, and the Datalog reasoner all assume set semantics. Changing this assumption would be a fundamental architectural rework, not a bug fix.

**Severity**: Multigraph support is important in some LPG use cases (e.g., temporal graphs where the same relationship exists at multiple time points, modeled as parallel edges rather than edge properties). However, the standard RDF modeling approach handles this via named graphs or reification/RDF-star, both of which pg_ripple supports.

---

## 4. Options

### Option A: Continue with transpilation (current approach)

**Approach**: Ship the transpiler with documented limitations; accept that 2 TCK scenarios will never pass; target ≥98% TCK compliance.

| Pros | Cons |
|---|---|
| Reuses entire SPARQL → SQL pipeline | High ongoing maintenance burden for semantic patching |
| No new execution engine | Each new Cypher/GQL standard revision multiplies edge cases |
| Single algebra IR for both languages | Two provably failing TCK scenarios — visible in compliance claims |
| Already partially built | Developer effort has far exceeded estimates |
| | Subtle correctness bugs from semantic mismatches are hard to detect — may surface in production as wrong query results |

**TCK compliance ceiling**: ~98% (2 fundamental failures out of ~100+ scenarios).

### Option B: Native Cypher execution engine (bypass SPARQL entirely)

**Approach**: Build a separate `src/cypher/` execution pipeline that compiles Cypher algebra directly to VP-table SQL, without going through SPARQL algebra as an intermediate representation. Shares storage (VP tables) and infrastructure (dictionary, HTAP) but has its own algebra → SQL compiler.

| Pros | Cons |
|---|---|
| No semantic mismatch — Cypher semantics implemented directly | Duplicates significant SQL generation infrastructure |
| Can implement multi-phase execution for Match4[8] | Still cannot solve Match6[14] (RDF set semantics) |
| Cleaner long-term maintenance | Estimated 50–70 person-weeks (nearly double the transpiler) |
| Can adopt GQL standard changes without SPARQL compatibility layer | Two execution engines to maintain and optimize |
| Higher TCK ceiling (~99%) | Optimization work (join reordering, filter pushdown) must be done twice |

### Option C: Drop Cypher/GQL from the project scope

**Approach**: Remove Cypher/GQL from the roadmap entirely. pg_ripple is an RDF/SPARQL system; LPG users should use purpose-built LPG databases (Neo4j, Kuzu, Memgraph) or PostgreSQL's future native SQL/PGQ support.

| Pros | Cons |
|---|---|
| Zero additional complexity | Loses a differentiator vs. pure RDF stores |
| Focus engineering effort on SPARQL excellence | Users who need both RDF and LPG must use two systems |
| No TCK compliance claims to defend | Misses the OneGraph convergence trend |
| Clear product positioning | Counter to the prior art findings (Stardog, AnzoGraph: unified store wins) |

### Option D: Defer to SQL/PGQ (PostgreSQL-native path)

**Approach**: Do not implement a Cypher parser at all. Instead, provide a well-documented VP-table view layer that PostgreSQL's future `GRAPH_TABLE()` / SQL/PGQ implementation can query directly. pg_ripple becomes the _storage engine_ for PGQ, not the query engine.

| Pros | Cons |
|---|---|
| Zero parser/transpiler maintenance | SQL/PGQ timeline is uncertain (PG18 does not have it) |
| Leverages PostgreSQL's own optimizer for graph patterns | Users get no LPG access until PostgreSQL ships PGQ |
| Standard SQL — no new query surface to document/support | pg_ripple has no control over PGQ's feature set or performance |
| Strongest long-term bet if PGQ succeeds | If PGQ slips or is limited, users have nothing |

### Option E: Limited Cypher support — read-only, no paths (pragmatic subset)

**Approach**: Support only the Cypher subset that maps cleanly to SPARQL with no semantic patching: `MATCH` patterns (no variable-length paths), `WHERE`, `RETURN`, `ORDER BY`, `SKIP`, `LIMIT`, `WITH`, `OPTIONAL MATCH`, aggregation. Explicitly exclude variable-length paths, `shortestPath`, list comprehensions, and all write operations. Document this as "Cypher-compatible pattern matching" rather than "Cypher support."

| Pros | Cons |
|---|---|
| Leverages existing transpiler work | "Cypher" without paths is arguably not Cypher |
| No semantic patching needed for this subset | TCK compliance claims are awkward for a subset |
| Low maintenance burden | Users expecting full Cypher will be disappointed |
| Honest product positioning | Still need write path separately if ever added later |
| Ships quickly from current state | |

---

## 5. Recommendation

**Option E (limited Cypher, read-only)** as the pragmatic near-term choice, with **Option D (SQL/PGQ)** as the long-term strategic direction.

### Rationale

1. **The transpiler difficulty is a signal, not a bug.** The semantic gap between Cypher and SPARQL is real and irreducible. Fighting it produces an ever-growing pile of compensating transformations that are hard to test and easy to regress. The prior art validates this: Stardog deprecated their Cypher endpoint in v10 after years of maintenance burden.

2. **The two fundamental failures are unresolvable in an RDF system.** Match6[14] (multigraph) is a data model incompatibility, not an engineering problem. No architecture change within pg_ripple can fix it without abandoning RDF set semantics. Match4[8] (runtime path constraints) requires multi-phase execution that a static algebra pipeline cannot express. Even Option B (native engine) can only solve one of the two.

3. **A limited, honest subset is more useful than a fragile full transpiler.** Users who write `MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.age > 30 RETURN b.name` — which is the overwhelming majority of real-world Cypher queries — get correct results immediately. Users who need full Cypher/GQL semantics should use a native LPG database.

4. **SQL/PGQ is the right long-term answer.** PostgreSQL is moving toward native graph pattern matching via SQL:2023 Part 16. When this ships, pg_ripple's VP tables become a natural storage substrate. This avoids the entire transpilation problem: the query engine is PostgreSQL itself, using its own optimizer, with full LPG semantics.

5. **Engineering effort is better spent on SPARQL excellence.** The remaining v0.x roadmap (HTAP, CDC, full-text search, monitoring, Datalog, SHACL) delivers more value per person-week than pushing the transpiler to 98% TCK compliance.

### Proposed roadmap change

| Current | Proposed |
|---|---|
| v1.6: Full Cypher/GQL (openCypher TCK ≥80%) | v1.6: Cypher-compatible pattern matching (read-only subset, no variable-length paths) |
| | v1.6+: SQL/PGQ view layer (when PostgreSQL ships PGQ support) |

### What "limited Cypher" includes

- `MATCH (n:Label)`, `MATCH (a)-[:TYPE]->(b)`, `MATCH (a)-[:TYPE]-(b)` (undirected)
- `WHERE` with property filters, boolean logic, `IS NULL` / `IS NOT NULL`
- `OPTIONAL MATCH`
- `RETURN` with expressions, aliases, `DISTINCT`
- `WITH` for intermediate projection and aggregation
- `ORDER BY`, `SKIP`, `LIMIT`
- `UNION` / `UNION ALL`
- `count()`, `sum()`, `avg()`, `min()`, `max()`, `collect()`
- `CASE ... WHEN ... THEN ... END`

### What it explicitly excludes (documented as unsupported)

- Variable-length paths: `[*]`, `[*1..5]`, `[*..n]`
- `shortestPath()` / `allShortestPaths()`
- List comprehensions: `[x IN list WHERE ... | expr]`
- `UNWIND` (except as `LATERAL` over a `VALUES` clause — possible future addition)
- Write operations: `CREATE`, `MERGE`, `SET`, `REMOVE`, `DELETE`
- `CALL` procedures
- Runtime-computed path constraints
- Multigraph semantics (parallel edges between same node pair with same type)

---

## 6. Impact on Existing Plans

| Document | Required change |
|---|---|
| [cypher_lpg_analysis.md](cypher_lpg_analysis.md) | Add note pointing to this reassessment |
| [query_language_landscape.md](query_language_landscape.md) | Add note pointing to this reassessment |
| [prior_art_hybrid_systems.md](prior_art_hybrid_systems.md) | No change (validates this conclusion — Stardog deprecated Cypher) |
| ROADMAP.md | Update v1.6 row; add SQL/PGQ note |
| README.md | Update future directions section |

---

## 7. Open Questions

1. **Should the limited Cypher subset support `MATCH (a)-[:TYPE*1..1]->(b)`?** This is equivalent to a single hop and avoids the variable-length path machinery. It would be a trivial special case. Recommended: yes, support `*1..1` as syntactic sugar for a single-hop match.

2. **Should we expose `pg_ripple.cypher()` or `pg_ripple.match()`?** The function name sets user expectations. `cypher()` implies full Cypher; `match()` or `graph_match()` is more honest. Recommended: `pg_ripple.graph_match()`.

3. **How do we handle the transition if/when PostgreSQL ships SQL/PGQ?** Ideally, `pg_ripple.graph_match()` becomes a thin wrapper that delegates to native PGQ when available. The VP table schema is designed to be PGQ-compatible (nodes are subjects, edges are predicates). The transition should be seamless for users.

4. **Is there value in publishing the `cypher-algebra` crate even if we limit the supported subset?** Yes — the parser and algebra IR are useful to the wider Rust ecosystem regardless of pg_ripple's feature scope. The crate should be published with clear documentation of which algebra operators pg_ripple supports vs. which are parse-only.
