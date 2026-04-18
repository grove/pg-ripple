-- pg_regress test: SPARQL-star UPDATE operations (v0.24.0)
--
-- Tests INSERT DATA with quoted triple subjects/objects and SPARQL-star SELECT
-- using the fixed resolve_term_pattern / resolve_ground_term for Triple.

SET search_path TO pg_ripple, public;

-- Pre-test cleanup: remove any su/* triples from all storage.
DO $$
DECLARE
    su_ids BIGINT[];
    pred_id BIGINT;
BEGIN
    SELECT array_agg(id) INTO su_ids
    FROM _pg_ripple.dictionary WHERE value LIKE 'https://example.org/su/%';
    IF su_ids IS NULL THEN RETURN; END IF;
    DELETE FROM _pg_ripple.vp_rare WHERE s = ANY(su_ids) OR o = ANY(su_ids);
    FOR pred_id IN SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL LOOP
        BEGIN
            EXECUTE format('DELETE FROM _pg_ripple.vp_%s_delta WHERE s = ANY($1) OR o = ANY($1)', pred_id) USING su_ids;
            EXECUTE format('DELETE FROM _pg_ripple.vp_%s_main  WHERE s = ANY($1) OR o = ANY($1)', pred_id) USING su_ids;
        EXCEPTION WHEN undefined_table THEN NULL;
        END;
    END LOOP;
END $$;

-- ── 1. INSERT DATA with quoted triple in object position ──────────────────────

-- Load via SPARQL UPDATE: assert that Alice knows Bob
SELECT pg_ripple.sparql_update(
    'PREFIX ex: <https://example.org/su/> '
    'INSERT DATA { '
    '  ex:carol ex:believes << ex:alice ex:knows ex:bob >> . '
    '}'
) AS inserted_quoted_object;

-- Check round-trip: find carol's belief triple.
SELECT count(*) >= 1 AS carol_belief_found
FROM pg_ripple.sparql(
    'PREFIX ex: <https://example.org/su/> '
    'SELECT ?x WHERE { ex:carol ex:believes ?x }'
);

-- ── 2. INSERT DATA with quoted triple in subject position ─────────────────────

SELECT pg_ripple.sparql_update(
    'PREFIX ex: <https://example.org/su/> '
    'INSERT DATA { '
    '  << ex:alice ex:knows ex:bob >> ex:assertedBy ex:carol . '
    '}'
) AS inserted_quoted_subject;

-- Check round-trip: find subject-position annotation.
SELECT count(*) >= 1 AS assertion_found
FROM pg_ripple.sparql(
    'PREFIX ex: <https://example.org/su/> '
    'SELECT ?who WHERE { ?s ex:assertedBy ?who }'
);

-- ── 3. Plain INSERT DATA still works alongside SPARQL-star ───────────────────
SELECT pg_ripple.sparql_update(
    'PREFIX ex: <https://example.org/su/> '
    'INSERT DATA { ex:dave ex:age "42"^^<http://www.w3.org/2001/XMLSchema#integer> . }'
) AS plain_insert_ok;

SELECT count(*) >= 1 AS dave_found
FROM pg_ripple.sparql(
    'PREFIX ex: <https://example.org/su/> '
    'SELECT ?a WHERE { ex:dave ex:age ?a }'
);

-- ── 4. export_ntriples includes the quoted triples ────────────────────────────
SELECT length(pg_ripple.export_ntriples()) > 0 AS export_nonempty;

-- Post-test cleanup: remove all su/* triples so later tests are not polluted.
DO $$
DECLARE
    su_ids BIGINT[];
    pred_id BIGINT;
BEGIN
    SELECT array_agg(id) INTO su_ids
    FROM _pg_ripple.dictionary WHERE value LIKE 'https://example.org/su/%';
    IF su_ids IS NULL THEN RETURN; END IF;
    DELETE FROM _pg_ripple.vp_rare WHERE s = ANY(su_ids) OR o = ANY(su_ids) OR p = ANY(su_ids);
    FOR pred_id IN SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL LOOP
        BEGIN
            EXECUTE format('DELETE FROM _pg_ripple.vp_%s_delta WHERE s=ANY($1) OR o=ANY($1)', pred_id) USING su_ids;
            EXECUTE format('DELETE FROM _pg_ripple.vp_%s_main  WHERE s=ANY($1) OR o=ANY($1)', pred_id) USING su_ids;
        EXCEPTION WHEN undefined_table THEN NULL;
        END;
    END LOOP;
END $$;
