[← Back to Blog Index](README.md)

# SPARQL on Citus: Shard-Pruning for Distributed Knowledge Graphs

## How pg_ripple eliminates worker fan-out by mapping IRIs to physical shards

---

A single PostgreSQL instance handles knowledge graphs up to a few hundred million triples. Beyond that, you need to distribute — and Citus is the way to do it without leaving PostgreSQL.

pg_ripple on Citus distributes VP tables across worker nodes using subject-based sharding. SPARQL queries that bind a subject can be pruned to a single shard, eliminating the coordinator fan-out that makes distributed graph queries expensive.

---

## The Distribution Model

Each VP table is distributed by subject:

```sql
SELECT create_distributed_table('_pg_ripple.vp_42', 's');  -- foaf:name
SELECT create_distributed_table('_pg_ripple.vp_87', 's');  -- foaf:mbox
SELECT create_distributed_table('_pg_ripple.vp_291', 's'); -- rdf:type
```

All VP tables use the same distribution column (`s` — subject). This means all triples about the same subject land on the same shard. A star pattern for a single entity hits one shard.

---

## Shard Pruning

When the SPARQL query binds a subject to a specific IRI:

```sparql
SELECT ?p ?o WHERE {
  ex:alice ?p ?o .
}
```

pg_ripple encodes `ex:alice` to its dictionary ID (say, 847291), computes the shard that contains `s = 847291`, and routes the query to that single worker. The other workers are never contacted.

```
Without shard pruning:  Coordinator → 32 workers → merge → result
With shard pruning:     Coordinator → 1 worker → result
```

For point queries (DESCRIBE, entity lookups, star patterns with bound subjects), this is a 10–100× speedup — the difference between querying 32 workers and querying 1.

---

## Multi-Hop Carry-Forward

Shard pruning works for single-hop queries. But graph queries often traverse multiple hops:

```sparql
SELECT ?name WHERE {
  ex:alice foaf:knows ?friend .
  ?friend foaf:name ?name .
}
```

After the first hop (find Alice's friends), the bound subjects for the second hop (`?friend` values) might be on different shards. Naive execution would fan out to all workers.

pg_ripple's carry-forward optimization batches the intermediate results by shard:

1. Execute hop 1 on Alice's shard. Get friends: `[bob@shard3, carol@shard7, dave@shard3]`.
2. Group by shard: `{shard3: [bob, dave], shard7: [carol]}`.
3. Execute hop 2 on shard3 for bob and dave. Execute hop 2 on shard7 for carol.
4. Merge results.

This contacts 2 shards instead of 32. The fan-out is proportional to the data distribution, not the cluster size.

---

## Property Path Push-Down

Recursive property paths (`foaf:knows+`) are pushed down to individual workers when possible:

```sparql
SELECT ?reachable WHERE {
  ex:alice foaf:knows+ ?reachable .
}
```

If the `foaf:knows` graph is co-located by subject (which it is with subject-based distribution), the recursive CTE can run on each shard independently and the results are merged on the coordinator.

For graphs where the social network is clustered (friends tend to be on the same shard — common when IRIs are distributed by hash), most of the recursion happens locally. Only cross-shard edges require coordinator coordination.

---

## Dictionary Distribution

The dictionary table is a reference table (replicated to all workers):

```sql
SELECT create_reference_table('_pg_ripple.dictionary');
```

This means dictionary lookups (encoding IRIs to integers, decoding integers to IRIs) are always local — no network round trip. The trade-off is storage: the dictionary is duplicated on every worker. For a dictionary with 10 million entries (~2 GB), this is acceptable.

---

## Inference on Citus

Datalog inference on a distributed cluster has unique challenges:

- **Semi-naive joins span shards.** The join `rdf_type(X, C) :- rdf_type(X, C'), rdfs_subClassOf(C', C)` may need to join data from different shards.
- **SID allocation.** Statement IDs must be globally unique. pg_ripple uses per-worker local SID ranges to avoid coordinator round trips during inference.
- **Merge workers.** Each worker runs its own HTAP merge independently.

pg_ripple handles distributed inference by running stratum evaluation per-worker (for rules that only reference co-located predicates) and coordinator-mediated joins (for rules that require cross-shard data). The optimizer determines which approach to use based on the distribution columns and join keys.

---

## The Numbers

On a 4-worker Citus cluster with 200 million triples:

| Query Type | Without pruning | With pruning |
|-----------|----------------|-------------|
| DESCRIBE (single entity) | 45ms | 3ms |
| Star pattern (bound subject) | 120ms | 8ms |
| 2-hop traversal (bound start) | 350ms | 25ms |
| Full graph scan (no bindings) | 2.1s | 2.1s (no pruning possible) |

Pruning doesn't help unbound queries — those must scan all shards by definition. But most application queries have bound subjects (entity pages, API responses, user-specific views), and those benefit enormously.

---

## When Citus Is Worth It

- **100M+ triples** with sub-second query latency requirements.
- **High concurrent query load** where a single PostgreSQL instance's CPU is saturated.
- **Write throughput** exceeding what a single instance can handle (bulk loads across workers in parallel).

For graphs under 50 million triples, a single well-tuned PostgreSQL instance is usually sufficient. pg_ripple's HTAP storage, BRIN indexes, and integer-only joins keep single-instance performance competitive up to that scale.

Citus adds operational complexity (coordinator, workers, rebalancing, distributed DDL). Use it when single-instance PostgreSQL can't meet your requirements — not before.
