# Prior Art Analysis: QLever

> **License note**: Apache-2.0 — safe to reference directly, including code patterns.

---

## 1. QLever

**Repository**: https://github.com/ad-freiburg/qlever  
**License**: Apache-2.0  
**What it is**: A high-performance C++ SPARQL/RDF database developed at the University of Freiburg
(Hannah Bast et al.). Claims to support hundreds of billions of triples on a single commodity
machine — the largest published demo has over one trillion triples. Active development; most recent
release v0.5.45 (February 2026, commits from April 12, 2026). 814 stars.

**Notable deployments**: Wikidata (17B triples), UniProt (94B triples), OpenStreetMap (10B+
geometries), DBLP, PubChem. These are the exact benchmark datasets pg_ripple should target.

**Against Oxigraph on DBLP (390M triples)**:

| Metric | Oxigraph (Rust) | QLever (C++) |
|---|---|---|
| Load speed | 0.6 M triples/s | **1.7 M triples/s** |
| Index size | 67 GB | **8 GB** |
| Avg query time | 93s | **0.7s** |

QLever is ~3× faster to load, ~8× smaller on disk, and ~130× faster on average queries than
Oxigraph. Compared to all others in the benchmark (Virtuoso, Stardog, GraphDB, Jena, Blazegraph),
QLever wins every metric by a wide margin.

### 1.1 Relevance

QLever's architecture explains *why* it is so much faster — not from raw C++ vs. Rust speed, but
from a fundamentally different index design. Understanding that design is more valuable than any
single code pattern.

### 1.2 Lessons

#### 1.2.1 Pre-sorted flat permutation files: the core performance insight

QLever's index stores triples in six sorted flat files (permutations), one per sort order:

```
PSO  POS  SPO  SOP  OSP  OPS
```

Each permutation is a sorted list of (id, id, id) triples, compressed using run-length encoding on
the leading key. Because the permutations are pre-sorted, **every SPARQL join is a merge join**:
given two sorted sequences from two triple patterns that share a variable, a linear scan aligns
them in O(n+m) time with zero random I/O.

This is the architectural explanation for QLever's performance numbers:
- "All predicates ordered by size" (conceptually a full-table scan) takes **0.01s in QLever vs
  1.48s in Virtuoso and 106s in Oxigraph** — because the PSO permutation lists all triples grouped
  by P; counting subjects per predicate is a prefix scan over sorted data.
- "All papers with their title" (7M rows, simple join) takes **4.2s in QLever vs 44–132s for
  others** — because the join compiles to two merge joins over PSO permutations.

Compare this to pg_ripple: VP tables are already predicate-partitioned (equivalent to QLever's P
being fixed), and within each VP table the `(s, o, g)` B-tree gives O(log n) single-key lookups.
PostgreSQL's planner can sometimes choose a merge join when both sides are sorted, but this
requires that the index provides a sorted scan order that matches the join column — which is only
guaranteed in pg_ripple's case for the primary index key.

**Implication for `src/sparql/emitter.rs`**: When emitting SQL for a BGP join where the join
variable is the sort key of the VP table's primary index (i.e., subject-to-subject or
object-to-object joins within the same VP table), add an `ORDER BY` hint in the CTEs so the
PostgreSQL planner considers a merge join. The planner already does this when index conditions
are met, but explicit ordering in subqueries can help avoid sort operations for multi-join BGPs.

More broadly: QLever's performance advantage is structural — flat, sorted, compressed index files
vs. B-trees with random-access overhead. pg_ripple cannot match QLever's throughput on bulk
sequential scans (aggregate all triples of predicate P) because PostgreSQL B-trees carry per-node
overhead. This is a known architectural trade-off: pg_ripple pays per-triple overhead to get
transactional writes, concurrent access, and the full PostgreSQL ecosystem. The benchmark numbers
are the quantification of that trade-off.

**Document this trade-off explicitly** in `implementation_plan.md` with the QLever DBLP numbers
as the reference benchmark. pg_ripple's North Star for bulk-scan performance should be within 5×
of QLever (achievable through BRIN-indexed main partitions and parallel seq-scan SQL).

#### 1.2.2 Two-tier vocabulary: RAM for hot IRIs, disk for cold literals

QLever assigns 8-byte integer IDs to all IRIs and literals. The IDs are allocated in type-sorted
order — IRIs get one contiguous ID range in lexicographic order, integers another in numeric order,
dates another, and so on. This means:

1. **Range queries work on raw IDs**: `FILTER(?year > 1990^^xsd:gYear)` becomes
   `id > encode(1990)` because the integer ID range is monotonically increasing with numeric value.
   No decoding step is needed for comparison operations.

2. **The vocabulary is split into two tiers**:
   - **Internal vocabulary** (RAM): IRIs and literals up to 1024 bytes; frequently-accessed
     language-tagged literals; anything matching configured `languages-internal` prefixes. This is
     the "hot" dictionary — ID→string lookups without I/O.
   - **External vocabulary** (disk): long literals, rare language tags, large dataset-specific IRI
     ranges (Wikidata statement IRIs, UniProt sequence IRIs). Accessed via OS page cache; warm
     after first use.

   At Wikidata scale: 3 billion vocabulary entries, 190 GB uncompressed. Only a few GB are needed
   in RAM for typical query workloads; the rest lives on SSD.

