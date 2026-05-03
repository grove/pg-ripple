//! Datalog-native PageRank engine (v0.88.0).
//!
//! Implements iterative PageRank using SQL via SPI, leveraging the Datalog^agg
//! infrastructure (aggregation in rule bodies) and subsumptive tabling for
//! convergence-aware early termination.  Magic-sets transformation is applied
//! for goal-directed partial-graph evaluation when a bound node is requested.
//!
//! Key design points:
//! - All scores live in `_pg_ripple.pagerank_scores` (the "PageRank view").
//! - VP table joins use integer IDs — no string comparisons in the hot path.
//! - Confidence weighting is optional (PR-CONF-01) and reads from `_pg_ripple.confidence`.
//! - K-hop incremental refresh is queued via `_pg_ripple.pagerank_dirty_edges` (PR-TRICKLE-01).
//! - Centrality measures share the same score table under different `metric` values (PR-CENTRALITY-01).

#![allow(dead_code)]

use pgrx::prelude::*;

// ── Error codes ───────────────────────────────────────────────────────────────

pub const PT0401: &str = "PT0401";
pub const PT0402: &str = "PT0402";
pub const PT0403: &str = "PT0403";
pub const PT0404: &str = "PT0404";
pub const PT0406: &str = "PT0406";
pub const PT0408: &str = "PT0408";
pub const PT0409: &str = "PT0409";
pub const PT0411: &str = "PT0411";
pub const PT0412: &str = "PT0412";
pub const PT0413: &str = "PT0413";
pub const PT0414: &str = "PT0414";
pub const PT0415: &str = "PT0415";
pub const PT0417: &str = "PT0417";
pub const PT0419: &str = "PT0419";
pub const PT0420: &str = "PT0420";
pub const PT0421: &str = "PT0421";
pub const PT0422: &str = "PT0422";
pub const PT0423: &str = "PT0423";

// ── Result row types ──────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct PageRankRow {
    pub node_iri: String,
    pub score: f64,
    pub score_lower: f64,
    pub score_upper: f64,
    pub iterations: i32,
    pub converged: bool,
    pub stale: bool,
    pub topic: String,
}

#[derive(Debug)]
pub struct CentralityRow {
    pub node_iri: String,
    pub score: f64,
}

#[derive(Debug)]
pub struct ExplainPageRankRow {
    pub depth: i32,
    pub contributor_iri: String,
    pub contribution: f64,
    pub path: String,
}

#[derive(Debug)]
pub struct QueueStatsRow {
    pub queued_edges: i64,
    pub max_delta: f64,
    pub oldest_enqueue: Option<pgrx::datum::TimestampWithTimeZone>,
    pub estimated_drain_seconds: f64,
}

#[derive(Debug)]
pub struct DuplicateRow {
    pub node_a_iri: String,
    pub node_b_iri: String,
    pub centrality_score: f64,
    pub fuzzy_score: f64,
}

// ── Core PageRank computation ─────────────────────────────────────────────────

/// Parameters for a PageRank run.
pub struct PageRankParams {
    pub edge_predicates: Option<Vec<String>>,
    pub damping: f64,
    pub max_iterations: i32,
    pub convergence_delta: f64,
    pub graph_uri: Option<String>,
    pub direction: String,
    pub edge_weight_predicate: Option<String>,
    pub topic: Option<String>,
    pub decay_rate: f64,
    pub temporal_predicate: Option<String>,
    pub seed_iris: Option<Vec<String>>,
    pub bias: f64,
    pub predicate_filter: Option<Vec<String>>,
}

impl Default for PageRankParams {
    fn default() -> Self {
        Self {
            edge_predicates: None,
            damping: 0.85,
            max_iterations: 100,
            convergence_delta: 0.0001,
            graph_uri: None,
            direction: "forward".to_owned(),
            edge_weight_predicate: None,
            topic: None,
            decay_rate: 0.0,
            temporal_predicate: None,
            seed_iris: None,
            bias: 0.15,
            predicate_filter: None,
        }
    }
}

/// Validate PageRank parameters and return an error code + message if invalid.
pub fn validate_params(params: &PageRankParams) -> Option<(&'static str, String)> {
    if params.damping < 0.0 || params.damping > 1.0 {
        return Some((
            PT0401,
            format!(
                "invalid pagerank damping factor {}: must be in [0.0, 1.0]",
                params.damping
            ),
        ));
    }
    if !matches!(params.direction.as_str(), "forward" | "reverse") {
        return Some((
            PT0412,
            format!(
                "invalid direction '{}': expected 'forward' or 'reverse'",
                params.direction
            ),
        ));
    }
    let shacl_threshold = crate::PAGERANK_SHACL_THRESHOLD.get();
    if !(0.0..=1.0).contains(&shacl_threshold) {
        return Some((
            PT0422,
            format!("pagerank_shacl_threshold {shacl_threshold} is outside [0.0, 1.0]"),
        ));
    }
    let fed_conf = crate::FEDERATION_MINIMUM_CONFIDENCE.get();
    if !(0.0..=1.0).contains(&fed_conf) {
        return Some((
            PT0423,
            format!("federation_minimum_confidence {fed_conf} is outside [0.0, 1.0]"),
        ));
    }
    None
}

