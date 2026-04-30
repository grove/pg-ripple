# Bidi Operations Runbook

> v0.78.0 — BIDIOPS-DOC-01

This runbook covers day-two operations for the bidirectional integration (bidi) subsystem introduced in v0.77.0 and v0.78.0. It assumes the reader has completed the [pg-trickle relay guide](./pg-trickle-relay.md).

---

## Queue draining procedure

When a relay goes down or pg-trickle delivery stalls:

1. **Check the queue depth** for each subscription:

   ```sql
   SELECT subscription_name, outbox_depth, outbox_oldest_age,
          dead_letter_count, pg_trickle_paused
   FROM pg_ripple.bidi_status();
   ```

2. **Pause delivery** (delegated to pg-trickle) if you need a maintenance window:

   ```sql
   SELECT pg_trickle.pause_subscription('<outbox_table_name>');
   ```

   `bidi_status()` will show `pg_trickle_paused = true` while paused. New outbox events continue to accumulate.

3. **Resolve the relay issue**, then resume:

   ```sql
   SELECT pg_trickle.resume_subscription('<outbox_table_name>');
   ```

4. **Monitor drain progress** until `outbox_depth` returns to 0:

   ```sql
   SELECT subscription_name, outbox_depth FROM pg_ripple.bidi_status();
   ```

5. **Inspect dead-letter events** if any accumulated during the pause:

   ```sql
   SELECT * FROM pg_ripple.list_dead_letters('<subscription_name>');
   ```

---

## Token rotation procedure

Per-subscription bearer tokens should be rotated on a schedule (e.g., every 90 days) or immediately after a suspected breach.

1. **Register the new token** while the old one is still active:

   ```sql
   SELECT pg_ripple.register_subscription_token(
       '<subscription_name>',
       ARRAY['linkback','divergence','abandon'],
       'crm-relay-2026-Q3'
   );
   ```

   Store the returned raw token securely (it is shown only once).

2. **Distribute the new token** to the relay(s). Allow a brief overlap period (≥ token cache TTL, 60 seconds).

3. **Revoke the old token** once `last_used_at` confirms relay traffic has switched:

   ```sql
   SELECT pg_ripple.revoke_subscription_token(<old_token_hash_bytea>);
   ```

4. **Verify** only the new token is active:

   ```sql
   SELECT label, scopes, last_used_at, revoked_at
   FROM pg_ripple.list_subscription_tokens('<subscription_name>');
   ```

---

## Redaction policy guidance

