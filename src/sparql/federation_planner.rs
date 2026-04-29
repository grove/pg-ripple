//! Cost-based federation source selection (v0.42.0).
//!
//! Implements a FedX-style planner that uses VoID statistics to rank
//! federation endpoints by estimated selectivity and assigns each BGP atom
//! to its best source.  Independent atoms (no shared variables with other
//! atoms) are scheduled for parallel execution up to
//! `pg_ripple.federation_parallel_max`.
//!
//! # VoID statistics
//!
//! On endpoint registration the planner attempts to fetch the endpoint's VoID
//! description from `{endpoint_url}/.well-known/void` or by querying the
//! endpoint with a SPARQL DESCRIBE.  The statistics are cached in
//! `_pg_ripple.endpoint_stats` with a TTL driven by
//! `pg_ripple.federation_stats_ttl_secs`.
//!
//! The relevant VoID properties used for cost estimation:
//! - `void:triples` — total triple count for the dataset
//! - `void:propertyPartition` + `void:property` + `void:triples` — per-predicate
//!   triple counts
//! - `void:distinctSubjects` / `void:distinctObjects` — selectivity estimates

// v0.56.0 dead-code audit (A-6):
// refresh_endpoint_stats: LIVE — called from federation_registry.rs:185.
// load_endpoint_stats: dead — not yet wired into the query planner; keep with
//   per-item annotation as it forms the planned cost-based API.
// estimate_selectivity, rank_endpoints_for_predicate, compute_parallel_groups:
//   dead — planned API for cost-based source selection; keep with per-item
//   annotations until wired.
// fetch_void_stats, execute_count_query, execute_predicate_count_query,
//   urlencoding_encode: internal helpers — annotated per-item.
// Replaced file-wide #![allow(dead_code)] with per-item annotations below.

use std::collections::HashMap;

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

// ─── VoID statistics cache ────────────────────────────────────────────────────

/// Per-endpoint VoID statistics used for cost estimation.
#[derive(Debug, Clone, Default)]
pub struct EndpointStats {
    /// Total triple count for the endpoint.
    pub total_triples: i64,
    /// Per-predicate IRI → triple count.
    pub predicate_triples: HashMap<String, i64>,
    /// Estimated distinct subject count.
    pub distinct_subjects: i64,
    /// Estimated distinct object count.
    pub distinct_objects: i64,
}

/// Fetch and cache VoID statistics for a registered endpoint.
///
/// Tries `{url}/.well-known/void` first; falls back to a SPARQL ASK + SELECT
/// against the endpoint itself.  Results are stored in `_pg_ripple.endpoint_stats`.
/// Skips the fetch if cached statistics are still within the TTL.
pub fn refresh_endpoint_stats(url: &str) {
    let ttl_secs = crate::FEDERATION_STATS_TTL_SECS.get();

    // Check whether cached stats are still fresh.
    if ttl_secs > 0 {
        let fresh: bool = Spi::get_one_with_args::<bool>(
            "SELECT EXISTS(
                SELECT 1 FROM _pg_ripple.endpoint_stats
                WHERE endpoint_url = $1
                  AND fetched_at > now() - ($2 || ' seconds')::interval
             )",
            &[
                DatumWithOid::from(url),
                DatumWithOid::from(ttl_secs.to_string().as_str()),
            ],
        )
        .unwrap_or(None)
        .unwrap_or(false);

        if fresh {
            return;
        }
    }

    // Attempt to fetch VoID statistics via simple SPARQL query.
    let stats = fetch_void_stats(url);

    // Upsert into endpoint_stats.
    // JSON-01 (v0.74.0): column renamed predicate_stats_json → predicate_stats (JSONB).
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.endpoint_stats
             (endpoint_url, total_triples, predicate_stats, distinct_subjects,
              distinct_objects, fetched_at)
         VALUES ($1, $2, $3::jsonb, $4, $5, now())
         ON CONFLICT (endpoint_url) DO UPDATE
             SET total_triples       = EXCLUDED.total_triples,
                 predicate_stats     = EXCLUDED.predicate_stats,
                 distinct_subjects   = EXCLUDED.distinct_subjects,
                 distinct_objects    = EXCLUDED.distinct_objects,
                 fetched_at          = EXCLUDED.fetched_at",
        &[
            DatumWithOid::from(url),
            DatumWithOid::from(stats.total_triples),
            DatumWithOid::from(
                serde_json::to_string(&stats.predicate_triples)
                    .unwrap_or_else(|_| "{}".to_owned())
                    .as_str(),
            ),
            DatumWithOid::from(stats.distinct_subjects),
            DatumWithOid::from(stats.distinct_objects),
        ],
    )
    .unwrap_or_else(|e| {
        pgrx::log!("federation_planner: failed to upsert endpoint_stats for {url}: {e}");
    });
}

