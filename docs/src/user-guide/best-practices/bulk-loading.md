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

## HTAP delta growth during bulk loads (v0.6.0)

With v0.6.0's HTAP storage layout, all inserts land in delta tables first. During a large bulk load the delta tables can grow very large before the merge worker has a chance to compress them into main.

**Symptoms of runaway delta growth**:
- Queries on bulk-loaded predicates scan large delta tables (slower than expected)
- `SELECT pg_ripple.stats() -> 'unmerged_delta_rows'` shows millions of rows
- `pg_stat_user_tables` shows very high `n_live_tup` on `*_delta` tables

**Strategies**:

### Option A: Tune merge aggressiveness

Before starting a large load, lower the merge thresholds so the worker keeps up:

```sql
ALTER SYSTEM SET pg_ripple.merge_threshold = 10000;
ALTER SYSTEM SET pg_ripple.latch_trigger_threshold = 5000;
ALTER SYSTEM SET pg_ripple.merge_interval_secs = 5;
SELECT pg_reload_conf();
```

After the load, restore production values.

### Option B: Periodic manual compact

For offline bulk loads where query freshness during the load is not important, call `compact()` at regular intervals:

```bash
for f in chunk_*; do
    psql -c "SELECT pg_ripple.load_ntriples_file('/data/$f');"
    # Compact every 10 chunks
    if (( i % 10 == 0 )); then
        psql -c "SELECT pg_ripple.compact();"
    fi
    ((i++))
done
# Final compact when done
psql -c "SELECT pg_ripple.compact();"
```

### Option C: Disable HTAP for the load predicate (advanced)

For predicates that will only ever be bulk-loaded (not streamed), you can keep them in the flat layout by migrating back after the load. This is an advanced use case — contact the maintainers for guidance.

### Final cleanup

After a bulk load and compact cycle, run `ANALYZE` to update planner statistics:

```sql
-- Analyze the delta, main, and vp_rare tables
ANALYZE _pg_ripple.vp_rare;

-- Analyze all objects in the _pg_ripple schema
DO $$
DECLARE t text;
BEGIN
  FOR t IN SELECT relname FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
           WHERE n.nspname = '_pg_ripple'
             AND c.relkind = 'r'
  LOOP
    EXECUTE 'ANALYZE _pg_ripple.' || quote_ident(t);
  END LOOP;
END $$;
```

## Parallel loads

Multiple concurrent `load_ntriples()` calls are safe — the dictionary insert uses `ON CONFLICT DO NOTHING … RETURNING` which is MVCC-safe. However, heavy concurrent writes to `vp_rare` can cause lock contention. For best throughput, load from a single database connection.
