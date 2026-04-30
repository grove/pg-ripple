# RDF Bidirectional Integration Profile, v1 (draft)

> **Status**: Draft, vendor-neutral. Editor: pg_ripple project. Reference implementation: [pg_ripple](https://github.com/grove/pg-ripple) v0.77.0+.
>
> This document specifies a wire-level and behavioural profile that any RDF triplestore can implement to participate in **bidirectional integration** with external systems (CRMs, ERPs, SaaS APIs, other RDF stores) without per-relay reconciliation glue. The profile is grounded in W3C standards (RDF 1.1, RDF-star, OWL 2 RL, SHACL, SPARQL 1.1, JSON-LD 1.1, PROV-O) and adds nothing not derivable from them; its contribution is the *combination* and the *protocol*.
>
> "Bidirectional" means the same store can be both a source (emitting CDC-style change events) and a sink (ingesting change events), with conflicts, echoes, target-assigned identifiers, and cross-source equivalences handled by the protocol rather than by ad-hoc relay code.

## 1. Motivation

Every RDF triplestore today exposes some form of change-data-capture (Jena listeners, Stardog cluster events, Oxigraph diffs, GraphDB notifications, pg_ripple subscriptions). Each is bespoke. None addresses the harder questions that arise once a triplestore sits inside an integration topology:

1. **Source attribution**: which external system contributed which triple, and at what timestamp from *that system's* clock?
2. **Echo suppression**: when a write originating in system A is mirrored to system B and then mirrored back, how do we suppress the echo without losing legitimate concurrent edits from B?
3. **Conflict resolution**: when two sources disagree on a `sh:maxCount 1` predicate, which wins, and where do the losers go?
4. **Cross-source identity**: when system A and system B refer to the same real-world entity under different IRIs, how do we route writes correctly without a global IRI registry that has to be updated retroactively?
5. **Target-assigned identifiers**: when the integration target (e.g. an ERP) assigns the canonical ID *after* accepting a create, how do we tie the resulting IRI back to the originating event?
6. **Optimistic concurrency**: how does a relay safely apply an event without overwriting a concurrent edit it doesn't know about?
7. **Schema evolution**: how do mid-flight changes to the projected shape interact with queued events?
8. **Operational invariants**: queue limits, dead-letter, audit, RBAC, redaction, reconciliation.

Existing CDC mechanisms answer (1) implicitly at best and (2)–(8) not at all. The Bidi Profile answers all eight in a way every triplestore can implement.

## 2. Conformance levels

A triplestore claims **Bidi-1.0 compliance** if it implements §3 (Data Model), §4 (Wire Format), §5 (Receiver Protocol), §6 (Reconciliation), and the §11 conformance test corpus.

A triplestore claims **Bidi-1.0-Ops compliance** if it additionally implements §7 (Schema Evolution), §8 (Auth & Redaction), §9 (Audit), and §10 (Operations Surface).

A triplestore MAY implement only Bidi-1.0; it MUST NOT claim Bidi-1.0-Ops without Bidi-1.0.

## 3. Data model

### 3.1 Sources are named graphs

Every triple ingested from an external system MUST be stored in a named graph whose IRI uniquely identifies the source. The recommended convention is `urn:source:<system>` (e.g. `urn:source:crm`, `urn:source:erp`); implementations MUST NOT impose a different convention.

The default graph (RDF default graph; "graph 0" in some implementations) is reserved for triples that are not source-attributed. Bidi-Profile-compliant ingest MUST NOT write to the default graph.

### 3.2 Per-source timestamps

Source timestamps are carried as RDF-star annotations on triples, using the predicate `prov:generatedAtTime` (PROV-O). For a triple `<<:s :p :o>>`, the timestamp is asserted as:

```turtle
<< :s :p :o >> prov:generatedAtTime "2026-04-22T14:00:00Z"^^xsd:dateTime .
```

Implementations without RDF-star MAY use reification but MUST document the mapping.

### 3.3 Resolved projection

Implementations MUST maintain a derived projection answering "what is the current consensus value for `(subject, predicate)` across all sources?" This projection is non-destructive: source graphs retain their original triples; only the projection reflects the resolution.

Four resolution policies are defined. An implementation MUST support all four.

| Policy | Definition |
|---|---|
| `source_priority [s1, s2, ...]` | The value from the highest-priority source that has a value wins. Lower-priority sources contribute only when higher-priority sources are silent. |
| `latest_wins` | The value with the latest `prov:generatedAtTime` wins. Ties broken by source priority. |
| `reject_on_conflict` | If multiple sources have differing values, NO value enters the projection; the conflict is recorded for operator review. |
| `union` | All values enter the projection (only valid for predicates without `sh:maxCount 1`). |

Policies MAY be configured per-predicate or globally. The unit of resolution is `(subject, predicate)`, not `(subject, predicate, object)`.

### 3.4 Echo-aware normalization

Conflict policies MAY be qualified with a `normalize` SPARQL expression. Two values are considered "the same for conflict purposes" iff their normalized forms are equal. Examples:

```
latest_wins normalize=`STRDT(STR(?v), xsd:string)`     # ignore datatype mismatches
latest_wins normalize=`UCASE(STR(?v))`                  # case-insensitive
latest_wins normalize=`REPLACE(STR(?v), "[\\s-]", "")`  # ignore whitespace and hyphens
```

When the source-A and source-B values normalize to the same form, no conflict is raised and no event is emitted (echo suppression). Precision is preserved: the underlying triples retain their original lexical form; only conflict detection compares normalized forms.

### 3.5 Cross-source equivalence

Cross-source identity uses `owl:sameAs`. The protocol does not require full OWL 2 reasoning; it requires only the symmetric-transitive closure of `owl:sameAs` over the union of source graphs.

Implementations MUST provide a query primitive equivalent to "give me the equivalence class of IRI `i`" with bounded latency.

## 4. Wire format

### 4.1 Event shape

Outbound events are JSON documents with the following top-level schema:

```json
{
  "version":           "1.0",
  "event_id":          "<uuid>",
  "event_type":        "INSERT" | "UPDATE" | "DELETE",
  "subject":           "<canonical-iri>",
  "subject_resolved":  "<rewritten-iri>",
  "graph":             "<source-graph-iri>",
  "timestamp":         "<RFC 3339 instant>",
  "@context":          { ... JSON-LD context ... },
  "after":             { ... JSON-LD framed object ... },
  "base":              { ... JSON-LD framed object ... }
}
```

Field semantics:

- **`version`**: the only top-level discriminator. Receivers MUST switch on this. Future versions MUST remain forward-compatible-by-default; receivers ignore unknown fields.
- **`event_id`**: globally unique. Used for ack/nack/linkback addressing.
- **`event_type`**: one of `INSERT`, `UPDATE`, `DELETE`. SHACL semantics (atomic delete-then-insert) collapse to `UPDATE`.
- **`subject`**: the canonical hub IRI of the changed entity.
- **`subject_resolved`**: the IRI as the receiver should see it (after late-binding rewrite under the receiver's `iri_pattern`). MAY equal `subject`.
- **`graph`**: the source graph IRI. The receiver uses this for echo detection (do not re-ingest events whose `graph` matches the receiver's own source graph).
- **`timestamp`**: the event emit time, RFC 3339.
- **`@context`**: JSON-LD context for `after` and `base`. SHOULD be a stable URL or a small inline context.
- **`after`**: the projected post-state of the entity, as a JSON-LD framed object. For `DELETE`, MUST be `null`.
- **`base`** (sparse-CAS): for `UPDATE`, the **sparse pre-state** of *only those predicates that changed*. For `INSERT`, MUST be `{}`. For `DELETE`, MUST be the full pre-deletion frame. See §5.2 for CAS semantics.

The schema URI for `version` is advertised via the HTTP `Link: rel="describedby"` header (§4.3). It MUST NOT be repeated inside every event body.

### 4.2 Framing symmetry rule

`after` and `base` MUST be framed under the *same* JSON-LD context and frame. Specifically:

- For every key present in both, the lexical representation of the value (string, number, list ordering, datatype tag presence) MUST be byte-identical when the underlying RDF values are equal.
- Implementations MUST NOT apply lossy normalization to one side and not the other.

This guarantees that receivers can perform CAS via byte-string equality (`actual[k] == base[k]`) without re-parsing.

### 4.3 HTTP transport

Implementations exposing events via HTTP MUST:

- Set `Content-Type: application/json` on event responses.
- Set `Link: <https://example.org/specs/bidi/v1/event.schema.json>; rel="describedby"` (with the implementation's actual schema URL).
- Use bearer-token auth via `Authorization: Bearer <token>`.
- Support per-route scope checks (§8.1).

Implementations MAY also offer a non-HTTP transport (Postgres SPI, gRPC, AMQP, Kafka). The wire format is identical regardless of transport.

### 4.4 Object-level grouping

One event MUST be emitted per `(subject, transaction)` pair. A transaction touching N subjects emits N events; a transaction touching one subject's K predicates emits one event with K populated keys in `after` and (for UPDATE) `base`.

Implementations MAY offer a triple-level grouping mode as a legacy escape hatch but it MUST be opt-in.

## 5. Receiver protocol

### 5.1 Pull, ack, nack

Receivers consume events via three primitives (transport-independent names):

- `next_event(subscription) → event | null` — returns the next unleased event, leases it for `lease_timeout` (default 30 s).
- `ack_event(event_id)` — confirms the event was applied. Removes it from the queue.
- `nack_event(event_id, reason)` — records the failure, releases the lease, leaves the event for retry.

Implementations MUST guarantee at-least-once delivery: an event whose lease expires without ack/nack MUST be redelivered.

### 5.2 Sparse-CAS application

For `UPDATE` events, the receiver applies the following algorithm:

```
for each (predicate, base_value) in event.base:
    actual_value = read_actual(target_system, event.subject_resolved, predicate)
    if actual_value == base_value:
        continue  # CAS holds for this predicate
    elif actual_value == event.after[predicate]:
        continue  # already applied; idempotent
    else:
        escalate(event, predicate, actual_value, base_value, event.after[predicate])
        return

apply_writes(target_system, event.subject_resolved, event.after)
ack_event(event.event_id)
```

`escalate` MUST invoke the reconciliation toolkit (§6) rather than silently overwriting.

For `INSERT` events on a target system that assigns its own primary key, see §5.3.

For `DELETE` events, the receiver SHOULD verify `actual == base` for at least one identifying predicate before deleting; on mismatch, escalate.

### 5.3 Linkback for target-assigned identifiers

When the target system assigns the canonical ID for a fresh INSERT (e.g. an ERP returns `4011` after accepting a create), the receiver:

1. Performs the insert in the target system, captures the assigned ID.
2. Calls `record_linkback(event_id, target_id)` on the source store.

The source store:

1. Expands `target_id` through the receiving graph's `iri_pattern` to produce the target IRI.
2. Atomically writes `owl:sameAs` between the original `subject` and the expanded target IRI.
3. Flushes any subscription-buffered subsequent events for that subject (§5.4).

Implementations MUST also accept a `target_iri` form for cases where the target system returns a canonical URL rather than a bare ID.

`ack_event` for an unresolved INSERT (one without a corresponding `record_linkback` or `target_id` field on the event) MUST fail. This prevents losing the linkback.

### 5.4 Subscription buffering during pending linkback

While a linkback is pending for `(subject, subscription)`, subsequent events for the same `(subject, subscription)` MUST be persisted in unrendered form and flushed atomically when `record_linkback` lands. Implementations MUST NOT emit two events for an entity whose target ID is not yet known.

If the linkback never lands (operator abandonment, target system failure), buffered events MUST be expired after `linkback_timeout` (default 1 h) with operator notification.

### 5.5 Late-binding IRI rewrite

When emitting an event for subject `s` to a subscription whose `target_graph` is `g`:

1. Compute the equivalence class `E = closure_owl_sameAs(s)`.
2. If any member of `E` matches the `iri_pattern` for `g`, that member is `subject_resolved`.
3. Otherwise, `subject_resolved = s` (the canonical hub IRI passes through unchanged).

This rewrite is **late-binding**: it happens at emit time, not at write time. Closure changes (new `owl:sameAs` discovered) MUST NOT retroactively rewrite already-queued events.

### 5.6 Loop prevention

Subscriptions MUST support an `exclude_graphs` parameter. Events whose `graph` field appears in the excluded list MUST be filtered out.

Subscriptions MUST support a `propagation_depth` parameter (default `1`). A change to a triple in graph G triggers events for G plus equivalent triples discoverable through up to `propagation_depth` `owl:sameAs` hops, but no further. This bounds fan-out in densely linked equivalence classes.

## 6. Reconciliation

When sparse-CAS fails (§5.2 `escalate`), the receiver MUST enqueue a reconciliation entry:

```
reconciliation_enqueue(
    event_id,
    divergence_summary = {
        predicate_iri: { actual: ..., base: ..., after: ... },
        ...
    }
)
```

Implementations MUST provide an operator-facing primitive `reconciliation_next(subscription)` that pulls the next unresolved entry under a lease, mirroring `next_event` semantics.

Implementations MUST support four resolution actions:

| Action | Semantics |
|---|---|
| `accept_external` | Ingest the external system's actual value into the corresponding source graph as if it had arrived via normal ingest. The original event is acked as a no-op. |
| `force_internal` | Re-emit the same event marked as a force overwrite. The receiver applies it ignoring `base`. |
| `merge_via_owl_sameAs` | Assert `owl:sameAs` between the divergent values' subjects (used when divergence reveals a duplicate). Ack the original event. |
| `dead_letter` | Move the event to the dead-letter store with the divergence summary attached. |

Implementations MAY offer additional actions but MUST NOT redefine these four.

## 7. Schema evolution

Subscriptions evolve over time: frames change, IRI patterns change, exclude lists change. The protocol defines explicit policies for each.

### 7.1 Frame changes

When a subscription's frame is altered, queued events are affected per `frame_change_policy`:

- `reframe_queued` (default): queued events render under the new frame at next-pull time. Currently-leased events finish under the old frame.
- `drain_then_switch`: the subscription is implicitly paused; new emits go to a parallel queue under the new frame; once the old queue drains, the parallel queue takes over.

### 7.2 IRI pattern changes

When a graph's `iri_pattern` is altered, queued events are affected per `pattern_change_policy`:

- `no_retroactive` (default): queued events keep the original pattern's IRIs; new events use the new pattern.
- `reframe_queued`: queued events are re-rewritten under the new pattern at next-pull time.

### 7.3 Exclude-graphs changes

When `exclude_graphs` is altered, queued events are affected per `exclude_change_policy`:

- `apply_to_new` (default): the new exclude list applies only to events emitted after the change. Queued events drain unchanged.
- `apply_to_queued`: re-apply the new exclude list to queued events; filtered-out events move to dead-letter.

All schema changes MUST be recorded in an audit table with the old value, new value, applied policy, and affected event count.

## 8. Auth and redaction

### 8.1 Per-subscription scope tokens

Implementations MUST support bearer tokens scoped to a single subscription with a subset of the following permissions:

| Scope | Permits |
|---|---|
| `pull` | `next_event` |
| `ack` | `ack_event` |
| `nack` | `nack_event` |
| `linkback` | `record_linkback` |
| `unredacted` | Receive cleartext for redacted predicates (§8.2) |

Tokens MUST be SHA-256 hashed at rest. The raw token MUST be returned only at registration time.

A token registered for subscription A MUST NOT permit operations on subscription B, even with matching scopes.

### 8.2 Frame-level redaction

Frames MAY mark predicates with `"@redact": true`:

```json
{
  "@context": { "ex": "https://example.com/ns#" },
  "@type":    "ex:Contact",
  "ex:name":  {},
  "ex:phone": { "@redact": true },
  "ex:taxId": { "@redact": true }
}
```

For non-`unredacted`-scoped consumers, redacted predicates MUST be emitted in `after` and `base` as the literal object `{"@redacted": true}`. CAS works because both sides redact identically.

For `unredacted`-scoped consumers, the cleartext value MUST be emitted. The same queued event MUST render differently per consumer based on the requesting token's scopes; implementations MUST NOT require re-emit to elevate.

## 9. Audit

Implementations MUST record every transport call (`next_event`, `ack_event`, `nack_event`, `record_linkback`, dead-letter requeue/drop, reconciliation resolve) with at minimum:

| Field | Required |
|---|---|
| `event_id` (or `reconciliation_id`) | Yes |
| `subscription_name` | Yes |
| `action` | Yes |
| `actor_token_hash` | Yes when called via authenticated transport |
| `actor_session` | Yes when called via direct query interface |
| `remote_addr` | Yes when called via HTTP |
| `observed_at` | Yes |

Audit records MUST distinguish `pull` from `pull_unredacted`.

Implementations MUST provide a configurable retention window with a default of 90 days.

## 10. Operations surface

Implementations MUST expose a per-subscription status view with at minimum:

- queue depth
- oldest-event age
- dead-letter count
- conflict-rejection rate (rolling window)
- pending-linkback count and oldest-pending age
- last-pull and last-ack timestamps
- reconciliation-open count

Implementations MUST expose a single overall health status with values `healthy` / `degraded` / `paused` / `failing` and a list of triggering conditions. The mapping rules are at the implementation's discretion but MUST be documented.

Implementations exposing HTTP MUST return `503` from a health endpoint when status is `failing`.

## 11. Conformance test corpus

A reference test corpus accompanies this specification at `tests/bidi-conformance/` (TBD; pg_ripple ships an interim version under `tests/fixtures/bidi/`). The corpus consists of black-box scenarios:

| Scenario | Asserts |
|---|---|
| `single_source_insert` | Ingest one triple under `urn:source:s1`; verify it appears in the resolved projection and emits one INSERT event. |
| `two_source_latest_wins` | Ingest conflicting `sh:maxCount 1` values from `s1` and `s2`; verify the latest timestamp wins; verify one UPDATE event with sparse `base`. |
| `echo_suppression` | Ingest from `s1`, mirror to `s2`, mirror back; verify no second event under the `normalize` rule. |
| `late_binding_rewrite` | Ingest `s1` reference to `s2:foo` before `owl:sameAs(s2:foo, s1:bar)` is asserted; verify the next event for `s1:bar` carries `subject_resolved = s2:foo`; verify *queued* events do NOT retroactively rewrite. |
| `linkback_round_trip` | Emit INSERT to a subscription with target-assigned IDs; record linkback with bare ID; verify `owl:sameAs` written; verify subsequent UPDATE for the same subject carries the rewritten IRI. |
| `subscription_buffer_flush` | Same as linkback_round_trip but with a second event emitted before the linkback lands; verify the second event is buffered and flushes atomically when the linkback lands. |
| `cas_divergence_escalates` | Receiver applies UPDATE; meanwhile target system has changed `actual` independently; verify CAS fails and reconciliation enqueues. |
| `four_resolutions` | For each of the four resolution actions, drive the reconciliation to that resolution and assert the resulting state. |
| `frame_change_reframe_queued` | Alter frame mid-flight; verify queued events render under the new frame. |
| `frame_change_drain_then_switch` | Alter frame mid-flight under `drain_then_switch`; verify subscription pauses, drains under v1, resumes under v2. |
| `redacted_predicate` | Frame predicate with `@redact`; pull with non-elevated token; verify `{"@redacted": true}`. Pull same event with `unredacted` token; verify cleartext. |
| `scope_isolation` | Token for subscription A used against subscription B; verify rejection. |
| `dead_letter_overflow` | Push events past `max_queue_depth` under each of the three overflow policies; verify expected dead-letter contents. |
| `audit_completeness` | After a round of pull/ack/nack/linkback/resolve, verify each call has exactly one audit record. |
| `convergence_random` | Apply 1000 random insert/update/delete operations from 4 sources; verify the resolved projection is independent of operation arrival order under `latest_wins`. |

Implementations claiming Bidi-1.0 MUST pass scenarios 1–8. Implementations claiming Bidi-1.0-Ops MUST additionally pass 9–15.

## 12. Non-normative reference implementation

[pg_ripple](https://github.com/grove/pg-ripple) v0.77.0+ is the reference implementation. The mapping from this specification's vocabulary to pg_ripple-specific surface is documented at [docs/src/operations/pg-trickle-relay.md](../src/operations/pg-trickle-relay.md). pg_ripple-specific concerns out of scope for this specification:

- Storage layout (HTAP delta/main partitioning).
- Queue substrate (Postgres tables with `SKIP LOCKED` leases).
- HTTP companion service.
- Migration mechanism.
- Prometheus metric naming.

Other triplestores implementing this profile are free to adopt different substrates as long as the protocol observable from the wire matches §4–§10.

## 13. Versioning

This specification is versioned via the top-level `version` field on events. Backwards-incompatible changes require a major version bump; receivers MAY refuse unknown major versions. Backwards-compatible additions require a minor version bump and MUST be ignorable by receivers that do not understand them.

Schema URLs follow the pattern `https://<editor>/specs/bidi/v<MAJOR>/event.schema.json` and MUST be advertised via HTTP `Link: rel="describedby"` (§4.3).

## 14. Security considerations

- Tokens MUST be hashed at rest. Implementations storing raw tokens are non-conformant.
- Per-subscription scope checks are mandatory; a global "admin" token is permitted for operator endpoints but MUST NOT be accepted for per-subscription transport calls.
- Frame-level redaction is a defense-in-depth measure, not a substitute for transport encryption. HTTPS is REQUIRED for the HTTP transport.
- Audit logs SHOULD be append-only or backed by an append-only store; implementations permitting deletion MUST log the deletion.
- The `force_internal` reconciliation action SHOULD require elevated authorization.

## 15. Recommended Qualifier Vocabulary (non-normative)

Implementations using RDF-star annotations (§3.2) to carry per-statement context are encouraged to follow this vocabulary. These predicates are recommended, not mandatory; implementations MAY use alternatives and MUST document their choices.

### Temporal

| Predicate | Purpose | Example |
|---|---|---|
| `prov:generatedAtTime` | Timestamp from the source system | `<< :s :p :o >> prov:generatedAtTime "2026-04-22T14:00:00Z"^^xsd:dateTime .` |
| `dcterms:valid` | Validity interval (start, end, or point in time) | `<< :s :contract :active "true"^^xsd:boolean >> dcterms:valid "[2026-01-01T00:00:00Z, 2026-12-31T23:59:59Z]"^^xsd:string .` |
| `prov:startedAtTime` | When a relationship/state began | `<< :contact :employedAt :company >> prov:startedAtTime "2020-01-15T00:00:00Z"^^xsd:dateTime .` |
| `prov:endedAtTime` | When a relationship/state ended | `<< :contact :employedAt :company >> prov:endedAtTime "2024-03-31T00:00:00Z"^^xsd:dateTime .` |

### Source Provenance

| Predicate | Purpose | Example |
|---|---|---|
| `prov:wasGeneratedBy` | Link to the activity/actor that produced this value | `<< :s :p :o >> prov:wasGeneratedBy [ a prov:Activity ; prov:agent <urn:agent:salesforce-sync> ] .` |
| `dcterms:source` | Reference URL or identifier of the originating system | `<< :s :phone :value >> dcterms:source "https://salesforce.com/records/0011..." .` |

### Quality / Confidence

| Predicate | Purpose | Example |
|---|---|---|
| `prov:confidence` | Confidence score (0–1); useful for ML-derived enrichment | `<< :contact :likelyRole "VP Marketing" >> prov:confidence "0.87"^^xsd:decimal .` |
| `dcterms:issued` | Date the assertion was made (as opposed to effective date) | `<< :product :costPerUnit "123.45" >> dcterms:issued "2026-04-20"^^xsd:date .` |

### Integration-Specific

| Predicate | Purpose | Example |
|---|---|---|
| `bidi:sourceSystemId` (implementation-defined) | External system's internal ID for this assertion; enables round-trip linkback | `<< :s :p :o >> bidi:sourceSystemId "SF-0011-PHONE-2026" .` |
| `bidi:integrationTimestamp` (implementation-defined) | When the assertion was ingested into the integration layer | `<< :s :p :o >> bidi:integrationTimestamp "2026-04-22T14:15:33Z"^^xsd:dateTime .` |

**Usage note**: avoid creating new predicates for each integrator. Use these standard ones where they fit; for implementation-specific metadata, adopt a consistent URI namespace (e.g. `https://yourdomain.com/integration/v1#`) and document it in your mapping definitions.

## 16. References

- [RDF 1.1 Concepts and Abstract Syntax](https://www.w3.org/TR/rdf11-concepts/) (W3C Recommendation)
- [RDF-star and SPARQL-star](https://w3c.github.io/rdf-star/) (W3C Working Draft)
- [SPARQL 1.1 Query Language](https://www.w3.org/TR/sparql11-query/) (W3C Recommendation)
- [SHACL](https://www.w3.org/TR/shacl/) (W3C Recommendation)
- [OWL 2 RL Profile](https://www.w3.org/TR/owl2-profiles/#OWL_2_RL) (W3C Recommendation)
- [JSON-LD 1.1](https://www.w3.org/TR/json-ld11/) (W3C Recommendation)
- [JSON-LD 1.1 Framing](https://www.w3.org/TR/json-ld11-framing/) (W3C Recommendation)
- [PROV-O](https://www.w3.org/TR/prov-o/) (W3C Recommendation)
- [CloudEvents 1.0](https://github.com/cloudevents/spec) (CNCF)
- [pg_ripple v0.77.0 + v0.78.0 roadmap](https://github.com/grove/pg-ripple/blob/main/roadmap/v0.77.0-full.md) (reference implementation)