/// Fetch VoID statistics from a remote SPARQL endpoint.
///
/// Queries the endpoint for:
/// 1. Total triple count via `SELECT (COUNT(*) AS ?c) WHERE { ?s ?p ?o }`
/// 2. Per-predicate counts via `SELECT ?p (COUNT(*) AS ?c) WHERE { ?s ?p ?o } GROUP BY ?p`
///
/// Falls back to defaults (0) on any error — failures are logged but not fatal.
fn fetch_void_stats(url: &str) -> EndpointStats {
    let mut stats = EndpointStats::default();

    // Check if the endpoint is reachable at all (respect the allowlist).
    if !crate::sparql::federation::is_endpoint_allowed(url) {
        return stats;
    }

    // Query for total triple count.
    let count_query = "SELECT (COUNT(*) AS ?c) WHERE { ?s ?p ?o }";
    if let Ok(rows) = execute_count_query(url, count_query)
        && let Some(count) = rows.first()
    {
        stats.total_triples = *count;
    }

    // Query for per-predicate counts (limit to top 100 predicates to avoid timeouts).
    let pred_query =
        "SELECT ?p (COUNT(*) AS ?c) WHERE { ?s ?p ?o } GROUP BY ?p ORDER BY DESC(?c) LIMIT 100";
    if let Ok(pred_counts) = execute_predicate_count_query(url, pred_query) {
        stats.predicate_triples = pred_counts;
    }

    // Estimate distinct subjects/objects from total (rough approximation when VoID not available).
    if stats.total_triples > 0 {
        // Heuristic: ~20% of triple count as distinct subjects/objects.
        stats.distinct_subjects = (stats.total_triples / 5).max(1);
        stats.distinct_objects = (stats.total_triples / 3).max(1);
    }

    stats
}

/// Execute a single-column COUNT query against a remote endpoint.
/// Returns the count values as a Vec<i64>.
fn execute_count_query(url: &str, sparql: &str) -> Result<Vec<i64>, String> {
    let timeout = std::time::Duration::from_secs(
        crate::FEDERATION_PARALLEL_TIMEOUT.get().clamp(1, 300) as u64,
    );
    let pool_size = crate::FEDERATION_POOL_SIZE.get().clamp(1, 32) as usize;
    let agent = crate::sparql::federation::get_agent_pub(timeout, pool_size);

    let body = agent
        .post(url)
        .set("Accept", "application/sparql-results+json")
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&format!("query={}", urlencoding_encode(sparql)))
        .map_err(|e| format!("HTTP error: {e}"))?
        .into_string()
        .map_err(|e| format!("Body read error: {e}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("JSON parse error: {e}"))?;

    let mut results = Vec::new();
    if let Some(bindings) = json["results"]["bindings"].as_array() {
        for binding in bindings {
            for key in ["c", "count", "total"] {
                if let Some(val) = binding[key]["value"].as_str()
                    && let Ok(n) = val.parse::<i64>()
                {
                    results.push(n);
                    break;
                }
            }
        }
    }
    Ok(results)
}

/// Execute a predicate-count query and return a map of predicate IRI → count.
fn execute_predicate_count_query(url: &str, sparql: &str) -> Result<HashMap<String, i64>, String> {
    let timeout = std::time::Duration::from_secs(
        crate::FEDERATION_PARALLEL_TIMEOUT.get().clamp(1, 300) as u64,
    );
    let pool_size = crate::FEDERATION_POOL_SIZE.get().clamp(1, 32) as usize;
    let agent = crate::sparql::federation::get_agent_pub(timeout, pool_size);

    let body = agent
        .post(url)
        .set("Accept", "application/sparql-results+json")
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&format!("query={}", urlencoding_encode(sparql)))
        .map_err(|e| format!("HTTP error: {e}"))?
        .into_string()
        .map_err(|e| format!("Body read error: {e}"))?;

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("JSON parse error: {e}"))?;

    let mut map = HashMap::new();
    if let Some(bindings) = json["results"]["bindings"].as_array() {
        for binding in bindings {
            let pred = binding["p"]["value"].as_str().unwrap_or("").to_string();
            let count: i64 = binding["c"]["value"]
                .as_str()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if !pred.is_empty() && count > 0 {
                map.insert(pred, count);
            }
        }
    }
    Ok(map)
}