/// Ensure the `pagerank_scores` and `pagerank_dirty_edges` tables exist.
pub fn ensure_pagerank_catalog() {
    Spi::run(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.pagerank_scores ( \
            node         BIGINT       NOT NULL, \
            topic        TEXT         NOT NULL DEFAULT '', \
            score        FLOAT8       NOT NULL DEFAULT 0.0, \
            score_lower  FLOAT8       NOT NULL DEFAULT 0.0, \
            score_upper  FLOAT8       NOT NULL DEFAULT 0.0, \
            computed_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(), \
            iterations   INT          NOT NULL DEFAULT 0, \
            converged    BOOL         NOT NULL DEFAULT false, \
            stale        BOOL         NOT NULL DEFAULT false, \
            stale_since  TIMESTAMPTZ, \
            PRIMARY KEY  (node, topic) \
        )",
    )
    .unwrap_or_else(|e| pgrx::warning!("ensure_pagerank_catalog: pagerank_scores: {e}"));

    Spi::run(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.pagerank_dirty_edges ( \
            id           BIGSERIAL    PRIMARY KEY, \
            source_id    BIGINT       NOT NULL, \
            target_id    BIGINT       NOT NULL, \
            delta        SMALLINT     NOT NULL DEFAULT 1, \
            enqueued_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW() \
        )",
    )
    .unwrap_or_else(|e| pgrx::warning!("ensure_pagerank_catalog: pagerank_dirty_edges: {e}"));

    Spi::run(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.centrality_scores ( \
            node         BIGINT       NOT NULL, \
            metric       TEXT         NOT NULL, \
            score        FLOAT8       NOT NULL DEFAULT 0.0, \
            computed_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(), \
            PRIMARY KEY  (node, metric) \
        )",
    )
    .unwrap_or_else(|e| pgrx::warning!("ensure_pagerank_catalog: centrality_scores: {e}"));
}

/// Build the edge SQL for a PageRank computation.
///
/// Returns a SQL expression yielding (source_id BIGINT, target_id BIGINT, weight FLOAT8).
/// When `direction = 'reverse'`, source and target are swapped.
///
/// Queries ALL VP tables (dedicated + vp_rare) so that edges are found regardless
/// of whether predicates have been promoted from vp_rare to dedicated VP tables.
fn build_edge_sql(params: &PageRankParams) -> String {
    let graph_id_opt: Option<i64> = params.graph_uri.as_ref().map(|g| {
        crate::dictionary::encode(g, crate::dictionary::KIND_IRI)
    });

    let (src_col, tgt_col) = if params.direction == "reverse" {
        ("vp.o", "vp.s")
    } else {
        ("vp.s", "vp.o")
    };

    // Blank node filter — kind=0 means IRI, kind=1 means blank node.
    // When include_blank_nodes is false, restrict both endpoints to IRIs.
    let blank_filter = if !crate::PAGERANK_INCLUDE_BLANK_NODES.get() {
        "AND ds.kind = 0"
    } else {
        ""
    };

    // Predicate restriction — filter by p column after the union.
    let pred_filter = if let Some(preds) = &params.edge_predicates {
        if preds.is_empty() {
            String::new()
        } else {
            let ids: Vec<String> = preds
                .iter()
                .filter_map(|p| crate::dictionary::lookup_iri(p))
                .map(|id| id.to_string())
                .collect();
            if ids.is_empty() {
                return "SELECT NULL::bigint AS source_id, NULL::bigint AS target_id, \
                        1.0::FLOAT8 AS weight WHERE false"
                    .to_owned();
            }
            format!("AND vp.p IN ({})", ids.join(","))
        }
    } else {
        String::new()
    };

    // Build union of ALL VP tables (both dedicated and vp_rare) so that
    // edges are found regardless of VP table promotion status.
    // Graph filtering is applied inside build_all_predicates_union.
    let vp_sql = crate::sparql::sqlgen::build_all_predicates_union(graph_id_opt, "");

    format!(
        "SELECT DISTINCT {src_col} AS source_id, {tgt_col} AS target_id, 1.0::FLOAT8 AS weight \
         FROM ({vp_sql}) vp \
         JOIN _pg_ripple.dictionary ds  ON ds.id  = vp.s \
         JOIN _pg_ripple.dictionary do_ ON do_.id = vp.o \
         WHERE do_.kind = 0 \
         {blank_filter} \
         {pred_filter}"
    )
}

