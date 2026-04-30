# Bidi Production Checklist

> v0.78.0 — BIDIOPS-DOC-01

Use this checklist before taking a bidirectional integration to production. Each item links to the relevant runbook section.

---

## 1. Extension version

- [ ] `SELECT pg_ripple.version()` returns `0.78.0` or later.
- [ ] `pg_ripple.control` `default_version` matches the installed version.

## 2. Schema completeness

Run this query — expect **7 rows** (one per required v0.78.0 table):

```sql
SELECT tablename FROM pg_tables
WHERE schemaname = '_pg_ripple'
  AND tablename IN (
      'event_dead_letters','subscription_schema_changes',
      'subscription_tokens','admin_tokens','event_audit',
      'reconciliation_queue','pending_linkbacks'
  )
ORDER BY tablename;
```

## 3. Subscriptions

- [ ] Each subscription is registered via `pg_ripple.create_subscription(...)`.
- [ ] `overflow_policy` is configured (`drop` or `dead_letter` — never `NULL`).
- [ ] `max_queue_depth` is set to a suitable value (recommended: 10,000–100,000).
- [ ] `dead_letter_after` is set to an appropriate retry count (recommended: 3–10).
- [ ] Frame includes `"@redact": true` for all PII predicates.

## 4. Tokens

- [ ] At least one per-subscription token registered via `pg_ripple.register_subscription_token(...)`.
- [ ] All tokens use **least-privilege scopes** (no extra scopes beyond relay requirements).
- [ ] Token rotation schedule is documented (recommended: 90-day rotation).
- [ ] Admin tokens registered in `_pg_ripple.admin_tokens` (if HTTP admin API is enabled).

## 5. Audit log

- [ ] `pg_ripple.audit_log_enabled` is `on`.
- [ ] `pg_ripple.audit_retention` is set (default: 90 days; adjust for compliance requirements).
- [ ] Confirm `purge_event_audit()` runs daily (via `pg_cron` or equivalent scheduler).

## 6. Health monitoring

- [ ] `pg_ripple.bidi_health()` is polled (recommended: every 60 seconds) by the alerting system.
- [ ] Alerts set for `status = 'failing'` and `status = 'degraded'` for more than 15 minutes.
- [ ] `pg_ripple.bidi_status()` included in dashboard (Grafana / Prometheus export).

## 7. Dead-letter triage

- [ ] `event_dead_letters` reviewed at least weekly.
- [ ] Alerts set when `dead_letter_count > 0` persists for more than 1 hour.
- [ ] Runbook for requeue / drop procedures is linked from the on-call guide.

## 8. Reconciliation

- [ ] `reconciliation_open` is monitored via `bidi_status()`.
- [ ] On-call procedure documented for each resolution action (`accept_external`, `force_internal`, `merge_via_owl_sameAs`, `dead_letter`).

## 9. Schema evolution

- [ ] Any planned frame, IRI template, or exclude-graph changes go through the [schema-evolution rollout playbook](./bidi-runbook.md#schema-evolution-rollout-playbook).
- [ ] `subscription_schema_changes` table is reviewed after each change.

## 10. Chaos testing

- [ ] `tests/stress/bidi_chaos.sh` passes on the production instance before each major release.
- [ ] Linkback idempotency is confirmed (`abandon_linkback` re-called ≥ 2× without error).

## 11. pg-trickle relay

- [ ] pg-trickle version matches `pg_ripple_http` compatibility matrix (see [compatibility.md](./compatibility.md)).
- [ ] Relay acknowledges rows within `outbox_oldest_age` SLA (recommended: ≤ 1 minute under normal load).
- [ ] Relay TLS certificate expiry is monitored.

## 12. Backup and restore

- [ ] `_pg_ripple.event_dead_letters`, `event_audit`, `subscription_tokens`, `reconciliation_queue` are included in the backup target.
- [ ] Point-in-time recovery drill has verified that these tables are restorable.

---

## Sign-off

| Item | Owner | Date |
|---|---|---|
| Extension version verified | | |
| Schema completeness verified | | |
| Subscriptions configured | | |
| Tokens registered | | |
| Audit log active | | |
| Health monitoring active | | |
| Dead-letter triage procedure documented | | |
| Chaos test passed | | |
| pg-trickle relay confirmed | | |
| Backup verified | | |
