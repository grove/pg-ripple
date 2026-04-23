-- benchmarks/merge_throughput.sql
-- Measures HTAP merge worker throughput at various worker counts.
--
-- Usage:
--   pgbench -n -f benchmarks/merge_throughput.sql -c 8 -j 4 -T 300 <db>
--
-- Parameters (set via environment or psql variables before running):
--   :merge_workers  — number of merge background workers (default 1)
--
-- The script inserts triples with varying predicates to trigger VP-table
-- creation and merge cycles.  Adjust :n_predicates to tune merge pressure.

\set n_predicates 20
\set subject 'http://bench.example/subject-' || (random() * 100000)::int
\set predicate 'http://bench.example/predicate-' || (random() * :n_predicates)::int
\set object '"value-' || (random() * 1000000)::int || '"'

SELECT pg_ripple.insert_triple(
    '<' || :'subject' || '>',
    '<' || :'predicate' || '>',
    :'object'
);
