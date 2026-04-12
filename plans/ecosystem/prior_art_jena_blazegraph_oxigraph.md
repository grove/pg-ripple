# Prior Art Analysis: Apache Jena / TDB2, Blazegraph, Oxigraph

> **License notes:**
> - **Apache Jena / TDB2**: Apache License 2.0 — safe to reference directly, including code patterns.
> - **Oxigraph**: Apache-2.0 / MIT (dual) — safe to reference directly.
> - **Blazegraph**: GPL-2.0. Architectural ideas only; no code reference.
>   Additionally: the repository was **archived on March 23, 2026** and is now read-only. The
>   Wikidata Query Service, which was Blazegraph's most prominent deployment, has migrated away.
>   Blazegraph is effectively a historical reference system; no future development is expected.
>
> This document records design insights drawn from public source code and documentation.
> No GPL code is copied; the analysis is used solely to inform pg_triple's architecture.

---

## 1. Apache Jena / TDB2

**Repository**: https://github.com/apache/jena  
**License**: Apache 2.0  
**What it is**: The most widely deployed open-source Java RDF framework. The two storage backends
are TDB1 (B+Tree, single-writer) and TDB2 (copy-on-write MVCC, concurrent readers + writer). The
Fuseki component provides a SPARQL 1.1 HTTP server fronting TDB. Large institutional deployments
(national libraries, biomedical ontologies, government LOD datasets). Actively maintained under
the ASF.

### 1.1 Relevance

Jena/TDB2 is the reference implementation for SPARQL query optimization in an open-source triple
store. Its statistics-based BGP (Basic Graph Pattern) optimizer and its inline-value NodeId
encoding are both directly applicable to pg_triple.

### 1.2 Lessons

#### 1.2.1 Full-covering B+Trees for all six quad permutations — and when to not do that

TDB1/TDB2 stores quads in six B+Tree indexes covering every access pattern:

```
Default graph (triples): SPO  POS  OSP
Named graphs (quads):   SPOG  POSG  OSPG  GSPO  GPOS  GOSP
```

Each index is a **full-covering** B+Tree — all four components of the quad are in every index
key, so a single index lookup returns the complete quad with no join back to a primary table. The
trade-off: 6× storage overhead compared to storing each quad once.

pg_triple's VP table approach sidesteps this differently: predicate partitioning means that within
a single VP table `vp_{id}(s, o, g)`, the predicate dimension is already fixed — the table name
encodes it. We need only two indices per VP table: `(s, o, g)` and `(o, s, g)`. This is
structurally analogous to TDB's `SPO` and `OSP` for a single predicate, but without the storage
overhead for the graph and full-scan permutations — those are handled by the `vp_rare(p, s, o, g)`
fallback.

**The key tension**: TDB's full-covering approach makes it straightforward to add SPARQL queries
that ask "which predicates does subject X have?" — the `SP` or `SPOG` index answers it directly.
In pg_triple, the equivalent is `SELECT DISTINCT p FROM _pg_triple.predicates JOIN vp_rare ...
UNION SELECT unnest(...)` — more expensive. The `_pg_triple.predicates` catalog partially
addresses this for common predicates, but a `(s, p)` secondary index on `vp_rare` is needed
for DESCRIBE queries' subject-to-predicate enumeration.

**Action**: Add a `(s, p)` index to `vp_rare` alongside the planned `(g, p, s, o)` index. Both
are needed for complete SPARQL 1.1 support (DESCRIBE enumerates all predicates for a subject;
graph-drop enumerates all subjects in a graph).

#### 1.2.2 Inline values in NodeId: skip the dictionary for numbers and dates

TDB2 uses an 8-byte NodeId where the high bit distinguishes pointer-to-table (high bit 0) from
inline-encoded value (high bit 1). Inline-encoded values pack a 6-bit type tag and a 56-bit value
into the remaining 63 bits, covering:

- `xsd:integer`, `xsd:decimal`, `xsd:boolean`
- `xsd:dateTime`, `xsd:date` (8000-year range, 1ms precision, timezone)
- `xsd:float`, `xsd:double`
- Derived XSD integer types (`xsd:int`, `xsd:long`, etc.)

For these types, no dictionary lookup is ever needed — the value is fully contained in the i64.
String literals still go to the node table.