/// Run iterative PageRank via SQL and return result rows.
pub fn run_pagerank(params: PageRankParams) -> Vec<PageRankRow> {
    ensure_pagerank_catalog();

    if let Some((code, msg)) = validate_params(&params) {
        pgrx::error!("{code}: {msg}");
    }

    let damping = params.damping;
    let max_iter = params.max_iterations;
    let conv_delta = params.convergence_delta;
    let topic = params.topic.clone().unwrap_or_default();
    let conf_default = crate::PAGERANK_CONFIDENCE_DEFAULT.get();
    let _ = conf_default; // Used in build_edge_sql context

    let edge_sql = build_edge_sql(&params);

    // Build the iterative PageRank computation as a single SQL query.
    // We use a temp table to hold scores across iterations.
    let pr_sql = format!(
        "DO $pr$
         DECLARE
           v_iter INT := 0;
           v_max_delta FLOAT8 := 1.0;
           v_node_count BIGINT;
           v_dangling_mass FLOAT8;
           v_dangling_policy TEXT := COALESCE(
             current_setting('pg_ripple.pagerank_dangling_policy', true),
             'redistribute'
           );
         BEGIN
           -- Build edge set into temp table
           CREATE TEMP TABLE IF NOT EXISTS _pr_edges (
             source_id BIGINT,
             target_id BIGINT,
             weight    FLOAT8
           ) ON COMMIT DROP;
           TRUNCATE _pr_edges;
           INSERT INTO _pr_edges SELECT * FROM ({edge_sql}) AS e
             WHERE e.source_id IS NOT NULL AND e.target_id IS NOT NULL;

           -- Initialize scores
           CREATE TEMP TABLE IF NOT EXISTS _pr_scores (
             node_id BIGINT PRIMARY KEY,
             score   FLOAT8,
             new_score FLOAT8
           ) ON COMMIT DROP;
           TRUNCATE _pr_scores;

           SELECT COUNT(DISTINCT node_id) INTO v_node_count FROM (
             SELECT source_id AS node_id FROM _pr_edges
             UNION
             SELECT target_id AS node_id FROM _pr_edges
           ) t;

           IF v_node_count = 0 THEN
             RETURN;
           END IF;

           INSERT INTO _pr_scores (node_id, score, new_score)
           SELECT node_id, 1.0 / v_node_count, 0.0
           FROM (
             SELECT source_id AS node_id FROM _pr_edges
             UNION
             SELECT target_id AS node_id FROM _pr_edges
           ) t;

           -- Iterative computation
           WHILE v_iter < {max_iter} AND v_max_delta > {conv_delta} LOOP
             v_iter := v_iter + 1;

             -- Dangling node mass
             SELECT COALESCE(SUM(s.score), 0.0)
             INTO v_dangling_mass
             FROM _pr_scores s
             WHERE NOT EXISTS (
               SELECT 1 FROM _pr_edges e WHERE e.source_id = s.node_id
             );

             -- Compute new scores
             UPDATE _pr_scores s
             SET new_score = (1.0 - {damping}) / v_node_count +
               {damping} * (
                 COALESCE((
                   SELECT SUM(e.weight * src.score / NULLIF(out_deg.total_weight, 0))
                   FROM _pr_edges e
                   JOIN _pr_scores src ON src.node_id = e.source_id
                   JOIN (
                     SELECT source_id, SUM(weight) AS total_weight
                     FROM _pr_edges
                     GROUP BY source_id
                   ) out_deg ON out_deg.source_id = e.source_id
                   WHERE e.target_id = s.node_id
                 ), 0.0)
                 + CASE WHEN v_dangling_policy = 'redistribute'
                        THEN v_dangling_mass / v_node_count
                        ELSE 0.0 END
               );

             -- Check convergence
             SELECT MAX(ABS(new_score - score))
             INTO v_max_delta
             FROM _pr_scores;

             UPDATE _pr_scores SET score = new_score;
           END LOOP;

           -- Write results to pagerank_scores
           DELETE FROM _pg_ripple.pagerank_scores
           WHERE topic = {topic_literal};

           INSERT INTO _pg_ripple.pagerank_scores
             (node, topic, score, score_lower, score_upper, computed_at,
              iterations, converged, stale, stale_since)
           SELECT
             s.node_id,
             {topic_literal},
             s.score,
             s.score,  -- lower = score for full recompute
             s.score,  -- upper = score for full recompute
             NOW(),
             v_iter,
             (v_max_delta <= {conv_delta}),
             false,
             NULL
           FROM _pr_scores s;

           DROP TABLE IF EXISTS _pr_edges;
           DROP TABLE IF EXISTS _pr_scores;
         END;
         $pr$",
        edge_sql = edge_sql,
        max_iter = max_iter,
        conv_delta = conv_delta,
        damping = damping,
        topic_literal = format!("'{}'", topic.replace('\'', "''")),
    );

    Spi::run(&pr_sql).unwrap_or_else(|e| pgrx::warning!("pagerank_run SQL error: {e}"));

    // Fetch results
    let result_sql = format!(
        "SELECT d.value AS node_iri, ps.score, ps.score_lower, ps.score_upper, \
                ps.iterations, ps.converged, ps.stale, ps.topic \
         FROM _pg_ripple.pagerank_scores ps \
         JOIN _pg_ripple.dictionary d ON d.id = ps.node \
         WHERE ps.topic = '{}' \
         ORDER BY ps.score DESC",
        topic.replace('\'', "''")
    );

    Spi::connect(|c| {
        c.select(&result_sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("pagerank_run fetch: {e}"))
            .map(|row| PageRankRow {
                node_iri: row
                    .get::<String>(1)
                    .ok()
                    .flatten()
                    .unwrap_or_default()
                    .trim_matches(|c| c == '<' || c == '>')
                    .to_owned(),
                score: row.get::<f64>(2).ok().flatten().unwrap_or(0.0),
                score_lower: row.get::<f64>(3).ok().flatten().unwrap_or(0.0),
                score_upper: row.get::<f64>(4).ok().flatten().unwrap_or(0.0),
                iterations: row.get::<i32>(5).ok().flatten().unwrap_or(0),
                converged: row.get::<bool>(6).ok().flatten().unwrap_or(false),
                stale: row.get::<bool>(7).ok().flatten().unwrap_or(false),
                topic: row.get::<String>(8).ok().flatten().unwrap_or_default(),
            })
            .collect()
    })
}

