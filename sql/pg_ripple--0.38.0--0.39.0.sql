-- Migration 0.38.0 → 0.39.0: Datalog HTTP API for pg_ripple_http
-- Schema changes: None (all changes are in the pg_ripple_http companion service binary)
-- Data-rewrite cost: None
-- Downgrade: No schema changes to revert; redeploy the previous pg_ripple_http binary.

-- No DDL changes required.
-- This release adds a /datalog REST API namespace to the pg_ripple_http service,
-- exposing 24 endpoints across four phases:
--   Phase 1 — Rule management (8 endpoints):
--     POST   /datalog/rules/{rule_set}           → pg_ripple.load_rules($1, $2)
--     POST   /datalog/rules/{rule_set}/builtin   → pg_ripple.load_rules_builtin($1)
--     GET    /datalog/rules                      → pg_ripple.list_rules()
--     DELETE /datalog/rules/{rule_set}           → pg_ripple.drop_rules($1)
--     POST   /datalog/rules/{rule_set}/add       → pg_ripple.add_rule($1, $2)
--     DELETE /datalog/rules/{rule_set}/{rule_id} → pg_ripple.remove_rule($1)
--     PUT    /datalog/rules/{rule_set}/enable    → pg_ripple.enable_rule_set($1)
--     PUT    /datalog/rules/{rule_set}/disable   → pg_ripple.disable_rule_set($1)
--   Phase 2 — Inference (6 endpoints):
--     POST /datalog/infer/{rule_set}             → pg_ripple.infer($1)
--     POST /datalog/infer/{rule_set}/stats       → pg_ripple.infer_with_stats($1)
--     POST /datalog/infer/{rule_set}/agg         → pg_ripple.infer_agg($1)
--     POST /datalog/infer/{rule_set}/wfs         → pg_ripple.infer_wfs($1)
--     POST /datalog/infer/{rule_set}/demand      → pg_ripple.infer_demand($1, $2::jsonb)
--     POST /datalog/infer/{rule_set}/lattice     → pg_ripple.infer_lattice($1, $2)
--   Phase 3 — Query & constraints (3 endpoints):
--     POST /datalog/query/{rule_set}             → pg_ripple.infer_goal($1, $2)
--     GET  /datalog/constraints                  → pg_ripple.check_constraints(NULL)
--     GET  /datalog/constraints/{rule_set}       → pg_ripple.check_constraints($1)
--   Phase 4 — Admin & monitoring (7 endpoints):
--     GET    /datalog/stats/cache                → pg_ripple.rule_plan_cache_stats()
--     GET    /datalog/stats/tabling              → pg_ripple.tabling_stats()
--     GET    /datalog/lattices                   → pg_ripple.list_lattices()
--     POST   /datalog/lattices                   → pg_ripple.create_lattice($1, $2, $3)
--     GET    /datalog/views                      → pg_ripple.list_datalog_views()
--     POST   /datalog/views                      → pg_ripple.create_datalog_view(...)
--     DELETE /datalog/views/{name}               → pg_ripple.drop_datalog_view($1)
-- All endpoints use parameterized queries; no SQL string concatenation.
-- New env var: PG_RIPPLE_HTTP_DATALOG_WRITE_TOKEN (optional write-protection)
-- New Prometheus metric: pg_ripple_http_datalog_queries_total

INSERT INTO _pg_ripple.schema_version (version, upgraded_from)
VALUES ('0.39.0', '0.38.0')
ON CONFLICT DO NOTHING;