pg_triple currently encodes all literals as i64 IDs via XXH3-128, requiring a dictionary lookup to
decode any literal to its string form. This is correct but unnecessarily expensive for numeric
and boolean literals which appear as FILTER constants and in BIND expressions frequently.

**Implication for `src/dictionary/`**: Implement the same tagging scheme. Designate the top 8
bits of the i64 as a type tag:

```
bit 63 = 0: dictionary pointer (existing XXH3-128 ID, truncated to 63 bits)
bit 63 = 1: inline value
  bits 56-62 (7 bits): type code
    0x01 = xsd:integer (i56 signed, bits 0-55)
    0x02 = xsd:boolean (bit 0)
    0x03 = xsd:double  (f64 truncated to 56 bits, lossy — keep as dict for lossless)
    0x04 = xsd:dateTime (packed timestamp)
    ...
```

For FILTER expressions like `FILTER(?age > 18)`, the comparison can then be done directly on i64
values without any dictionary round-trip: encode the constant `18` as inline at translation time,
and the comparison `vp_age.o > $1` (with `$1 = inline_encode(18)`) is a pure integer comparison
on the VP table column.

This is a significant optimization for numeric-heavy analytical queries and should be planned for
`src/dictionary/inline.rs` in v0.3.0 (Basic SPARQL).

**Important**: The inline tag scheme must be consistent — the highest bit must never be set on a
dictionary pointer ID. Since XXH3-128 is uniformly distributed, 50% of hashes would have bit 63
set. Solution: mask dictionary IDs to 63 bits. The theoretical collision probability on 63-bit
hashes over a 10-billion-triple dataset is ~5×10⁻¹⁰ — acceptable, but document it.

#### 1.2.3 Statistics-based BGP optimizer with a persistent stats file

Jena's TDB optimizer uses a per-database `stats.opt` file containing predicate frequency counts
in a small domain-specific language:

```
(stats
  (meta (count 250000000))
  (rdf:type 50000000)        -- 50M rdf:type triples
  (foaf:name 12000000)       -- 12M foaf:name triples
  ((VAR :identifier TERM) 1) -- inverse functional: at most 1 match
  (other 1000)               -- default for unknown predicates
)
```

The optimizer uses these counts to estimate the selectivity of each triple pattern and reorder the
BGP to join the most selective patterns first. Patterns with bound subject/object/graph are scored
lower (more selective) than patterns with all-variable positions. The file is generated by
`tdbstats` and can be manually edited to add semantic hints (e.g., marking a property as
approximately inverse-functional).

pg_triple already has better data for this purpose: every VP table is a genuine PostgreSQL table
with live `pg_statistic` entries updated by `AUTOVACUUM`. PostgreSQL's `n_distinct`, histogram
bounds, and correlation statistics are exactly what the BGP optimizer needs to estimate pattern
cardinalities.

**Implication for `src/sparql/optimizer.rs`** (v0.12.0 Performance):

The join reordering pass should use `pg_catalog.pg_stats` and `pg_catalog.pg_class.reltuples` to
estimate cardinality for each triple pattern. The order of operations:

1. For each triple pattern in the BGP, compute an estimated row count:
   - If predicate is known: look up `reltuples` from `pg_class` for `_pg_triple.vp_{id}`.
   - If subject is bound: multiply by `1/n_distinct_subject` from `pg_stats`.
   - If object is bound: multiply by `1/n_distinct_object` from `pg_stats`.
2. Sort patterns by ascending estimated row count.
3. Bind the cheapest pattern first; propagate bound variables into subsequent patterns.

This is exactly the Jena approach, but using live PostgreSQL statistics rather than a static file.
The statistics are always current (AUTOVACUUM-maintained) and require no manual `tdbstats` step.

The Jena abbreviation for "inverse functional property" — `((VAR :prop TERM) 1)` — maps to
SHACL's `sh:uniqueLang` or `sh:maxCount 1`. When a shape asserts `sh:maxCount 1` for predicate P,
the optimizer should set the cardinality estimate for `(?x, P, ?y)` to the smaller of
`{estimated subjects, 1}` — the same effect as Jena's manual rule.

#### 1.2.4 TDB2 copy-on-write compaction: the `Data-NNNN` generation model

TDB2 stores database generations as separate `Data-NNNN` directories. Compaction creates a new
generation from the current live state, then atomically switches to pointing at it. Previous
generations are retained until manually deleted. This gives:

