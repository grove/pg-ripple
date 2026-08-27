# JSON Mapping

pg_ripple's **JSON mapping** feature provides a bidirectional bridge between JSON
payloads and the RDF knowledge graph.  Register a named JSON-LD context once with
`register_json_mapping()`, then use it for both ingest and export.

## Registration

```sql
SELECT pg_ripple.register_json_mapping(
    'contacts',
    '{"contact_id": "http://schema.org/identifier",
      "full_name":  "http://schema.org/name",
      "email_addr": "http://schema.org/email"}'::jsonb
);
```

Parameters:

| Parameter | Description |
|---|---|
| `name` | Unique mapping name |
| `context` | JSON-LD `@context` object mapping JSON keys to RDF predicate IRIs |
| `shape_iri` | Optional SHACL shape for consistency validation |
| `default_graph_iri` | Default named graph for ingested triples |
| `timestamp_path` | JSONPath to root timestamp field (diff mode) |
| `timestamp_predicate` | RDF predicate for per-triple change timestamps |
| `iri_template` | IRI template with `{id}` placeholder |
| `iri_match_pattern` | Prefix or regex for late-binding IRI rewrite |

## Ingest (JSON → RDF)

```sql
SELECT pg_ripple.ingest_json(
    '{"contact_id": "c001", "full_name": "Alice Smith"}'::jsonb,
    'https://example.com/contacts/c001',
    'contacts'
);
```

Modes: `'append'` (default), `'upsert'`, `'diff'`.

## Export (RDF → JSON)

```sql
SELECT pg_ripple.export_json_node(subject_id, 'contacts');
```

## Relational Writeback (v0.128.0)

v0.128.0 adds the ability to write RDF graph changes back to the originating
relational table — completing the full round-trip.

### Configuration

Add writeback configuration to the mapping:

```sql
UPDATE _pg_ripple.json_mappings
SET writeback_table        = 'contacts',
    writeback_schema       = 'public',
    writeback_key_columns  = ARRAY['contact_id'],
    writeback_conflict_policy = 'replace'
WHERE name = 'contacts';
```

### Conflict Policies

| Policy | Behaviour |
|---|---|
| `'replace'` (default) | `ON CONFLICT (key_cols) DO UPDATE SET …` |
| `'skip'` | `ON CONFLICT DO NOTHING` — returns 0 rows |
| `'error'` | Raises `PT0551` on conflict |

### Direct Writeback

```sql
-- Write a single subject back to the relational table.
SELECT pg_ripple.writeback_json_row('contacts', 'https://example.com/contacts/c001');

-- Delete a subject from the relational table.
SELECT pg_ripple.writeback_json_row_delete('contacts', 'https://example.com/contacts/c001');
```

### Trigger-Based Automation

Enable VP delta triggers for automatic async writeback:

```sql
SELECT pg_ripple.enable_json_writeback('contacts');
```

The background merge worker drains the queue in batches (controlled by
`pg_ripple.json_writeback_batch_size`, default 100).

Monitor queue status:

```sql
SELECT * FROM pg_ripple.json_writeback_status();
--  mapping_name | pending | errors | last_error | last_processed_at
```

### HTTP Writeback

The HTTP companion exposes the same writeback path for applications that do not
call SQL directly:

```bash
curl -X POST http://localhost:7878/json-mapping/contacts/writeback \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"subject_iri":"https://example.com/contacts/c001"}'
```

Successful synchronous writeback returns:

```json
{"rows_affected": 1}
```

Queue status for one mapping is available over HTTP as well:

```bash
curl http://localhost:7878/json-mapping/contacts/writeback/status \
    -H "Authorization: Bearer $TOKEN"
```

The status response mirrors `json_writeback_status()` for the selected mapping:

```json
{
    "mapping_name": "contacts",
    "pending": 0,
    "errors": 0,
    "last_error": null,
    "last_processed_at": null
}
```

Disable triggers:

```sql
SELECT pg_ripple.disable_json_writeback('contacts');
```

Both `enable_json_writeback()` and `disable_json_writeback()` are idempotent.

### Error Codes

| Code | Message |
|---|---|
| `PT0550` | `json mapping writeback target not configured` — `writeback_table` is NULL or `writeback_key_columns` is empty |
| `PT0551` | `json mapping writeback conflict` — conflict detected with policy `'error'` |
| `PT0552` | `json mapping writeback key column(s) have no asserted value` — a configured `writeback_key_columns` predicate was never asserted for this subject |

## See Also

- [JSON-LD Reverse Mapping blog post](https://github.com/trickle-labs/pg-ripple/blob/main/blog/json-ld-reverse-mapping.md)
- [JSON Writeback, and What It Took to Make It Actually Work](https://github.com/trickle-labs/pg-ripple/blob/main/blog/json-writeback-correctness.md)
- [GUC reference: json_writeback_batch_size](../reference/guc-reference.md#pg_ripplejson_writeback_batch_size)
- [HTTP API Reference](../reference/http-api.md#json-mapping-writeback)
- [R2RML for complex ETL](r2rml.md)
