[← Back to Blog Index](README.md)

# Everything Is an Integer

## How dictionary encoding turns string-heavy RDF into integer-only joins

---

RDF is a data model built on strings. Every subject is an IRI — a string. Every predicate is an IRI — a string. Every object is either an IRI, a blank node identifier, or a literal — all strings. A typical triple looks like:

```
<http://example.org/person/alice> <http://xmlns.com/foaf/0.1/name> "Alice"^^<http://www.w3.org/2001/XMLSchema#string>
```

That's three strings, totaling ~140 bytes, for one fact: "Alice's name is Alice."

Now imagine joining two triple patterns — say, finding everyone's name and email. In a naive implementation, that's a string equality join on the subject column. Two 50-byte IRIs compared character by character, millions of times. The CPU hates this.

pg_ripple never does string comparisons in triple store queries. Every IRI, blank node, and literal is mapped to a `BIGINT` (8 bytes) before it's stored. All VP table columns — subject, object, graph — are integers. All joins are integer equality joins. Strings are decoded only when results are returned to the caller.

This is dictionary encoding, and it changes the performance characteristics of RDF storage fundamentally.

---

## The Dictionary

pg_ripple maintains a single dictionary table:

```sql
_pg_ripple.dictionary (
  id    BIGINT PRIMARY KEY,
  value TEXT NOT NULL,
  kind  SMALLINT NOT NULL  -- 0=IRI, 1=blank node, 2=literal
)
```

Every unique RDF term gets exactly one row. The `id` is derived from an XXH3-128 hash of the term's canonical form, folded to 64 bits. The hash function is deterministic: the same term always produces the same ID, across transactions, across sessions, across pg_ripple installations.

When you load triples:

1. Each term (subject, predicate, object) is canonicalized and hashed.
2. The hash is used as the dictionary ID.
3. An `INSERT ... ON CONFLICT DO NOTHING` ensures the dictionary entry exists.
4. The triple is stored as `(subject_id, object_id, graph_id)` in the appropriate VP table.

Step 3 is important: the batch upsert pattern means pg_ripple never needs to check if a term exists before inserting. It always inserts. If the term is already there, the conflict resolution skips it. This eliminates the SELECT-then-INSERT round trip that kills bulk load performance in most dictionary-encoded stores.

---

## Why Not Sequential IDs?

A simpler design would assign sequential IDs: the first term gets ID 1, the second gets ID 2, and so on. Many triplestores do this.

Sequential IDs have two problems:

1. **Coordination.** In a concurrent environment (multiple sessions loading triples simultaneously), you need a lock or a sequence to assign IDs without collisions. This becomes a bottleneck at high write rates.

2. **Non-determinism.** The same dataset loaded in a different order produces different IDs. This makes debugging harder, makes cross-instance comparisons impossible, and breaks any caching that relies on ID stability.

XXH3-128 hashing avoids both. The ID is derived from the term itself, so there's no coordination needed — two sessions can independently compute the same ID for the same term. And the mapping is deterministic: same term, same ID, always.

The trade-off is collision risk. XXH3-128 produces a 128-bit hash, folded to 64 bits. The probability of a collision in a dataset with 1 billion unique terms is approximately $2^{-34}$ — about 1 in 17 billion. For comparison, the probability of a silent data corruption in a typical enterprise SSD is higher. This is an acceptable engineering trade-off.

---

## What Integer-Only Joins Buy You

### Cache Efficiency

A `BIGINT` is 8 bytes. An average IRI is 50–80 bytes. A dictionary-encoded VP table with 10 million triples stores 10M × 24 bytes (three `BIGINT` columns) = 240 MB. The same data with raw strings would be 10M × 180 bytes ≈ 1.8 GB.

The 8× reduction in data size means:
- More rows fit in PostgreSQL's shared buffers.
- More index entries fit in a single page.
- Sequential scans are 8× faster in I/O.
- Sort-merge joins spill to disk 8× less often.

### Join Performance

Integer equality comparison is a single CPU instruction. String equality comparison requires iterating through bytes until a mismatch or end-of-string. For 50-byte strings, that's up to 50 iterations per comparison. A join that performs 10 million comparisons saves 500 million iterations by using integers.

In practice, the wall-clock difference for a three-way star-pattern join over 10 million triples is ~15ms with integers versus ~180ms with strings. That's a 12× difference from encoding alone, before considering index size and cache effects.

### Index Compactness

B-tree indexes on `BIGINT` columns are compact and shallow. A B-tree on a `TEXT` column with average key length 60 bytes has roughly 8× fewer keys per page, which means the tree is 2–3 levels deeper. Each additional level is a random I/O on a cache miss.

For the `(s, o)` index on a VP table with 5 million rows, the integer B-tree is 3 levels deep. The equivalent string B-tree would be 5 levels deep. That's two extra page reads per lookup.

---

## The Decode-Last Principle

Dictionary encoding creates an asymmetry: encoding (string → integer) is cheap and can be done in bulk, but decoding (integer → string) requires a lookup per term. If you decode too early, you pay the cost for rows that will be filtered out later.

pg_ripple follows the decode-last principle: all filtering, joining, aggregation, and sorting happens on integer IDs. Only the final result set is decoded back to strings.

Consider this SPARQL query:

```sparql
SELECT ?name WHERE {
  ?person rdf:type foaf:Person ;
          foaf:name ?name .
  FILTER(LANG(?name) = "en")
}
```

The translation pipeline:

1. Encode `rdf:type`, `foaf:Person`, `foaf:name` to their integer IDs.
2. Join `vp_{rdf:type}` and `vp_{foaf:name}` on subject ID.
3. Filter: the `LANG()` filter needs the actual literal, so it decodes only the `?name` column for candidate rows.
4. Return: decode the surviving `?name` values.

Step 3 is the only place where decoding happens before the final result. And it only decodes the one column that the filter needs, only for rows that survived the join. For a query that starts with 500,000 `foaf:Person` triples and produces 12,000 English-language names, decoding happens 12,000 times — not 500,000.

---

## Bulk Loading and the Dictionary Cache

During bulk loads (loading a large Turtle or N-Triples file), the dictionary insertion pattern is: many terms, most of them new, arriving in bursts.

pg_ripple optimizes this with two mechanisms:

1. **Batch upsert.** Terms are collected in memory and inserted in a single `INSERT ... ON CONFLICT DO NOTHING ... RETURNING id, value` statement. This minimizes round trips to the dictionary table and lets PostgreSQL's executor handle the deduplication efficiently.

2. **LRU cache in shared memory.** Recently encoded terms are cached in a shared-memory LRU cache (size controlled by `pg_ripple.dictionary_cache_size`). During a bulk load where the same IRIs (like `rdf:type` or common namespace prefixes) appear repeatedly, the cache avoids hitting the dictionary table for terms that were just inserted.

The combination means that a bulk load of 10 million triples typically requires ~500,000 dictionary inserts (since many terms repeat) with ~95% cache hit rate after the first pass.

---

## When Decoding Hurts

The one scenario where dictionary encoding is a net negative is small, simple queries where the majority of the cost is the final decode.

A query like `SELECT * FROM pg_ripple.sparql('DESCRIBE <http://example.org/alice>')` on a small dataset might spend 0.5ms on the VP table lookups and 2ms decoding 30 terms from the dictionary. The encoding overhead dominates.

For these queries, pg_ripple's inline encoding optimization (introduced in v0.5.1) short-circuits the decode step for small result sets by batching the dictionary lookups into a single `IN (...)` query.

For any query that touches more than a few hundred triples — which is the common case for SPARQL analytics — the integer-join savings vastly outweigh the decode cost.