- **Instant crash recovery**: the current generation is always a complete, consistent database.
- **Zero-downtime compaction**: reads continue against the old generation while the new one is
  being built.
- **Historical retention**: older generations can be retained briefly for backup purposes.

pg_triple's HTAP merge worker moves rows from `vp_{id}_delta` to `vp_{id}_main`. The analogous
compaction pattern is: at a configurable interval, create a new `vp_{id}_main` table, bulk-insert
the current delta into it, CLUSTER by the primary index, then atomically `ALTER TABLE RENAME` to
replace the old main table. This allows:

1. The BRIN index on the new main table to be built from scratch on sorted data (much more
   effective than incremental inserts into an existing BRIN).
2. Zero-downtime because queries use `UNION ALL delta + main`; the rename is a catalog-only
   operation visible instantly.
3. The old main table is retained briefly for a configurable `pg_triple.merge_retention_seconds`
   before `DROP TABLE`.

The current plan to `INSERT INTO ... SELECT` incrementally into the existing main partition
produces a suboptimal BRIN index because BRIN requires data to be physically sorted on disk to
be effective. The TDB2 generation model suggests building a fresh main table for each major merge
cycle.

---

## 2. Blazegraph

**Repository**: https://github.com/blazegraph/database  
**License**: GPL-2.0  
**Status**: **Archived March 23, 2026 — no further development.** The Wikidata Query Service
(WQDS), Blazegraph's largest and most prominent deployment, has migrated to a different engine.
The repository is read-only.  
**What it was**: A high-performance Java RDF/graph database claiming support for 50 billion edges
on a single machine. Built a custom B-Tree and WORM (Write Once Read Many) append-only journal.
Powers — or powered — Fortune 500 deployments in life sciences and cyber analytics.

### 2.1 Relevance

Blazegraph's archival is itself a signal. A system deployed at Wikidata scale (7+ billion triples,
millions of SPARQL queries per day) was abandoned rather than maintained. The reasons are worth
understanding to avoid the same failure modes.

### 2.2 Lessons

#### 2.2.1 The Wikidata migration: why Blazegraph died

Blazegraph's failure mode was **maintenance debt on a custom storage stack**. The core was a
bespoke Java B-Tree and journal implementation — not built on an existing storage engine. When
the original authors moved on and Wikidata's query load grew, there was no community capable of
maintaining the custom storage layer. Bugs accumulated, SPARQL compliance gaps remained, and the
cost of adding features (aggregation push-down, property path optimisation) was too high.

**Lesson for pg_triple**: do not build custom storage. pg_triple's VP tables are PostgreSQL heap
tables with standard B-tree and BRIN indices. Every PostgreSQL DBA knows how to maintain them;
every PostgreSQL release improves them; `pg_dump`/`pg_restore`, VACUUM, REINDEX all work without
extension-specific tooling. This is the right choice and Blazegraph's demise confirms it.

#### 2.2.2 WORM append-only journal: crash safety without undo log

Blazegraph's `bigdata-journal` uses a Write-Once/Read-Many strategy: committed data is never
overwritten in place. Each transaction appends new versions of modified pages. The journal is
therefore inherently crash-safe — there is no partial write that can corrupt existing data, only
an incomplete append that will be truncated at recovery.

The architectural equivalent in pg_triple is PostgreSQL's WAL. The WAL is also append-only. The
implication: **VP table writes are already crash-safe** by virtue of being PostgreSQL heap writes.
No additional journaling layer is needed for the delta partition. The one case where this is
non-obvious is the dictionary: `_pg_triple.resources (id, value)` insertions are also WAL-backed
and crash-safe.

The Blazegraph lesson is a negative one — a confirmation that pg_triple's decision to use
PostgreSQL as the storage layer is correct, not an alternative design to adopt.

#### 2.2.3 Namespace API as a multi-tenancy primitive

Blazegraph's namespace feature allows multiple independent RDF datasets within a single server
process, each with their own SPARQL endpoint at `/blazegraph/namespace/NAME/sparql`. The isolation
is logical (separate B-Trees), not security-level (the auth documentation explicitly warns that
namespace isolation does not provide security, only logical partitioning).

pg_triple's natural multi-tenancy model is PostgreSQL schemas. Each tenant gets:
- `CREATE SCHEMA tenant_a;`
- VP tables created as `tenant_a.vp_{id}` rather than `_pg_triple.vp_{id}`
- Per-schema predicates catalog and resource dictionary

