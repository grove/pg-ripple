# GraphRAG Ontology Reference

The GraphRAG namespace `https://graphrag.org/ns/` (prefix `gr:`) defines the
vocabulary used to represent GraphRAG knowledge graphs in pg_ripple.

## Classes

### `gr:Entity`
A named entity extracted from source text.

**Required properties:** `gr:title`  
**Optional properties:** `gr:type`, `gr:description`, `gr:frequency`, `gr:degree`

### `gr:Relationship`
A directed relationship between two entities.

**Required properties:** `gr:source`, `gr:target`  
**Optional properties:** `gr:description`, `gr:weight`

### `gr:TextUnit`
A chunk of source text from which entities were extracted.

**Required properties:** `gr:text`  
**Optional properties:** `gr:tokenCount`, `gr:documentId`, `gr:mentionsEntity`

### `gr:Community`
A community of related entities (from community detection).

**Optional properties:** `gr:title`, `gr:description`, `gr:rank`, `gr:level`

### `gr:CommunityReport`
A natural-language summary report for a community.

**Optional properties:** `gr:summary`, `gr:findings`, `gr:rating`

## Properties

### Data properties

| Property | Domain | Range | Notes |
|---|---|---|---|
| `gr:title` | `gr:Entity`, `gr:Community` | `xsd:string` | Display label |
| `gr:type` | `gr:Entity` | `xsd:string` | E.g. `"PERSON"`, `"ORGANIZATION"` |
| `gr:description` | any | `xsd:string` | Free-text description |
| `gr:text` | `gr:TextUnit` | `xsd:string` | Raw source text |
| `gr:tokenCount` | `gr:TextUnit` | `xsd:integer` | Token count |
| `gr:frequency` | `gr:Entity` | `xsd:integer` | Mention frequency |
| `gr:degree` | `gr:Entity` | `xsd:integer` | Graph degree (# relationships) |
| `gr:weight` | `gr:Relationship` | `xsd:float` | Relationship strength 0–1 |
| `gr:rank` | `gr:Community` | `xsd:integer` | Community rank |
| `gr:level` | `gr:Community` | `xsd:integer` | Hierarchical level |
| `gr:summary` | `gr:CommunityReport` | `xsd:string` | Summary text |
| `gr:rating` | `gr:CommunityReport` | `xsd:float` | Impact rating |

### Object properties

| Property | Domain | Range | Notes |
|---|---|---|---|
| `gr:source` | `gr:Relationship` | `gr:Entity` | Relationship source entity |
| `gr:target` | `gr:Relationship` | `gr:Entity` | Relationship target entity |
| `gr:mentionsEntity` | `gr:TextUnit` | `gr:Entity` | Entity mentioned in text unit |
| `gr:documentId` | `gr:TextUnit` | `gr:Document` | Source document |
| `gr:hasReport` | `gr:Community` | `gr:CommunityReport` | Community report |

### Derived properties (Datalog)

These properties are not loaded directly but derived by the enrichment rule set:

| Property | Semantics |
|---|---|
| `gr:coworker` | Symmetric — share a common relationship target |
| `gr:collaborates` | Symmetric — both mentioned in the same text unit |
| `gr:indirectReport` | Transitive closure of `gr:manages` |
| `gr:relatedOrg` | Organizations bridged by a shared entity |

## Ontology file

The full OWL ontology is shipped as `sql/graphrag_ontology.ttl`.  Load it with:

```sql
SELECT pg_ripple.load_turtle(pg_read_file('/path/to/graphrag_ontology.ttl'));
```

## SHACL shapes

Validation shapes are in `sql/graphrag_shapes.ttl`.  Load with:

```sql
SELECT pg_ripple.load_shacl(pg_read_file('/path/to/graphrag_shapes.ttl'));
```