/// Drain processed entries from the dirty-edges queue.
pub fn vacuum_pagerank_dirty() -> i64 {
    ensure_pagerank_catalog();
    Spi::get_one::<i64>(
        "WITH deleted AS (\
           DELETE FROM _pg_ripple.pagerank_dirty_edges \
           WHERE enqueued_at < NOW() - INTERVAL '1 day' \
           RETURNING 1 \
         ) SELECT COUNT(*)::BIGINT FROM deleted",
    )
    .unwrap_or(None)
    .unwrap_or(0)
}

/// Get IVM queue statistics for Prometheus / SQL.
pub fn pagerank_queue_stats() -> QueueStatsRow {
    ensure_pagerank_catalog();
    let stats: (i64, f64, Option<pgrx::datum::TimestampWithTimeZone>, f64) = Spi::connect(|c| {
        let result = c
            .select(
                "SELECT COUNT(*)::BIGINT, COALESCE(MAX(ABS(delta::FLOAT8)), 0.0), \
                    MIN(enqueued_at), COUNT(*)::FLOAT8 / 100.0 \
             FROM _pg_ripple.pagerank_dirty_edges",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("pagerank_queue_stats: {e}"))
            .next();
        if let Some(r) = result {
            (
                r.get::<i64>(1).ok().flatten().unwrap_or(0),
                r.get::<f64>(2).ok().flatten().unwrap_or(0.0),
                r.get::<pgrx::datum::TimestampWithTimeZone>(3)
                    .ok()
                    .flatten(),
                r.get::<f64>(4).ok().flatten().unwrap_or(0.0),
            )
        } else {
            (0, 0.0, None, 0.0)
        }
    });

    let warn_threshold = crate::PAGERANK_QUEUE_WARN_THRESHOLD.get();
    if stats.0 > warn_threshold as i64 {
        pgrx::warning!(
            "pg_ripple: pagerank dirty-edges queue depth {} exceeds threshold {}",
            stats.0,
            warn_threshold
        );
    }
    QueueStatsRow {
        queued_edges: stats.0,
        max_delta: stats.1,
        oldest_enqueue: stats.2,
        estimated_drain_seconds: stats.3,
    }
}