This is both more flexible (PostgreSQL schema isolation is used by every major SaaS multi-tenant
Postgres deployment) and more secure (RLS + schema search path provide real isolation) than
Blazegraph's namespace approach. The design should be noted in v0.13.0 (Admin & Security) as the
recommended multi-tenancy pattern.

---

## 3. Oxigraph

**Repository**: https://github.com/oxigraph/oxigraph  
**License**: Apache-2.0 / MIT (dual)  
**What it is**: An actively developed Rust SPARQL database targeting correctness and compliance.
Backed by RocksDB (LSMT) for on-disk storage or a pointer-chained hash set for in-memory use.
Exposes libraries for Rust, Python (`pyoxigraph`), and WebAssembly (npm package). Most recent
release: v0.5.6 (March 2026). Approximately 1.6k GitHub stars.

**Critical shared dependency**: Oxigraph publishes `spargebra` (SPARQL parser), `oxrdf` (RDF data
structures), `oxttl` (Turtle/TriG parser), `oxrdfxml`, `oxjsonld`, `sparopt` (optimizer), and
`sparesults` — all of which pg_triple either already uses or plans to use. We are downstream
dependents of Oxigraph's published crates.

### 3.1 Relevance

Oxigraph is the highest-relevance prior art in this document: same language (Rust), same crate
dependencies (`spargebra`, `oxrdf`, `oxttl`), same target (SPARQL-compliant RDF store). The
architectural differences (RocksDB vs. PostgreSQL heap; six-permutation indexing vs. VP tables)
are illuminating precisely because the starting points are so similar.

### 3.2 Lessons

#### 3.2.1 Index layout: six quad permutations in RocksDB vs. VP tables

Oxigraph stores quads in nine RocksDB key-value tables:

```
Default graph:    spo  pos  osp
Named graph quads: spog posg ospg gspo gpos gosp
Plus: id2str (string dictionary)
```

All nine tables are full-covering: every element of the quad appears in each key. The total
write amplification is 3× for default-graph triples and 6× for named-graph quads, plus the
`id2str` dictionary write.

**RocksDB-specific reason**: RocksDB's LSMT has high write throughput but range scans are fast
only with a key-sorted layout. Having all six permutations means any triple access pattern hits
a sorted range scan. Without multiple permutations, a lookup like "find all triples with object X"
would require a full table scan.

pg_triple's VP tables avoid this trade-off: within a VP table for predicate P, the `(s, o, g)`
B-tree handles O-first and S-first lookups efficiently through different scan strategies. And
since each VP table only contains triples for a single predicate, the predicate dimension is
collapsed. This eliminates the P-leading indices (POSG, POGS) that Oxigraph needs for predicate
lookups.

The Oxigraph layout is correct and general; the VP layout is more space-efficient at the cost of
being specific to the predicate-clustered workload. The comment in Oxigraph's wiki —
"TODO: Can we reduce the number of stored combinations without hitting too much performances?" —
is effectively answered by VP partitioning: yes, but only if your workload is predicate-anchored.

#### 3.2.2 128-bit hash as term ID: SipHash-2-4 vs. XXH3-128

Oxigraph hashes IRIs and string literals to 128-bit SipHash-2-4 IDs. The hash is the complete
term representation in all indices — there is no separate "IRI to ID" lookup table. The `id2str`
table maps 128-bit ID → string for decode, but the encode path is just `hash(iri_string)`.

pg_triple uses XXH3-128 truncated to 63 bits (fitting a signed i64, leaving room for the inline
type tag from §1.2.2). The functional design is the same; the differences are:

| Aspect | Oxigraph (SipHash-2-4-128) | pg_triple (XXH3-128→i63) |
|---|---|---|
| Speed | ~1.5 GB/s (seeded) | ~30–50 GB/s (unseeded) |
| Collision resistance | Designed for adversarial inputs | Non-adversarial only |
| Seed | Randomised per-process (DoS protection) | Fixed (deterministic) |
| Storage | 16 bytes | 8 bytes (fits PostgreSQL BIGINT) |

The per-process random seed in Oxigraph means the same IRI gets a different ID in different
server runs — IDs cannot be compared across process restarts. pg_triple's fixed XXH3-128 seed
means IDs are deterministic across restarts, which is important for `pg_dump`/`pg_restore`
compatibility: a restored database has the same IDs as the original.

