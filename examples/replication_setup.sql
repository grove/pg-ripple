-- examples/replication_setup.sql
-- Annotated walkthrough for setting up logical replication between a primary
-- PostgreSQL 18 + pg_ripple instance and one or more read replicas.
--
-- This example uses PostgreSQL built-in logical replication (PUBLICATION /
-- SUBSCRIPTION) to replicate the pg_ripple internal tables from primary to
-- standby.  pg_ripple.replication_enabled = on (v0.54.0+) must be set on
-- the primary.
--
-- ┌─────────────────────┐       logical replication        ┌──────────────────┐
-- │  PRIMARY            │  ──────────────────────────────▶  │  REPLICA         │
-- │  pg_ripple enabled  │       WAL streaming              │  read-only mirror │
-- └─────────────────────┘                                   └──────────────────┘
--
-- IMPORTANT: Run the PRIMARY sections on the primary server,
--            and the REPLICA sections on each replica server.
-- ──────────────────────────────────────────────────────────────────────────────

-- ==============================================================================
-- SECTION 1 (PRIMARY): Enable pg_ripple replication mode
-- ==============================================================================

-- Set replication mode so pg_ripple marks its internal tables with
-- REPLICA IDENTITY FULL, enabling logical replication to capture all changes.
-- This must be set *before* creating the extension or in postgresql.conf.
ALTER SYSTEM SET pg_ripple.replication_enabled = on;
SELECT pg_reload_conf();   -- Reload without restart (Sighup-level GUC)

-- Verify the setting took effect:
SHOW pg_ripple.replication_enabled;
-- Expected: on

-- ==============================================================================
-- SECTION 2 (PRIMARY): Create the extension and a publication
-- ==============================================================================

-- Install the extension on the primary (if not already installed):
CREATE EXTENSION IF NOT EXISTS pg_ripple;

-- Verify replica identity is set on the key tables:
SELECT relname, relreplident
FROM pg_class
WHERE relname IN ('dictionary', 'predicates', 'vp_rare', 'shacl_shapes',
                  'federation_endpoints', 'federation_cache')
  AND relnamespace = (SELECT oid FROM pg_namespace WHERE nspname = '_pg_ripple');
-- Expected: 'f' (FULL) for all rows

-- Create a publication covering all pg_ripple internal tables.
-- Using FOR ALL TABLES IN SCHEMA is the simplest option; you can also list
-- individual tables for finer control.
CREATE PUBLICATION pg_ripple_pub
    FOR ALL TABLES IN SCHEMA _pg_ripple, pg_ripple;

-- Verify the publication was created:
SELECT pubname, puballtables, pubinsert, pubupdate, pubdelete
FROM pg_publication
WHERE pubname = 'pg_ripple_pub';

-- ==============================================================================
-- SECTION 3 (PRIMARY): Create a replication user
-- ==============================================================================

-- The replica needs a dedicated user with REPLICATION privilege.
-- Never use a superuser account for replication in production.
CREATE ROLE pg_ripple_replicator
    WITH LOGIN REPLICATION PASSWORD 'change-me-in-production';

-- Grant SELECT on pg_ripple schema so the walsender can read rows.
GRANT SELECT ON ALL TABLES IN SCHEMA _pg_ripple TO pg_ripple_replicator;
GRANT SELECT ON ALL TABLES IN SCHEMA pg_ripple   TO pg_ripple_replicator;
GRANT USAGE ON SCHEMA _pg_ripple, pg_ripple TO pg_ripple_replicator;

-- ==============================================================================
-- SECTION 4 (REPLICA): Install extension and create subscription
-- ==============================================================================

-- On the replica, install pg_ripple (same version as primary):
-- CREATE EXTENSION IF NOT EXISTS pg_ripple;
--
-- IMPORTANT: The replica's pg_ripple tables must exist before creating the
-- subscription.  Do NOT load data on the replica before syncing.

-- Create a subscription pointing to the primary.
-- Replace the connection string with your actual primary host/port/db.
CREATE SUBSCRIPTION pg_ripple_sub
    CONNECTION 'host=primary.example.com port=5432 dbname=mydb user=pg_ripple_replicator password=change-me-in-production'
    PUBLICATION pg_ripple_pub
    WITH (
        copy_data = true,       -- initial full copy of current data
        synchronous_commit = on -- replicas lag only by wal_sender_timeout
    );

-- Verify the subscription state (on the replica):
SELECT subname, subenabled, subslotname, subpublications
FROM pg_subscription
WHERE subname = 'pg_ripple_sub';

-- Check replication lag (on the primary):
SELECT application_name, state, sent_lsn, write_lsn, flush_lsn, replay_lsn,
       write_lag, flush_lag, replay_lag
FROM pg_stat_replication;

-- ==============================================================================
-- SECTION 5: Read-only queries on the replica
-- ==============================================================================

-- Once synced, SPARQL and standard SQL queries work on the replica.
-- pg_ripple.read_replica_dsn GUC can be used to route read queries from the
-- primary to the replica automatically (v0.55.0):

-- On the primary:
ALTER SYSTEM SET pg_ripple.read_replica_dsn =
    'host=replica.example.com port=5432 dbname=mydb user=myapp password=myapppass';
SELECT pg_reload_conf();

-- Queries that run via the SPARQL engine will then be forwarded to the replica.
-- NOTE: Write operations (INSERT, UPDATE, DELETE) always execute on the primary.

-- ==============================================================================
-- SECTION 6: Monitoring replication health
-- ==============================================================================

-- Check replication slot lag (on the primary):
SELECT slot_name, active, confirmed_flush_lsn,
       pg_wal_lsn_diff(pg_current_wal_lsn(), confirmed_flush_lsn) AS bytes_behind
FROM pg_replication_slots
WHERE slot_name = 'pg_ripple_sub';

-- Alert if bytes_behind > 100MB (100 * 1024 * 1024):
SELECT slot_name,
       pg_wal_lsn_diff(pg_current_wal_lsn(), confirmed_flush_lsn) AS bytes_behind,
       CASE WHEN pg_wal_lsn_diff(pg_current_wal_lsn(), confirmed_flush_lsn) > 104857600
            THEN 'WARNING: replica is >100MB behind'
            ELSE 'OK'
       END AS status
FROM pg_replication_slots
WHERE slot_name = 'pg_ripple_sub';

-- ==============================================================================
-- SECTION 7: Failover and promotion
-- ==============================================================================

-- If the primary fails, promote the replica:
--   On the replica server: pg_ctl promote -D $PGDATA
--
-- After promotion, re-create the extension as read-write:
--   (the replica already has all data from replication)
--
-- Update pg_ripple.replication_enabled and re-create a new publication
-- if you set up a new replica from the promoted standby.

-- ==============================================================================
-- SECTION 8: Teardown (for testing environments)
-- ==============================================================================

-- Drop subscription on the replica:
-- DROP SUBSCRIPTION pg_ripple_sub;

-- Drop publication on the primary:
-- DROP PUBLICATION pg_ripple_pub;

-- Drop replication user:
-- DROP ROLE pg_ripple_replicator;
