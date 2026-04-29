# GraphRAG Export Functions

These functions export GraphRAG knowledge-graph data from pg_ripple to Parquet files
compatible with the Microsoft GraphRAG pipeline.

All functions require **superuser** and return the number of rows written.

---

## `export_graphrag_entities`

```sql
pg_ripple.export_graphrag_entities(
    graph_iri  TEXT,   -- named graph IRI, or '' for the default graph
    output_path TEXT   -- absolute path to the output .parquet file
) RETURNS BIGINT
```

Exports all `gr:Entity` instances.

**Output columns:**

| Column | Type | Description |
|---|---|---|
| `id` | `BYTE_ARRAY` | Entity IRI |
| `title` | `BYTE_ARRAY` | `gr:title` value |
| `type` | `BYTE_ARRAY` | `gr:type` value |
| `description` | `BYTE_ARRAY` | `gr:description` value |
| `text_unit_ids` | `BYTE_ARRAY` | JSON array placeholder (always `"[]"`) |
| `frequency` | `INT64` | `gr:frequency` value, default 0 |
| `degree` | `INT64` | `gr:degree` value, default 0 |

**Example:**

```sql
SELECT pg_ripple.export_graphrag_entities(
    'https://myapp.org/graphs/graphrag',
    '/var/data/entities.parquet'
);
```

---

## `export_graphrag_relationships`

```sql
pg_ripple.export_graphrag_relationships(
    graph_iri  TEXT,
    output_path TEXT
) RETURNS BIGINT
```

Exports all `gr:Relationship` instances.

**Output columns:**

| Column | Type | Description |
|---|---|---|
| `id` | `BYTE_ARRAY` | Relationship IRI |
| `source` | `BYTE_ARRAY` | `gr:source` entity IRI |
| `target` | `BYTE_ARRAY` | `gr:target` entity IRI |
| `description` | `BYTE_ARRAY` | `gr:description` value |
| `weight` | `DOUBLE` | `gr:weight` value, default 0.0 |
| `combined_degree` | `INT64` | Placeholder, always 0 |
| `text_unit_ids` | `BYTE_ARRAY` | JSON array placeholder |

---

## `export_graphrag_text_units`

```sql
pg_ripple.export_graphrag_text_units(
    graph_iri  TEXT,
    output_path TEXT
) RETURNS BIGINT
```

Exports all `gr:TextUnit` instances.

**Output columns:**

| Column | Type | Description |
|---|---|---|
| `id` | `BYTE_ARRAY` | Text unit IRI |
| `text` | `BYTE_ARRAY` | `gr:text` value |
| `n_tokens` | `INT64` | `gr:tokenCount` value, default 0 |
| `document_id` | `BYTE_ARRAY` | `gr:documentId` value |
| `entity_ids` | `BYTE_ARRAY` | JSON array placeholder |
| `relationship_ids` | `BYTE_ARRAY` | JSON array placeholder |

---

## Notes

- **Graph IRI:** Pass `''` (empty string) to query the default graph (all triples without a named-graph assignment). Pass a full IRI to restrict to a named graph.
- **Path security:** The output path must not contain `..` components or null bytes. The directory must already exist.
- **Parquet encoding:** Uses Snappy compression. Columns are `REQUIRED BYTE_ARRAY` for mandatory fields and `OPTIONAL BYTE_ARRAY / INT64 / DOUBLE` for optional ones.
- **Superuser required:** Because the function writes to the filesystem.

## See also

- [GraphRAG User Guide](../features/graphrag.md)
- [GraphRAG Ontology](graphrag-ontology.md)
- [GraphRAG Enrichment](../features/graphrag.md)