**One Oxigraph concern to learn from**: their wiki notes "No collision found on 2019 Wikidata
dump". pg_triple should add the same validation: a CI test that encodes the full Wikidata dump
(or a representative large dataset) and checks for any collision in `_pg_triple.resources`.

#### 3.2.3 Dictionary GC is an unsolved problem — design for it now

Oxigraph's wiki has an explicit TODO:

> **How is string garbage collection handled?**
> Currently strings are never removed from the database even if the corresponding term is removed.
> TODO: figure out a way to implement it without hitting too much read and write performances.

This is a known correctness/space deficiency: deleting all triples that reference an IRI or
literal does not free the `id2str` entry. Over time (especially with workloads that generate
many blank nodes or temporary IRIs) the dictionary grows without bound.

pg_triple faces exactly the same problem with `_pg_triple.resources`. The fix requires either:

1. **Reference counting**: `resources (id, value, refcount)`. Increment on encode, decrement on
   delete. When `refcount = 0`, the entry is eligible for deletion. Problem: tracking refcounts
   correctly during batch operations and rollbacks is hard; a transaction that inserts and then
   rolls back must not decrement the refcount.

2. **Periodic GC scan**: A background worker (separate from the merge worker) periodically runs:
   ```sql
   DELETE FROM _pg_triple.resources r
   WHERE NOT EXISTS (
       SELECT 1 FROM _pg_triple.all_vp_ids WHERE id = r.id
   )
   ```
   This is O(dictionary_size) but can run as a low-priority background job. The `all_vp_ids`
   view collects all `s`, `o`, `g` values across all VP tables via `UNION ALL`.

3. **Append-only (Oxigraph's current approach)**: Accept dictionary growth. Reasonable if the
   working set of IRIs/literals is bounded and deletions are rare (e.g., bulk-loaded static
   datasets).

**Recommendation**: Option 3 is acceptable for v0.1–v0.8. Option 2 (periodic GC scan) should be
implemented in v0.11.0 (Maintenance) as `pg_triple.vacuum_dictionary()` and called from the
admin vacuum routine. Option 1 (refcounting) is too complex relative to its benefit for a
PostgreSQL-backed store.

#### 3.2.4 `sparopt` crate: a shared SPARQL optimizer to track and potentially contribute to

Oxigraph's `sparopt` crate is a standalone SPARQL optimizer that operates on the `spargebra`
algebra IR. It is dual Apache-2.0/MIT. pg_triple already uses `spargebra` for parsing — `sparopt`
is the natural next step in the pipeline.

`sparopt` currently implements:
- Hash join vs. nested loop join selection
- Basic greedy join reordering by decreasing triple pattern selectivity
- Filter pushdown

These are exactly the optimizations pg_triple needs in `src/sparql/optimizer.rs`.

**Options:**

1. **Use `sparopt` directly as a dependency**: Call `sparopt::Optimizer::optimize(algebra)` to
   get a rewritten algebra, then translate to SQL. The output is a `spargebra`-compatible algebra
   tree, so no IR bridging is needed. This gives pg_triple the optimizer for free and contributes
   usage/feedback to the Oxigraph project.

2. **Implement independently**: pg_triple's optimizer has access to PostgreSQL statistics that are
   unavailable to `sparopt` (which is storage-agnostic and uses heuristics, not statistics).
   A pg_triple-specific optimizer can use `pg_stats.n_distinct` and `pg_class.reltuples` for
   far better cardinality estimates than `sparopt`'s heuristics.

**Recommendation**: Start with option 1 (`sparopt` as a dependency) for v0.3.0 to get a working
optimizer quickly. Add a pg_triple-specific statistics pass in v0.12.0 that post-processes
`sparopt`'s output using live `pg_stats` data. Contribute that statistics interface back to
`sparopt` if it can be made storage-agnostic (difficult but possible via a callback trait).

The relationship with the Oxigraph project should be monitored and a contribution strategy
established. As a downstream user of `spargebra`, `oxttl`, `oxrdfxml`, `oxrdf`, and potentially
`sparopt`, pg_triple benefits from their maintenance. Filing issues and opening PRs is the right
form of reciprocity.

#### 3.2.5 Volcano iterator model: single-threaded, simple, proven

Oxigraph's query evaluation uses the Volcano (pull-based iterator) model. Each operator (`Join`,
`Filter`, `Project`, `Sort`, `Distinct`) implements an iterator interface. Query plans are trees
of iterators; results are pulled one at a time from the root. Single-threaded.