pg_ripple's `_pg_ripple.resources (id BIGINT, value TEXT)` is a single-tier table — all entries
are PostgreSQL heap pages, all subject to the global `shared_buffers` eviction policy. For
Wikidata-scale workloads where the dictionary approaches tens of billions of entries, the vast
majority of `resources` lookups will be cold page reads.

**Implication for v0.9.0 (Scale)**: Implement a QLever-style tiered dictionary in pg_ripple:

1. **Hot tier** (UNLOGGED TABLE, `_pg_ripple.resources_hot`): IRIs shorter than a GUC-controlled
   threshold (default 512 bytes), all prefixes in the prefix registry, all predicate IRIs. These
   fit in `shared_buffers` and are always warm.

2. **Cold tier** (HEAP TABLE, `_pg_ripple.resources_cold`): Everything else — long literals,
   infrequently-used IRIs. Accessed via OS cache; tolerate I/O latency.

3. The dictionary encoder checks the hot tier first (in-process LRU cache → shared memory →
   hot table), then falls back to the cold tier.

The `resources_hot` table can be pre-warmed at server start using `pg_prewarm` or by scanning
from a `shared_preload_libraries` GUC hook in `_PG_init`.

**Also adopt QLever's ID ordering principle**: when assigning IDs to typed literals, allocate IDs
such that comparison operators preserve value ordering within each type. For integer literals,
the ID should be `i64::from_bits(value_bits_with_type_tag)` where the tag ensures the integer
range is disjoint from the IRI range. This enables range scan compilation:
`FILTER(?count > 100)` → `vp_count.o BETWEEN $lower_bound AND $i64::MAX` with no decode step.
This synergizes with the inline encoding from the Jena/TDB2 analysis — the same mechanism,
approached from "allocate IDs sorted" rather than "pack value into the ID bits".

#### 1.2.3 Patterns: precomputed subject→predicate-set index

QLever precomputes a "patterns" data structure: for each distinct set of predicates that any
subject has, one pattern entry is stored. Subjects that share the same predicate set reference the
same pattern entry. The storage cost is one pattern-id per subject (8 bytes) plus one entry per
distinct pattern in the vocabulary (relatively small for real-world KGs where entity types cluster
into a few hundred patterns).

This structure is used for:
1. **SPARQL autocompletion**: "what predicates can follow `?x` given that ?x is bound to
   entities of type `Person`?" — answered in O(1) via pattern lookup.
2. **DESCRIBE queries**: "what predicates does subject X have?" — answered directly from the
   pattern without scanning all six permutations.
3. **Statistics queries**: "how many subjects have the predicate `schema:birthDate`?" — answered
   via pattern aggregation.

pg_ripple has no equivalent. The current approach for DESCRIBE is to query each VP table for
subject X separately — N queries for N predicates. For `vp_rare`, all triples for X are in one
table but still require a full scan filtered by s = X.

**Implication for v0.4.0 (SPARQL Completeness)**: Create `_pg_ripple.subject_patterns`:

```sql
CREATE TABLE _pg_ripple.subject_patterns (
    s        BIGINT NOT NULL,
    pattern  BIGINT[] NOT NULL,  -- sorted array of predicate IDs
    PRIMARY KEY (s)
);
CREATE INDEX ON _pg_ripple.subject_patterns USING GIN (pattern);
```

The `pattern` column contains a sorted array of all predicate IDs for subject `s`. This enables:
- `DESCRIBE <iri>`: SELECT pattern FROM subject_patterns WHERE s = encode('<iri>')
  → then query each VP table in the pattern array.
- "predicates by popularity": GROUP BY unnest(pattern) ORDER BY count(*) — directly.
- SPARQL autocompletion for a pg_ripple SQL API extension.

The table is updated by the merge worker after each delta→main promotion, not on every INSERT
(too expensive). The GIN index allows "which subjects have both predicate P1 and predicate P2?"
efficiently.

#### 1.2.4 Materialized views as n-ary projection tables

QLever's materialized view feature stores the result of an arbitrary SELECT query as a sorted flat
file indexed in the same 6-permutation structure as the main data. The result can contain more
than 3 columns — addressing RDF's fundamental limitation (triples only). A query that repeatedly
needs `(subject, birthDate, name, nationality)` together creates one materialized view and joins
against it, rather than issuing four separate triple pattern joins.

The view is sorted by the first three columns; joining on the first column is O(n) merge join.
The access syntax is `SERVICE view:VIEWNAME { ... }` — the SERVICE keyword repurposed for
local sub-graph access.

pg_ripple's equivalent is **PostgreSQL's native `CREATE MATERIALIZED VIEW`**:

```sql
CREATE MATERIALIZED VIEW pg_ripple.person_facts AS
SELECT
    pg_ripple.decode(s.s) AS subject,
    pg_ripple.decode(bd.o) AS birth_date,
    pg_ripple.decode(n.o)  AS name
FROM _pg_ripple.vp_birthDate bd
JOIN _pg_ripple.vp_name n USING (s)
JOIN _pg_ripple.vp_type t USING (s)
WHERE t.o = pg_ripple.encode('schema:Person');
```

