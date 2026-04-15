# Bulk Loading Best Practices

## Batch size

`load_ntriples()` and `load_turtle()` process the entire input in a single batch. For very large files (hundreds of millions of triples) split the input into chunks of 1–10 million triples each and load them sequentially. This keeps transaction sizes manageable and allows periodic `ANALYZE` runs between batches.

```bash
# Split a large NT file into 1M-triple chunks
split -l 1000000 large.nt chunk_
for f in chunk_*; do
    psql -c "SELECT pg_ripple.load_ntriples_file('/data/$f');"
    psql -c "ANALYZE _pg_ripple.vp_rare;"
done
```

## VP promotion threshold

The default threshold is 1000 triples. For workloads dominated by a small number of very common predicates (e.g. `rdf:type`) consider lowering the threshold to trigger promotion sooner:

```sql
SET pg_ripple.vp_promotion_threshold = 100;
```

After promotion, dedicated VP tables get B-tree indexes on `(s, o)` and `(o, s)`, which are much faster for predicate-specific lookups than the shared `vp_rare` table.

## ANALYZE after large loads

The PostgreSQL query planner relies on table statistics to choose join strategies. After loading more than ~100K triples, run:

```sql
-- Analyze the shared rare table
ANALYZE _pg_ripple.vp_rare;

-- Analyze any newly promoted VP tables
-- (replace XXX with the actual predicate IDs shown in _pg_ripple.predicates)
ANALYZE _pg_ripple.vp_XXX;
```

Without fresh statistics the planner may choose a sequential scan over an index scan on the VP tables.

## Blank-node scoping

Each call to a bulk-load function is an independent blank-node scope. If you load two files that each contain `_:b0`, they will get different dictionary IDs — as required by the RDF specification.

**This means**: do not split an N-Triples file that uses blank nodes across multiple `load_ntriples_file()` calls if the blank nodes are shared across the split point. Either load the complete file in one call, or use globally unique blank node IDs (e.g. UUID-based `_:b_{uuid}`).

## Using COPY for extremely large datasets

For multi-billion-triple loads, consider a two-phase approach:

1. Pre-encode terms to `BIGINT` IDs using `pg_ripple.encode_term()` in a staging script
2. Use PostgreSQL `COPY` to stream data directly into the target VP tables

This bypasses the per-row dictionary lookup overhead in the Rust parse-and-insert path. See the Bulk Load implementation notes in `plans/implementation_plan.md` for details.

## Parallel loads

Multiple concurrent `load_ntriples()` calls are safe — the dictionary insert uses `ON CONFLICT DO NOTHING … RETURNING` which is MVCC-safe. However, heavy concurrent writes to `vp_rare` can cause lock contention. For best throughput, load from a single database connection.