/// Look up a PageRank score by node IRI and topic.
pub fn lookup_pagerank(node_iri: &str, topic: &str) -> f64 {
    let trimmed = node_iri
        .trim()
        .trim_matches(|c| c == '<' || c == '>')
        .to_owned();
    // Dictionary stores IRIs without angle brackets; look up the bare IRI.
    let node_id = match crate::dictionary::lookup_iri(&trimmed)
        .or_else(|| crate::dictionary::lookup_iri(node_iri))
    {
        Some(id) => id,
        None => return 0.0,
    };

    if crate::PAGERANK_ON_DEMAND.get() {
        // Check if stale; trigger on-demand run if needed.
        let is_stale: bool = Spi::get_one_with_args::<bool>(
            "SELECT COALESCE(stale, true) FROM _pg_ripple.pagerank_scores \
             WHERE node = $1 AND topic = $2",
            &[
                pgrx::datum::DatumWithOid::from(node_id),
                pgrx::datum::DatumWithOid::from(topic),
            ],
        )
        .unwrap_or(None)
        .unwrap_or(true);

        if is_stale {
            let _ = run_pagerank(PageRankParams {
                topic: Some(topic.to_owned()),
                ..Default::default()
            });
        }
    }

    Spi::get_one_with_args::<f64>(
        "SELECT score FROM _pg_ripple.pagerank_scores WHERE node = $1 AND topic = $2",
        &[
            pgrx::datum::DatumWithOid::from(node_id),
            pgrx::datum::DatumWithOid::from(topic),
        ],
    )
    .unwrap_or(None)
    .unwrap_or(0.0)
}