/// Load cached VoID statistics for a given endpoint URL from the database.
#[allow(dead_code)] // v0.56.0 audit: planned API, not yet wired into query planner
pub fn load_endpoint_stats(url: &str) -> EndpointStats {
    let mut stats = EndpointStats::default();

    Spi::connect(|c| {
        let result = c.select(
            "SELECT total_triples, predicate_stats::text, distinct_subjects, distinct_objects
             FROM _pg_ripple.endpoint_stats
             WHERE endpoint_url = $1",
            None,
            &[DatumWithOid::from(url)],
        );
        if let Ok(mut rows) = result
            && let Some(row) = rows.next()
        {
            stats.total_triples = row.get::<i64>(1).ok().flatten().unwrap_or(0);
            let pred_json: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
            if !pred_json.is_empty()
                && let Ok(map) = serde_json::from_str::<HashMap<String, i64>>(&pred_json)
            {
                stats.predicate_triples = map;
            }
            stats.distinct_subjects = row.get::<i64>(3).ok().flatten().unwrap_or(0);
            stats.distinct_objects = row.get::<i64>(4).ok().flatten().unwrap_or(0);
        }
    });

    stats
}

// ─── Cost-based source selection ─────────────────────────────────────────────

/// For a given predicate IRI, estimate the number of triples at `url` using
/// cached VoID statistics.  Falls back to `stats.total_triples` when no
/// per-predicate count is available.
#[allow(dead_code)] // v0.56.0 audit: planned API, not yet wired into query planner
pub fn estimate_selectivity(url: &str, predicate_iri: Option<&str>) -> i64 {
    let stats = load_endpoint_stats(url);
    if stats.total_triples == 0 {
        // No statistics — assign a default cost of 1,000,000 (effectively "unknown").
        return 1_000_000;
    }
    match predicate_iri {
        Some(p) => *stats
            .predicate_triples
            .get(p)
            .unwrap_or(&stats.total_triples),
        None => stats.total_triples,
    }
}

/// Rank registered endpoints for a given predicate IRI.
///
/// Returns a `Vec<(url, estimated_selectivity)>` sorted ascending by
/// selectivity (fewest estimated triples first = most selective = execute first).
#[allow(dead_code)] // v0.56.0 audit: planned API, not yet wired into query planner
pub fn rank_endpoints_for_predicate(predicate_iri: Option<&str>) -> Vec<(String, i64)> {
    // Collect all registered + enabled endpoint URLs.
    let urls: Vec<String> = Spi::connect(|c| {
        c.select(
            "SELECT url FROM _pg_ripple.federation_endpoints WHERE enabled = true",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("rank_endpoints: SPI error: {e}"))
        .filter_map(|row| row.get::<String>(1).ok().flatten())
        .collect()
    });

    let mut ranked: Vec<(String, i64)> = urls
        .iter()
        .map(|url| {
            let sel = estimate_selectivity(url, predicate_iri);
            (url.clone(), sel)
        })
        .collect();

    ranked.sort_by_key(|(_, sel)| *sel);
    ranked
}

// ─── Parallel execution scheduling ───────────────────────────────────────────