This is the same model PostgreSQL uses internally for query execution. pg_triple's SPARQL→SQL
translation effectively delegates the execution model to PostgreSQL's executor, which is also
Volcano-based. There is no benefit in implementing a separate SPARQL-level Volcano executor —
PostgreSQL already provides one.

The practical implication: **SPARQL query evaluation in pg_triple should always go through
PostgreSQL's executor** (via the generated SQL and SPI), not through a Rust-level iterator tree.
This is already the plan. Oxigraph's development confirms it: they have a full
Volcano-in-Rust implementation and note it is "currently single-threaded for simplicity" with a
TODO for multi-threading. PostgreSQL's executor is already parallel-capable where pg_triple
benefits from that automatically by emitting appropriate SQL (e.g., `/*+ parallel(none) */` or
leaving it to the planner).

#### 3.2.6 RocksDB LSMT vs. PostgreSQL heap: write amplification tradeoffs

Oxigraph chose RocksDB (LSMT) for its excellent write performance. LSMT writes are sequential
appends to memtables (WAL + sorted file), giving O(1) amortised write cost per key-value pair.
Read performance degrades as levels accumulate; compaction trades CPU for read performance.

pg_triple's VP table delta partition uses PostgreSQL heap (row store), which is random-access
B-tree insertion. For very high-frequency individual-triple inserts, this has higher per-insert
cost than RocksDB's memtable append.

However, pg_triple's HTAP architecture already addresses this: the delta partition absorbs
random writes, and the merge worker amortises the B-tree insertion cost into bulk batches. The
main partition receives data in sorted order during merge, giving exactly the same sequential-write
pattern that makes LSMT efficient.

**The key lesson from Oxigraph's LSMT use**: bulk-load performance matters more than single-triple
insert performance for triple stores. The `pg_triple.load_turtle(path)` bulk loader should bypass
individual `INSERT` statements and use `COPY` into the delta partition, achieving the same
sequential-write throughput as LSMT's memtable flush.

---

## 4. Summary: Changes to pg_triple Architecture

| Source | Lesson | Target module | Roadmap version |
|---|---|---|---|
| Jena/TDB2 | Add `(s, p)` index to `vp_rare` for DESCRIBE subject→predicate enumeration | `src/storage/` migration | v0.1.0 / v0.4.0 |
| Jena/TDB2 | Inline numeric/boolean/date types in i64 via type-tag bits | `src/dictionary/inline.rs` (new) | v0.3.0 |
| Jena/TDB2 | BGP join reordering using `pg_stats.n_distinct` + `reltuples` statistics | `src/sparql/optimizer.rs` | v0.12.0 |
| Jena/TDB2 | SHACL `sh:maxCount 1` → treat predicate as inverse-functional in optimizer | `src/sparql/optimizer.rs` | v0.6.0 |
| Jena/TDB2 | Fresh-table compaction: rebuild `vp_{id}_main` from scratch each merge cycle | `src/storage/merge.rs` | v0.5.0 |
| Blazegraph | Custom storage stack = maintenance trap — keep pg heap, no custom B-Tree | architecture principle | ongoing |
| Blazegraph | PostgreSQL schema per tenant as multi-tenancy primitive | `src/admin/` | v0.13.0 |
| Oxigraph | `sparopt` as v0.3.0 optimizer dependency; replace with stats-based optimizer at v0.12.0 | `src/sparql/optimizer.rs` | v0.3.0 / v0.12.0 |
| Oxigraph | CI test: encode large dataset, assert zero hash collisions in `_pg_triple.resources` | `tests/` | v0.2.0 |
| Oxigraph | Dictionary GC: `pg_triple.vacuum_dictionary()` periodic scan removes unreferenced entries | `src/admin/` | v0.11.0 |
| Oxigraph | Bulk load via `COPY` into delta partition (not individual INSERTs) | `src/storage/loader.rs` (new) | v0.2.0 |
| Oxigraph | Monitor `spargebra`/`oxttl`/`oxrdf` upstream; maintain contribution strategy | dependency policy | ongoing |