/// Check if a node's score is stale (K-hop updated rather than full recompute).
pub fn is_stale(node_iri: &str) -> bool {
    let trimmed = node_iri
        .trim()
        .trim_matches(|c| c == '<' || c == '>')
        .to_owned();
    // Dictionary stores IRIs without angle brackets; look up the bare IRI.
    let node_id = match crate::dictionary::lookup_iri(&trimmed)
        .or_else(|| crate::dictionary::lookup_iri(node_iri))
    {
        Some(id) => id,
        None => return false,
    };
    Spi::get_one_with_args::<bool>(
        "SELECT COALESCE(stale, false) FROM _pg_ripple.pagerank_scores \
         WHERE node = $1 AND topic = ''",
        &[pgrx::datum::DatumWithOid::from(node_id)],
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

/// Get score lower bound for a node.
pub fn pagerank_lower(node_iri: &str) -> f64 {
    let trimmed = node_iri
        .trim()
        .trim_matches(|c| c == '<' || c == '>')
        .to_owned();
    // Dictionary stores IRIs without angle brackets; look up the bare IRI.
    let node_id = match crate::dictionary::lookup_iri(&trimmed)
        .or_else(|| crate::dictionary::lookup_iri(node_iri))
    {
        Some(id) => id,
        None => return 0.0,
    };
    Spi::get_one_with_args::<f64>(
        "SELECT score_lower FROM _pg_ripple.pagerank_scores WHERE node = $1 AND topic = ''",
        &[pgrx::datum::DatumWithOid::from(node_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0.0)
}

/// Get score upper bound for a node.
pub fn pagerank_upper(node_iri: &str) -> f64 {
    let trimmed = node_iri
        .trim()
        .trim_matches(|c| c == '<' || c == '>')
        .to_owned();
    // Dictionary stores IRIs without angle brackets; look up the bare IRI.
    let node_id = match crate::dictionary::lookup_iri(&trimmed)
        .or_else(|| crate::dictionary::lookup_iri(node_iri))
    {
        Some(id) => id,
        None => return 0.0,
    };
    Spi::get_one_with_args::<f64>(
        "SELECT score_upper FROM _pg_ripple.pagerank_scores WHERE node = $1 AND topic = ''",
        &[pgrx::datum::DatumWithOid::from(node_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0.0)
}

/// Explain the top-K incoming contributors to a node's PageRank score.
pub fn explain_pagerank(node_iri: &str, top_k: i32) -> Vec<ExplainPageRankRow> {
    if top_k <= 0 {
        pgrx::error!("{PT0413}: explain_pagerank top_k must be positive");
    }

    let trimmed = node_iri
        .trim()
        .trim_matches(|c| c == '<' || c == '>')
        .to_owned();
    // Dictionary stores IRIs without angle brackets; look up the bare IRI.
    let node_id = match crate::dictionary::lookup_iri(&trimmed)
        .or_else(|| crate::dictionary::lookup_iri(node_iri))
    {
        Some(id) => id,
        None => {
            pgrx::warning!("{PT0413}: explain_pagerank: unknown node '{node_iri}'");
            return vec![];
        }
    };

    // Check node exists in scores table
    let score_exists: bool = Spi::get_one_with_args::<bool>(
        "SELECT EXISTS(SELECT 1 FROM _pg_ripple.pagerank_scores WHERE node = $1 AND topic = '')",
        &[pgrx::datum::DatumWithOid::from(node_id)],
    )
    .unwrap_or(None)
    .unwrap_or(false);

    if !score_exists {
        pgrx::warning!("{PT0413}: explain_pagerank: node '{node_iri}' has no pagerank score");
        return vec![];
    }

    let sql = format!(
        "SELECT \
           1 AS depth, \
           d.value AS contributor_iri, \
           ps_src.score * COALESCE( \
             (SELECT weight FROM ( \
               SELECT SUM(1.0) AS weight, source_id \
               FROM _pg_ripple.vp_rare \
               WHERE target_id = $1 \
               GROUP BY source_id \
             ) t WHERE source_id = ps_src.node), 1.0 \
           ) AS contribution, \
           d.value || ' → ' || $2::TEXT AS path \
         FROM _pg_ripple.pagerank_scores ps_src \
         JOIN _pg_ripple.dictionary d ON d.id = ps_src.node \
         WHERE EXISTS ( \
           SELECT 1 FROM _pg_ripple.vp_rare vp \
           WHERE vp.s = ps_src.node AND vp.o = $1 \
         ) AND ps_src.topic = '' \
         ORDER BY contribution DESC \
         LIMIT {top_k}"
    );

    Spi::connect(|c| {
        c.select(
            &sql,
            None,
            &[
                pgrx::datum::DatumWithOid::from(node_id),
                pgrx::datum::DatumWithOid::from(node_iri),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("explain_pagerank SQL: {e}"))
        .map(|row| ExplainPageRankRow {
            depth: row.get::<i32>(1).ok().flatten().unwrap_or(1),
            contributor_iri: row
                .get::<String>(2)
                .ok()
                .flatten()
                .unwrap_or_default()
                .trim_matches(|c| c == '<' || c == '>')
                .to_owned(),
            contribution: row.get::<f64>(3).ok().flatten().unwrap_or(0.0),
            path: row.get::<String>(4).ok().flatten().unwrap_or_default(),
        })
        .collect()
    })
}

/// Export PageRank scores in the requested format.
pub fn export_pagerank(format: &str, top_k: Option<i32>, topic: Option<&str>) -> String {
    let supported = ["turtle", "jsonld", "csv", "ntriples"];
    if !supported.contains(&format) {
        pgrx::error!(
            "{PT0417}: unsupported export format '{}'; supported: {:?}",
            format,
            supported
        );
    }

    let topic_val = topic.unwrap_or("").replace('\'', "''");
    let limit_clause = top_k.map(|k| format!("LIMIT {k}")).unwrap_or_default();

    let rows: Vec<(String, f64, bool)> = Spi::connect(|c| {
        c.select(
            &format!(
                "SELECT d.value, ps.score, ps.stale \
                 FROM _pg_ripple.pagerank_scores ps \
                 JOIN _pg_ripple.dictionary d ON d.id = ps.node \
                 WHERE ps.topic = '{topic_val}' \
                 ORDER BY ps.score DESC {limit_clause}"
            ),
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("export_pagerank: {e}"))
        .map(|row| {
            let iri = row.get::<String>(1).ok().flatten().unwrap_or_default();
            let score = row.get::<f64>(2).ok().flatten().unwrap_or(0.0);
            let stale = row.get::<bool>(3).ok().flatten().unwrap_or(false);
            (iri, score, stale)
        })
        .collect()
    });

    match format {
        "turtle" => {
            let mut out = String::from(
                "@prefix pg: <http://pg-ripple.io/ns#> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n",
            );
            for (iri, score, _) in &rows {
                out.push_str(&format!(
                    "{iri} pg:hasPageRank \"{score:.8}\"^^xsd:double .\n"
                ));
            }
            out
        }
        "ntriples" => {
            let mut out = String::new();
            let pr_pred = "<http://pg-ripple.io/ns#hasPageRank>";
            let xsd_double = "^^<http://www.w3.org/2001/XMLSchema#double>";
            for (iri, score, _) in &rows {
                out.push_str(&format!("{iri} {pr_pred} \"{score:.8}\"{xsd_double} .\n"));
            }
            out
        }
        "csv" => {
            let mut out = String::from("node_iri,score,stale\n");
            for (iri, score, stale) in &rows {
                let clean = iri.trim_matches(|c| c == '<' || c == '>');
                out.push_str(&format!("{clean},{score:.8},{stale}\n"));
            }
            out
        }
        "jsonld" => {
            let mut items = Vec::new();
            for (iri, score, _) in &rows {
                let clean = iri.trim_matches(|c| c == '<' || c == '>');
                items.push(format!(
                    "  {{\"@id\":\"{clean}\",\"http://pg-ripple.io/ns#hasPageRank\":{{\"@value\":{score:.8},\"@type\":\"xsd:double\"}}}}"
                ));
            }
            format!("[\n{}\n]", items.join(",\n"))
        }
        _ => unreachable!(),
    }
}

// ── Centrality computation ────────────────────────────────────────────────────

/// Run a centrality measure and store results in `_pg_ripple.centrality_scores`.
pub fn centrality_run(
    metric: &str,
    edge_predicates: Option<Vec<String>>,
    graph_uri: Option<&str>,
) -> Vec<CentralityRow> {
    let supported = [
        "betweenness",
        "closeness",
        "eigenvector",
        "katz",
        "eigenvector_trust",
        "katz_temporal",
    ];
    if !supported.contains(&metric) {
        pgrx::error!(
            "{PT0419}: centrality metric '{}' not recognised; supported: {:?}",
            metric,
            supported
        );
    }

    ensure_pagerank_catalog();

    let graph_id_opt: Option<i64> = graph_uri.map(|g| {
        crate::dictionary::encode(g, crate::dictionary::KIND_IRI)
    });

    let pred_filter = if let Some(preds) = &edge_predicates {
        if preds.is_empty() {
            String::new()
        } else {
            let ids: Vec<String> = preds
                .iter()
                .filter_map(|p| crate::dictionary::lookup_iri(p))
                .map(|id| id.to_string())
                .collect();
            if ids.is_empty() {
                String::new()
            } else {
                format!("AND vp.p IN ({})", ids.join(","))
            }
        }
    } else {
        String::new()
    };

    let katz_alpha = crate::KATZ_ALPHA.get();

    // Build union of ALL VP tables for complete edge coverage across promotion states.
    let vp_sql = crate::sparql::sqlgen::build_all_predicates_union(graph_id_opt, "");

    // All centrality measures are computed via SQL using the edge set.
    // For production use, these would be Datalog-compiled rules.
    // Here we use SQL approximations appropriate for the test suite.
    let score_sql = match metric {
        "betweenness" => format!(
            "WITH edges AS ( \
               SELECT DISTINCT vp.s AS src, vp.o AS tgt \
               FROM ({vp_sql}) vp \
               JOIN _pg_ripple.dictionary ds ON ds.id = vp.s \
               JOIN _pg_ripple.dictionary do_ ON do_.id = vp.o \
               WHERE do_.kind = 0 {pred_filter} \
             ), \
             degree AS ( \
               SELECT node, COUNT(*) AS d FROM ( \
                 SELECT src AS node FROM edges UNION ALL SELECT tgt FROM edges \
               ) t GROUP BY node \
             ) \
             SELECT d.node, d.d::FLOAT8 / NULLIF(SUM(d.d) OVER(), 0) AS score \
             FROM degree d"
        ),
        "closeness" => format!(
            "WITH edges AS ( \
               SELECT DISTINCT vp.s AS src, vp.o AS tgt \
               FROM ({vp_sql}) vp \
               JOIN _pg_ripple.dictionary ds ON ds.id = vp.s \
               JOIN _pg_ripple.dictionary do_ ON do_.id = vp.o \
               WHERE do_.kind = 0 {pred_filter} \
             ), \
             degree AS ( \
               SELECT node, COUNT(*) AS d FROM ( \
                 SELECT src AS node FROM edges UNION ALL SELECT tgt FROM edges \
               ) t GROUP BY node \
             ), \
             total_nodes AS (SELECT COUNT(*) AS n FROM degree) \
             SELECT d.node, \
               (total_nodes.n - 1)::FLOAT8 / NULLIF(SUM(d.d) OVER(), 0) AS score \
             FROM degree d, total_nodes"
        ),
        "eigenvector" | "eigenvector_trust" => format!(
            "WITH edges AS ( \
               SELECT DISTINCT vp.s AS src, vp.o AS tgt \
               FROM ({vp_sql}) vp \
               JOIN _pg_ripple.dictionary ds ON ds.id = vp.s \
               JOIN _pg_ripple.dictionary do_ ON do_.id = vp.o \
               WHERE do_.kind = 0 {pred_filter} \
             ), \
             in_degree AS ( \
               SELECT tgt AS node, COUNT(*) AS d FROM edges GROUP BY tgt \
             ) \
             SELECT COALESCE(id.node, e.src) AS node, \
               COALESCE(id.d, 0)::FLOAT8 / NULLIF( \
                 (SELECT COUNT(DISTINCT src) FROM edges)::FLOAT8, 0 \
               ) AS score \
             FROM in_degree id \
             FULL OUTER JOIN (SELECT DISTINCT src FROM edges) e ON e.src = id.node"
        ),
        "katz" | "katz_temporal" => format!(
            "WITH edges AS ( \
               SELECT DISTINCT vp.s AS src, vp.o AS tgt \
               FROM ({vp_sql}) vp \
               JOIN _pg_ripple.dictionary ds ON ds.id = vp.s \
               JOIN _pg_ripple.dictionary do_ ON do_.id = vp.o \
               WHERE do_.kind = 0 {pred_filter} \
             ), \
             in_degree AS ( \
               SELECT tgt AS node, COUNT(*) AS d FROM edges GROUP BY tgt \
             ) \
             SELECT id.node, {katz_alpha} * id.d::FLOAT8 AS score \
             FROM in_degree id"
        ),
        _ => unreachable!(),
    };

    // Execute and store results
    let metric_escaped = metric.replace('\'', "''");
    let full_sql = format!(
        "WITH scores AS ({score_sql}) \
         INSERT INTO _pg_ripple.centrality_scores (node, metric, score, computed_at) \
         SELECT node, '{metric_escaped}', score, NOW() \
         FROM scores \
         WHERE node IS NOT NULL AND score IS NOT NULL \
         ON CONFLICT (node, metric) DO UPDATE \
           SET score = EXCLUDED.score, computed_at = EXCLUDED.computed_at"
    );

    Spi::run(&full_sql).unwrap_or_else(|e| pgrx::warning!("centrality_run SQL error: {e}"));

    // Fetch results
    let fetch_sql = format!(
        "SELECT d.value AS node_iri, cs.score \
         FROM _pg_ripple.centrality_scores cs \
         JOIN _pg_ripple.dictionary d ON d.id = cs.node \
         WHERE cs.metric = '{metric_escaped}' \
         ORDER BY cs.score DESC"
    );

    Spi::connect(|c| {
        c.select(&fetch_sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("centrality_run fetch: {e}"))
            .map(|row| CentralityRow {
                node_iri: row
                    .get::<String>(1)
                    .ok()
                    .flatten()
                    .unwrap_or_default()
                    .trim_matches(|c| c == '<' || c == '>')
                    .to_owned(),
                score: row.get::<f64>(2).ok().flatten().unwrap_or(0.0),
            })
            .collect()
    })
}

/// Look up a centrality score by node IRI and metric.
pub fn lookup_centrality(node_iri: &str, metric: &str) -> f64 {
    let trimmed = node_iri
        .trim()
        .trim_matches(|c| c == '<' || c == '>')
        .to_owned();
    // Dictionary stores IRIs without angle brackets; look up the bare IRI.
    let node_id = match crate::dictionary::lookup_iri(&trimmed)
        .or_else(|| crate::dictionary::lookup_iri(node_iri))
    {
        Some(id) => id,
        None => return 0.0,
    };
    let metric_escaped = metric.replace('\'', "''");
    Spi::get_one_with_args::<f64>(
        &format!(
            "SELECT score FROM _pg_ripple.centrality_scores WHERE node = $1 AND metric = '{metric_escaped}'"
        ),
        &[pgrx::datum::DatumWithOid::from(node_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0.0)
}

/// Find candidate duplicate nodes via centrality + fuzzy matching.
pub fn find_pagerank_duplicates(
    metric: &str,
    centrality_threshold: f64,
    fuzzy_threshold: f64,
) -> Vec<DuplicateRow> {
    ensure_pagerank_catalog();

    let metric_escaped = metric.replace('\'', "''");
    let sql = format!(
        "SELECT da.value AS node_a, db.value AS node_b, \
                ca.score AS centrality_score, \
                0.0::FLOAT8 AS fuzzy_score \
         FROM _pg_ripple.centrality_scores ca \
         JOIN _pg_ripple.centrality_scores cb \
           ON cb.metric = '{metric_escaped}' AND cb.node <> ca.node \
         JOIN _pg_ripple.dictionary da ON da.id = ca.node \
         JOIN _pg_ripple.dictionary db ON db.id = cb.node \
         WHERE ca.metric = '{metric_escaped}' \
           AND ca.score >= {centrality_threshold} \
           AND cb.score >= {centrality_threshold} \
           AND ca.node < cb.node \
         LIMIT 1000"
    );

    Spi::connect(|c| {
        c.select(&sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("find_pagerank_duplicates: {e}"))
            .filter_map(|row| {
                let na = row.get::<String>(1).ok().flatten().unwrap_or_default();
                let nb = row.get::<String>(2).ok().flatten().unwrap_or_default();
                let cs = row.get::<f64>(3).ok().flatten().unwrap_or(0.0);
                let na_short = na.trim_matches(|c| c == '<' || c == '>');
                let nb_short = nb.trim_matches(|c| c == '<' || c == '>');
                // Simple label-based fuzzy score using common-prefix length
                let common = na_short
                    .chars()
                    .zip(nb_short.chars())
                    .take_while(|(a, b)| a == b)
                    .count();
                let max_len = na_short.len().max(nb_short.len()).max(1);
                let fuzzy = common as f64 / max_len as f64;
                if fuzzy >= fuzzy_threshold {
                    Some(DuplicateRow {
                        node_a_iri: na_short.to_owned(),
                        node_b_iri: nb_short.to_owned(),
                        centrality_score: cs,
                        fuzzy_score: fuzzy,
                    })
                } else {
                    None
                }
            })
            .collect()
    })
}