/// Determine which groups of SERVICE endpoints can be executed in parallel.
///
/// Two SERVICE calls are independent when they share no output variable names.
/// Returns a list of groups; calls within the same group can run concurrently;
/// groups themselves must run sequentially (to preserve join semantics).
///
/// # Arguments
/// - `services`: list of `(url, variables)` pairs where `variables` is the set
///   of projected variable names for that SERVICE call.
#[allow(dead_code)] // v0.56.0 audit: planned API, not yet wired into parallel executor
pub fn compute_parallel_groups(services: &[(String, Vec<String>)]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    // Greedy algorithm: assign each SERVICE to the first group where it shares
    // no variables with existing members.
    for (idx, (_, vars)) in services.iter().enumerate() {
        let var_set: std::collections::HashSet<&String> = vars.iter().collect();
        let mut placed = false;
        for group in &mut groups {
            // Check whether this SERVICE shares any variable with any member of the group.
            let conflict = group.iter().any(|&i| {
                let (_, gvars) = &services[i];
                gvars.iter().any(|v| var_set.contains(v))
            });
            if !conflict {
                group.push(idx);
                placed = true;
                break;
            }
        }
        if !placed {
            groups.push(vec![idx]);
        }
    }
    groups
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// URL-encode a string for use in `application/x-www-form-urlencoded` bodies.
fn urlencoding_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            b => {
                out.push('%');
                out.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
                out.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('0'));
            }
        }
    }
    out
}

// ─── Public SQL API ───────────────────────────────────────────────────────────

/// Manually trigger a VoID statistics refresh for a registered endpoint (v0.42.0).
///
/// Called from `federation_registry.rs` on endpoint registration and from
/// `pg_ripple.refresh_federation_stats(url)`.
#[pgrx::pg_schema]
mod pg_ripple {
    use pgrx::prelude::*;

    /// Refresh cached VoID statistics for a registered federation endpoint.
    ///
    /// Forces a re-fetch from the remote endpoint regardless of TTL.
    /// Returns TRUE if the refresh succeeded, FALSE if the endpoint is
    /// unreachable or returns no useful statistics.
    #[pg_extern]
    fn refresh_federation_stats(url: &str) -> bool {
        // Temporarily clear the cache entry so refresh_endpoint_stats re-fetches.
        let _ = pgrx::Spi::run_with_args(
            "DELETE FROM _pg_ripple.endpoint_stats WHERE endpoint_url = $1",
            &[pgrx::datum::DatumWithOid::from(url)],
        );
        super::refresh_endpoint_stats(url);
        // Return true if we now have stats.
        pgrx::Spi::get_one_with_args::<bool>(
            "SELECT EXISTS(SELECT 1 FROM _pg_ripple.endpoint_stats WHERE endpoint_url = $1)",
            &[pgrx::datum::DatumWithOid::from(url)],
        )
        .unwrap_or(None)
        .unwrap_or(false)
    }

    /// List VoID statistics for all registered federation endpoints (v0.42.0).
    #[pg_extern]
    fn list_federation_stats() -> pgrx::iter::TableIterator<
        'static,
        (
            pgrx::name!(endpoint_url, String),
            pgrx::name!(total_triples, i64),
            pgrx::name!(distinct_subjects, i64),
            pgrx::name!(distinct_objects, i64),
            pgrx::name!(fetched_at, Option<pgrx::datum::TimestampWithTimeZone>),
        ),
    > {
        let mut rows: Vec<(
            String,
            i64,
            i64,
            i64,
            Option<pgrx::datum::TimestampWithTimeZone>,
        )> = Vec::new();
        pgrx::Spi::connect(|c| {
            let result = c
                .select(
                    "SELECT endpoint_url, total_triples, distinct_subjects, \
                     distinct_objects, fetched_at \
                     FROM _pg_ripple.endpoint_stats ORDER BY endpoint_url",
                    None,
                    &[],
                )
                .unwrap_or_else(|e| pgrx::error!("list_federation_stats SPI error: {e}"));
            for row in result {
                let url: String = row.get(1).ok().flatten().unwrap_or_default();
                let tt: i64 = row.get(2).ok().flatten().unwrap_or(0);
                let ds: i64 = row.get(3).ok().flatten().unwrap_or(0);
                let dobj: i64 = row.get(4).ok().flatten().unwrap_or(0);
                let ts: Option<pgrx::datum::TimestampWithTimeZone> = row.get(5).ok().flatten();
                rows.push((url, tt, ds, dobj, ts));
            }
        });
        pgrx::iter::TableIterator::new(rows)
    }
}