This is more powerful than QLever's materialized views (no limitations on column count, supports
UPDATE via `REFRESH MATERIALIZED VIEW CONCURRENTLY`, integrates with PostgreSQL's query planner
for automatic use), and requires zero new extension code.

**Implication**: Document in `src/sparql/` that `SERVICE <local:view-name>` in SPARQL queries
should be translated to a reference to a PostgreSQL materialized view of the corresponding name.
This makes pg_ripple's equivalent of QLever's materialized views available at v0.3.0 essentially
for free, since PostgreSQL already has this feature.

#### 1.2.5 Index rebuild vs. HTAP: QLever's update weakness

QLever requires a **full index rebuild** when the dataset changes significantly. Incremental SPARQL
1.1 Update is supported but accumulates changes in a delta that eventually forces a rebuild. The
index files are largely immutable once built. The QLever docs say: "Rebuild index: this is needed
after a significant number of updates."

This is pg_ripple's most significant architectural advantage over QLever:

- pg_ripple supports concurrent transactional writes (INSERT/DELETE/UPDATE via SPI) without any
  index rebuild, because VP tables are ordinary PostgreSQL tables with live B-tree indexes.
- The merge worker promotes delta→main in the background without interrupting queries.
- SPARQL 1.1 Update is a first-class operation, not an afterthought.

**Implication**: The pg_ripple marketing / documentation framing should explicitly contrast the
HTAP architecture with QLever's rebuild requirement. For workloads with frequent updates (streaming
data, live knowledge graphs, versioned datasets), pg_ripple is architecturally superior. For
workloads with infrequent bulk loads and intensive read queries (Wikidata-style), QLever's
flat-file permutations give a 10–100× raw query throughput advantage.

This is the fundamental design trade-off to document: **QLever = fast bulk reads, update requires
rebuild; pg_ripple = transactional reads + writes, slightly slower bulk scans**.

#### 1.2.6 GeoSPARQL performance comparison with PostgreSQL+PostGIS

The SIGSPATIAL'25 paper from the QLever team explicitly compared QLever's spatial join
performance to **PostgreSQL+PostGIS**. QLever's geometry processing uses a custom R-tree built
into the index. The comparison covers spatial joins at OpenStreetMap scale (10B+ geometries).

This paper is directly relevant because:
1. It provides a quantified benchmark of QLever vs. PostgreSQL for spatial RDF data.
2. pg_ripple runs inside PostgreSQL and can use PostGIS natively — the spatial extension is
   already present and indexes are already built if the deployment uses it.
3. A SPARQL extension `geof:distance`, `geof:within`, `geof:intersects` in pg_ripple can compile
   to `ST_Distance`, `ST_Within`, `ST_Intersects` SQL calls, leveraging existing PostGIS GIST
   indexes directly. No custom R-tree needed.

**Implication for v0.8.0 (SPARQL Extensions)**: Add GeoSPARQL function translation in
`src/sparql/functions/geo.rs` that maps GeoSPARQL built-ins to PostGIS SQL functions. The
presence of PostGIS is detected at extension load time via `SELECT extname FROM pg_extension WHERE
extname = 'postgis'`; if present, GeoSPARQL functions are available. The PostGIS GIST spatial
index will in many cases outperform QLever's custom R-tree because it benefits from all
PostgreSQL join order optimizations and can be combined with non-spatial predicates in the same
query. Cite the SIGSPATIAL'25 paper as the motivation.

---

## 2. Summary: Changes to pg_ripple Architecture

| Source | Lesson | Target module | Roadmap version |
|---|---|---|---|
| QLever | Document merge-join efficiency of VP tables; ORDER BY hints in multi-join CTEs | `src/sparql/emitter.rs` | v0.3.0 |
| QLever | DBLP/Wikidata/UniProt as reference benchmarks; document 5× QLever gap as accepted trade-off | `implementation_plan.md` | v0.2.0 |
| QLever | Tiered dictionary: hot (UNLOGGED) + cold (HEAP) with pg_prewarm warm-up | `src/dictionary/` | v0.9.0 |
| QLever | ID ordering: allocate typed-literal IDs monotonically within type for range-scan compilation | `src/dictionary/` | v0.3.0 |
| QLever | `_pg_ripple.subject_patterns (s, pattern BIGINT[])` with GIN index for DESCRIBE + autocomplete | `src/storage/` | v0.4.0 |
| QLever | `SERVICE <local:view>` → PostgreSQL `MATERIALIZED VIEW` translation in SPARQL compiler | `src/sparql/` | v0.3.0 |
| QLever | Document HTAP-vs-rebuild trade-off explicitly vs. QLever | `implementation_plan.md` | v0.1.0 |
| QLever | GeoSPARQL → PostGIS translation; detect PostGIS at `_PG_init`; cite SIGSPATIAL'25 | `src/sparql/functions/geo.rs` | v0.8.0 |