- Mark PII and secret-bearing predicates with `"@redact": true` in the subscription's JSON-LD frame.
- Redacted predicates render as `{"@redacted": true}` in the standard outbox.
- For compliance pipelines that need cleartext, configure a separate subscription with an unredacted outbox table and grant access only to the elevated relay's credentials (standard PostgreSQL GRANTs).
- The frame is the **only** source of redaction truth — there is no additional allow-list.
- See [Redaction pattern](#pattern-redaction-with-elevated-subscription) below.

---

## Schema-evolution rollout playbook

When you need to change the subscription frame, IRI template, or exclude list:

1. **Frame change** (add/remove predicates):

   ```sql
   SELECT pg_ripple.alter_subscription(
       '<name>',
       frame_change_policy => 'new_events_only'
   );
   -- Then update the subscription's frame column separately.
   ```

   Queued outbox rows drain with the old frame; new events use the updated frame.

2. **IRI template or match-pattern change**:

   ```sql
   SELECT pg_ripple.alter_subscription(
       '<name>',
       iri_change_policy => 'new_events_only'
   );
   ```

   Already-rendered outbox IRIs are not retroactively rewritten.  
   For broken templates, pause pg-trickle, drop/requeue affected rows if needed, and record the action in `_pg_ripple.subscription_schema_changes`.

3. **Exclude-graphs change**:

   ```sql
   SELECT pg_ripple.alter_subscription(
       '<name>',
       exclude_change_policy => 'new_events_only'
   );
   ```

   All changes are recorded automatically in `_pg_ripple.subscription_schema_changes`:

   ```sql
   SELECT * FROM _pg_ripple.subscription_schema_changes
   WHERE subscription_name = '<name>'
   ORDER BY changed_at DESC;
   ```

---

## Reconciliation playbook

When a relay reports CAS divergence (actual values differ from base):

1. **Enqueue the divergence** (the relay calls this automatically):

   ```sql
   SELECT pg_ripple.reconciliation_enqueue(
       '<event_id>'::uuid,
       '{"ex:phone": {"actual": "+1-555-0100", "base": "+1-555-0200", "after": "+1-555-0300"}}'::jsonb
   );
   ```

2. **Pull the next item** for review:

   ```sql
   SELECT * FROM pg_ripple.reconciliation_next('<subscription_name>');
   ```

3. **Choose a resolution action**:

   | Action | When to use |
   |---|---|
   | `accept_external` | External system is the authority; ingest its actual values |
   | `force_internal` | Hub is the authority; re-emit event unconditionally |
   | `merge_via_owl_sameAs` | Divergence reveals a duplicate subject; assert `owl:sameAs` |
   | `dead_letter` | Cannot resolve now; move to dead-letter for later review |

   ```sql
   SELECT pg_ripple.reconciliation_resolve(
       <reconciliation_id>,
       'accept_external',
       'Confirmed with CRM team: external value is correct'
   );
   ```

4. **Monitor open items**:

   ```sql
   SELECT subscription_name, reconciliation_open
   FROM pg_ripple.bidi_status()
   WHERE reconciliation_open > 0;
   ```

---

## Chaos-test interpretation

The bidi chaos test (`tests/stress/bidi_chaos.sh`) runs the following smoke scenarios:

1. **abandon_linkback idempotency** — calling twice should not error.
2. **purge_event_audit zero-row safety** — no error when table is empty.
3. **reconciliation enqueue/resolve cycle** — end-to-end round-trip.
4. **bidi_health valid status** — always returns one of: `healthy|degraded|paused|failing`.
5. **token registration/revocation** — register, then revoke.

If any test fails, the script exits with a non-zero code and identifies the failing case.

---

## Pattern: per-subscription auth

Four-token deployment example (two relays × two scope sets):

```sql
-- CRM relay: full bidi scopes.
SELECT pg_ripple.register_subscription_token(
    'crm_relay',
    ARRAY['linkback','divergence','abandon','outbox_read'],
    'crm-relay-prod'
);

-- ERP relay: outbox-read only.
SELECT pg_ripple.register_subscription_token(
    'erp_relay',
    ARRAY['outbox_read'],
    'erp-relay-prod'
);

-- Operator admin token: registered in _pg_ripple.admin_tokens (not shown here).
```

HTTP endpoints enforce scope checks: `POST /subscriptions/crm_relay/events/{id}/linkback` requires `linkback` scope for the `crm_relay` subscription.

---

## Pattern: redaction with elevated subscription {#pattern-redaction-with-elevated-subscription}

Standard relay (redacted, normal consumers):

```json
{
  "@context": { "ex": "https://example.com/ns#" },
  "@type": "ex:Contact",
  "ex:name": {},
  "ex:phone": { "@redact": true },
  "ex:email": {},
  "ex:taxId": { "@redact": true }
}
```

Compliance relay (unredacted, elevated access):

- Register a separate subscription `crm_relay_audit` pointing to `crm_relay_outbox_unredacted`.
- The frame omits `"@redact": true` for all predicates.
- Grant `SELECT` on `crm_relay_outbox_unredacted` only to the compliance relay's database role.
- Register a separate token with `outbox_read` scope for `crm_relay_audit`.

`event_audit` records `action = 'emit_unredacted'` for bridge-writer rows in the unredacted outbox when optional emit auditing is enabled (controlled by `pg_ripple.audit_log_enabled`).
