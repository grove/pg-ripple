//! Storage ops: scan, query, graph management, SID API (M15-13, v0.96.0).
//! Moved from storage/ops/mod.rs lines 492-1615.
//! v0.122.0 H17-02: deduplication helpers extracted to `dedup.rs`.
// @allow-large-file: scan and SID operations remain consolidated for shared query paths

#[path = "dedup.rs"]
pub(super) mod dedup;
pub use dedup::{deduplicate_all, deduplicate_predicate, statement_id_for_triple};

use super::super::dictionary_io::{encode_rdf_term, strip_angle_brackets};
use super::super::mutation_journal;
use super::super::vp_rare_io::{get_dedicated_vp_table, scan_vp_rare, scan_vp_table};
use crate::dictionary;
use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;

use super::insert_encoded_triple;

/// Return the sum of `triple_count` across all predicate catalog entries.
pub fn total_triple_count() -> i64 {
    Spi::get_one::<i64>("SELECT COALESCE(SUM(triple_count), 0)::bigint FROM _pg_ripple.predicates")
        .unwrap_or_else(|e| pgrx::error!("triple_count SPI error: {e}"))
        .unwrap_or(0)
}

/// Return the number of triples in a specific named graph.
pub fn triple_count_in_graph(g_id: i64) -> i64 {
    let mut total = 0i64;

    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("predicates scan SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        let cnt = Spi::get_one_with_args::<i64>(
            &format!("SELECT count(*)::bigint FROM {table} WHERE g = $1"),
            &[DatumWithOid::from(g_id)],
        )
        .unwrap_or(None)
        .unwrap_or(0);
        total += cnt;
    }

    let rare_cnt = Spi::get_one_with_args::<i64>(
        "SELECT count(*)::bigint FROM _pg_ripple.vp_rare WHERE g = $1",
        &[DatumWithOid::from(g_id)],
    )
    .unwrap_or(None)
    .unwrap_or(0);
    total += rare_cnt;

    total
}

/// Find triples matching the supplied pattern (includes vp_rare).
///
/// Any argument may be `None` to act as a wildcard.  Returns decoded text tuples
/// `(s, p, o, g)` in the default graph unless `graph` is supplied.
pub fn find_triples(
    s: Option<&str>,
    p: Option<&str>,
    o: Option<&str>,
    graph: Option<i64>,
) -> Vec<(String, String, String, String)> {
    let g = graph.unwrap_or(0);
    let mut results = Vec::new();

    let s_id = s.map(encode_rdf_term);
    let o_id = o.map(encode_rdf_term);

    if let Some(p_str) = p {
        let p_id = dictionary::encode(strip_angle_brackets(p_str), dictionary::KIND_IRI);

        // Check dedicated VP table.
        if let Some(table) = get_dedicated_vp_table(p_id) {
            results.extend(scan_vp_table(&table, p_id, s_id, o_id, g));
        }
        // Also check vp_rare.
        results.extend(scan_vp_rare(Some(p_id), s_id, o_id, g));
    } else {
        // Scan all dedicated VP tables.
        let pred_ids: Vec<i64> = Spi::connect(|c| {
            c.select(
                "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("predicates scan SPI error: {e}"))
            .filter_map(|row| row.get::<i64>(1).ok().flatten())
            .collect()
        });

        for pid in pred_ids {
            let table = format!("_pg_ripple.vp_{pid}");
            results.extend(scan_vp_table(&table, pid, s_id, o_id, g));
        }

        // Scan vp_rare for remaining triples.
        results.extend(scan_vp_rare(None, s_id, o_id, g));
    }

    results
}

/// Collect all (s_id, p_id, o_id, g_id) from all VP tables (for export).
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[allow(dead_code)]
pub fn all_encoded_triples(graph: Option<i64>) -> Vec<(i64, i64, i64, i64)> {
    let mut results = Vec::new();

    // Dedicated VP tables.
    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("predicates scan SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        let g_filter = match graph {
            Some(gid) => format!(" WHERE g = {}", gid),
            None => String::new(),
        };
        let sql = format!("SELECT s, o, g FROM {table}{g_filter}");
        let rows: Vec<(i64, i64, i64)> = Spi::connect(|c| {
            c.select(&sql, None, &[])
                .unwrap_or_else(|e| pgrx::error!("all_encoded_triples VP scan SPI error: {e}"))
                .filter_map(|row| {
                    let s: Option<i64> = row.get(1).ok().flatten();
                    let o: Option<i64> = row.get(2).ok().flatten();
                    let g: Option<i64> = row.get(3).ok().flatten();
                    match (s, o, g) {
                        (Some(s), Some(o), Some(g)) => Some((s, o, g)),
                        _ => None,
                    }
                })
                .collect()
        });
        for (s, o, g_val) in rows {
            results.push((s, p_id, o, g_val));
        }
    }

    // vp_rare.
    let g_filter = match graph {
        Some(gid) => format!(" WHERE g = {}", gid),
        None => String::new(),
    };
    let sql = format!("SELECT p, s, o, g FROM _pg_ripple.vp_rare{g_filter}");
    let rare_rows: Vec<(i64, i64, i64, i64)> = Spi::connect(|c| {
        c.select(&sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("all_encoded_triples vp_rare scan SPI error: {e}"))
            .filter_map(|row| {
                let p: Option<i64> = row.get(1).ok().flatten();
                let s: Option<i64> = row.get(2).ok().flatten();
                let o: Option<i64> = row.get(3).ok().flatten();
                let g: Option<i64> = row.get(4).ok().flatten();
                match (p, s, o, g) {
                    (Some(p), Some(s), Some(o), Some(g)) => Some((s, p, o, g)),
                    _ => None,
                }
            })
            .collect()
    });
    results.extend(rare_rows);

    results
}

/// Iterate over all encoded triples in batches using cursor-based streaming.
///
/// Calls `callback` for each batch of `(s_id, p_id, o_id, g_id)` tuples.
/// The batch size is controlled by `pg_ripple.export_batch_size` (default 10 000).
///
/// This avoids loading the entire graph into a single Rust `Vec`, which can
/// consume many GiB of memory for large stores.
///
/// # Parameters
/// - `graph`: optional graph filter (None = all graphs)
/// - `callback`: called once per batch with a slice of `(s, p, o, g)` tuples
#[allow(clippy::type_complexity)]
pub fn for_each_encoded_triple_batch(
    graph: Option<i64>,
    callback: &mut dyn FnMut(&[(i64, i64, i64, i64)]), // (s, p, o, g)
) {
    let batch_size = crate::EXPORT_BATCH_SIZE.get() as usize;

    // ── Dedicated VP tables ───────────────────────────────────────────────────
    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("predicates scan SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        // Keyset pagination keeps each page indexable even when a graph has
        // millions of triples; OFFSET would rescan and discard every prior page.
        let mut last_i = None;
        loop {
            let mut filters = Vec::new();
            if let Some(gid) = graph {
                filters.push(format!("g = {gid}"));
            }
            if let Some(i) = last_i {
                filters.push(format!("i > {i}"));
            }
            let where_clause = if filters.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", filters.join(" AND "))
            };
            let sql = format!(
                "SELECT s, o, g, i FROM {table}{where_clause} ORDER BY i LIMIT {batch_size}"
            );
            let page: Vec<(i64, i64, i64, i64, i64)> = Spi::connect(|c| {
                c.select(&sql, None, &[])
                    .unwrap_or_else(|e| {
                        pgrx::error!("for_each_encoded_triple_batch VP scan SPI error: {e}")
                    })
                    .filter_map(|row| {
                        let s: Option<i64> = row.get(1).ok().flatten();
                        let o: Option<i64> = row.get(2).ok().flatten();
                        let g: Option<i64> = row.get(3).ok().flatten();
                        let i: Option<i64> = row.get(4).ok().flatten();
                        match (s, o, g, i) {
                            (Some(s), Some(o), Some(g), Some(i)) => Some((s, p_id, o, g, i)),
                            _ => None,
                        }
                    })
                    .collect()
            });
            let page_len = page.len();
            if !page.is_empty() {
                let triples: Vec<_> = page.iter().map(|&(s, p, o, g, _)| (s, p, o, g)).collect();
                callback(&triples);
                last_i = page.last().map(|row| row.4);
            }
            if page_len < batch_size {
                break;
            }
        }
    }

    // ── vp_rare ───────────────────────────────────────────────────────────────
    let mut last_i = None;
    loop {
        let mut filters = Vec::new();
        if let Some(gid) = graph {
            filters.push(format!("g = {gid}"));
        }
        if let Some(i) = last_i {
            filters.push(format!("i > {i}"));
        }
        let where_clause = if filters.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", filters.join(" AND "))
        };
        let sql = format!(
            "SELECT p, s, o, g, i FROM _pg_ripple.vp_rare{where_clause} ORDER BY i LIMIT {batch_size}"
        );
        let page: Vec<(i64, i64, i64, i64, i64)> = Spi::connect(|c| {
            c.select(&sql, None, &[])
                .unwrap_or_else(|e| {
                    pgrx::error!("for_each_encoded_triple_batch vp_rare scan SPI error: {e}")
                })
                .filter_map(|row| {
                    let p: Option<i64> = row.get(1).ok().flatten();
                    let s: Option<i64> = row.get(2).ok().flatten();
                    let o: Option<i64> = row.get(3).ok().flatten();
                    let g: Option<i64> = row.get(4).ok().flatten();
                    let i: Option<i64> = row.get(5).ok().flatten();
                    match (p, s, o, g, i) {
                        (Some(p), Some(s), Some(o), Some(g), Some(i)) => Some((s, p, o, g, i)),
                        _ => None,
                    }
                })
                .collect()
        });
        let page_len = page.len();
        if !page.is_empty() {
            let triples: Vec<_> = page.iter().map(|&(s, p, o, g, _)| (s, p, o, g)).collect();
            callback(&triples);
            last_i = page.last().map(|row| row.4);
        }
        if page_len < batch_size {
            break;
        }
    }
}

/// Encode a named graph IRI and return its dictionary id.
/// This is idempotent — calling it again returns the same id.
pub fn create_graph(graph_iri: &str) -> i64 {
    dictionary::encode(strip_angle_brackets(graph_iri), dictionary::KIND_IRI)
}

/// Clear all triples in a named or default graph (identified by `g_id`).
/// Like `drop_graph` but operates by numeric graph ID.  Returns triples deleted.
pub fn clear_graph_by_id(g_id: i64) -> i64 {
    let mut deleted = 0i64;

    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("predicates scan SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    for p_id in pred_ids {
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        let tombs = format!("_pg_ripple.vp_{p_id}_tombstones");
        let main_t = format!("_pg_ripple.vp_{p_id}_main");

        let d_delta = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH d AS (DELETE FROM {delta} WHERE g = $1 RETURNING 1) \
                 SELECT count(*)::bigint FROM d"
            ),
            &[DatumWithOid::from(g_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("clear_graph_by_id delta delete SPI error: {e}"))
        .unwrap_or(0);

        let d_main = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH ins AS ( \
                     INSERT INTO {tombs} (s, o, g) \
                     SELECT s, o, g FROM {main_t} WHERE g = $1 \
                     ON CONFLICT DO NOTHING \
                     RETURNING 1 \
                 ) SELECT count(*)::bigint FROM ins"
            ),
            &[DatumWithOid::from(g_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("clear_graph_by_id tombstones SPI error: {e}"))
        .unwrap_or(0);

        let d = d_delta + d_main;
        if d > 0 {
            Spi::run_with_args(
                "UPDATE _pg_ripple.predicates \
                 SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
                &[DatumWithOid::from(p_id), DatumWithOid::from(d)],
            )
            .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
            deleted += d;
        }

        // M15-05 (v0.96.0): if tombstones were added to main, update tombstone_count.
        // If prior count was 0 the view may be in tombstone-skip form; switch back.
        if d_main > 0 {
            let prev_count: i64 = Spi::get_one_with_args::<i64>(
                "UPDATE _pg_ripple.predicates \
                 SET tombstone_count = tombstone_count + $2 \
                 WHERE id = $1 \
                 RETURNING tombstone_count - $2",
                &[DatumWithOid::from(p_id), DatumWithOid::from(d_main)],
            )
            .unwrap_or(None)
            .unwrap_or(1);
            if prev_count == 0 {
                crate::storage::merge::rebuild_htap_view(p_id, true);
            }
        }
    }

    let d = Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.vp_rare WHERE g = $1 RETURNING p) \
         SELECT count(*)::bigint FROM d",
        &[DatumWithOid::from(g_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("clear_graph_by_id vp_rare delete SPI error: {e}"))
    .unwrap_or(0);
    deleted += d;

    deleted
}

/// Collect all distinct graph IDs currently in the store (including default graph 0).
pub fn all_graph_ids() -> Vec<i64> {
    let mut g_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();

    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("predicates scan SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    for p_id in &pred_ids {
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        let main_t = format!("_pg_ripple.vp_{p_id}_main");
        Spi::connect(|c| {
            for row in c
                .select(&format!("SELECT DISTINCT g FROM {delta}"), None, &[])
                .unwrap_or_else(|e| pgrx::error!("all_graph_ids delta scan: {e}"))
            {
                if let Some(g) = row.get::<i64>(1).ok().flatten() {
                    g_ids.insert(g);
                }
            }
            for row in c
                .select(&format!("SELECT DISTINCT g FROM {main_t}"), None, &[])
                .unwrap_or_else(|e| pgrx::error!("all_graph_ids main scan: {e}"))
            {
                if let Some(g) = row.get::<i64>(1).ok().flatten() {
                    g_ids.insert(g);
                }
            }
        });
    }

    Spi::connect(|c| {
        for row in c
            .select("SELECT DISTINCT g FROM _pg_ripple.vp_rare", None, &[])
            .unwrap_or_else(|e| pgrx::error!("all_graph_ids vp_rare scan: {e}"))
        {
            if let Some(g) = row.get::<i64>(1).ok().flatten() {
                g_ids.insert(g);
            }
        }
    });

    g_ids.into_iter().collect()
}

/// Drop all triples in a named graph.  Returns the number of triples deleted.
pub fn drop_graph(graph_iri: &str) -> i64 {
    let g_id = dictionary::encode(strip_angle_brackets(graph_iri), dictionary::KIND_IRI);

    let mut deleted = 0i64;

    // Delete from all dedicated VP tables.
    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("predicates scan SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    for p_id in pred_ids {
        // For HTAP split: delete from delta + add tombstones for main rows.
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        let tombs = format!("_pg_ripple.vp_{p_id}_tombstones");
        let main_t = format!("_pg_ripple.vp_{p_id}_main");

        // Delete from delta.
        let d_delta = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH d AS (DELETE FROM {delta} WHERE g = $1 RETURNING 1) \
                 SELECT count(*)::bigint FROM d"
            ),
            &[DatumWithOid::from(g_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("drop_graph delta delete SPI error: {e}"))
        .unwrap_or(0);

        // Add tombstones for main rows (to suppress them from the view).
        let d_main = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH ins AS ( \
                     INSERT INTO {tombs} (s, o, g) \
                     SELECT s, o, g FROM {main_t} WHERE g = $1 \
                     ON CONFLICT DO NOTHING \
                     RETURNING 1 \
                 ) SELECT count(*)::bigint FROM ins"
            ),
            &[DatumWithOid::from(g_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("drop_graph tombstones SPI error: {e}"))
        .unwrap_or(0);

        let d = d_delta + d_main;
        if d > 0 {
            Spi::run_with_args(
                "UPDATE _pg_ripple.predicates \
                 SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
                &[DatumWithOid::from(p_id), DatumWithOid::from(d)],
            )
            .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
            deleted += d;
        }
    }

    // Delete from vp_rare.
    let d = Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.vp_rare WHERE g = $1 RETURNING p) \
         SELECT count(*)::bigint FROM d",
        &[DatumWithOid::from(g_id)],
    )
    .unwrap_or_else(|e| pgrx::error!("drop_graph vp_rare delete SPI error: {e}"))
    .unwrap_or(0);
    deleted += d;

    deleted
}

/// List all named graph IRIs (excludes the default graph 0).
pub fn list_graphs() -> Vec<String> {
    // Collect distinct g values > 0 from all VP tables and vp_rare, decode them.
    let mut g_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();

    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("predicates scan SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        let ids: Vec<i64> = Spi::connect(|c| {
            c.select(
                &format!("SELECT DISTINCT g FROM {table} WHERE g > 0"),
                None,
                &[],
            )
            .unwrap_or_else(|e| pgrx::error!("list_graphs VP scan SPI error: {e}"))
            .filter_map(|row| row.get::<i64>(1).ok().flatten())
            .collect()
        });
        for id in ids {
            g_ids.insert(id);
        }
    }

    let rare_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT DISTINCT g FROM _pg_ripple.vp_rare WHERE g > 0",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("list_graphs vp_rare scan SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });
    for id in rare_ids {
        g_ids.insert(id);
    }

    let mut graphs: Vec<String> = g_ids
        .into_iter()
        .filter_map(dictionary::decode)
        .map(|iri| format!("<{}>", iri))
        .collect();
    graphs.sort();
    graphs
}

// ─── IRI Prefix Management ────────────────────────────────────────────────────

/// Register (or update) an IRI prefix abbreviation.
pub fn register_prefix(prefix: &str, expansion: &str) {
    validate_prefix(prefix, expansion);
    ensure_prefix_mutation_authorized();
    Spi::run_with_args(
        "INSERT INTO _pg_ripple.prefixes (prefix, expansion, owner_oid) VALUES ($1, $2, (current_user::regrole)::oid) \
         ON CONFLICT (prefix) DO UPDATE SET expansion = EXCLUDED.expansion, updated_at = now()",
        &[DatumWithOid::from(prefix), DatumWithOid::from(expansion)],
    )
    .unwrap_or_else(|e| pgrx::error!("register_prefix SPI error: {e}"));
    bump_prefix_generation();
}

/// Drop a registered prefix, returning whether a row was removed.
pub fn drop_prefix(prefix: &str) -> bool {
    validate_prefix_label(prefix);
    ensure_prefix_mutation_authorized();
    let removed = Spi::get_one_with_args::<bool>(
        "WITH deleted AS (DELETE FROM _pg_ripple.prefixes WHERE prefix = $1 RETURNING 1) \
         SELECT EXISTS (SELECT 1 FROM deleted)",
        &[DatumWithOid::from(prefix)],
    )
    .unwrap_or_else(|e| pgrx::error!("drop_prefix SPI error: {e}"))
    .unwrap_or(false);
    if removed {
        bump_prefix_generation();
    }
    removed
}

/// Return all registered prefix → expansion pairs.
pub fn list_prefixes() -> Vec<(String, String)> {
    Spi::connect(|c| {
        c.select(
            "SELECT prefix, expansion FROM _pg_ripple.prefixes ORDER BY prefix",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("list_prefixes SPI error: {e}"))
        .filter_map(|row| {
            let prefix: Option<String> = row.get(1).ok().flatten();
            let expansion: Option<String> = row.get(2).ok().flatten();
            match (prefix, expansion) {
                (Some(p), Some(e)) => Some((p, e)),
                _ => None,
            }
        })
        .collect()
    })
}

/// Return the committed registry generation visible to this transaction.
pub fn prefix_registry_generation() -> i64 {
    Spi::get_one::<i64>("SELECT generation FROM _pg_ripple.prefix_registry_state WHERE singleton")
        .unwrap_or_else(|e| pgrx::error!("prefix registry generation SPI error: {e}"))
        .unwrap_or(1)
}

fn bump_prefix_generation() {
    Spi::run(
        "UPDATE _pg_ripple.prefix_registry_state SET generation = generation + 1 WHERE singleton",
    )
    .unwrap_or_else(|e| pgrx::error!("prefix registry generation update error: {e}"));
}

fn validate_prefix(prefix: &str, expansion: &str) {
    validate_prefix_label(prefix);
    let bytes = expansion.as_bytes();
    let scheme_end = expansion.find(':').unwrap_or(0);
    if scheme_end == 0
        || !expansion[..scheme_end].chars().enumerate().all(|(i, c)| {
            if i == 0 {
                c.is_ascii_alphabetic()
            } else {
                c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
            }
        })
        || expansion.chars().any(|c| {
            c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | '\'' | ';')
        })
    {
        pgrx::error!("PT0577: invalid prefix expansion IRI");
    }
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            pgrx::error!("PT0577: invalid prefix expansion IRI");
        }
        i += 1;
    }
}

fn validate_prefix_label(prefix: &str) {
    let mut chars = prefix.chars();
    let valid = match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {
            !prefix.ends_with('.')
                && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        }
        _ => false,
    };
    if !valid {
        pgrx::error!("PT0577: invalid prefix label");
    }
}

fn ensure_prefix_mutation_authorized() {
    let allowed = Spi::get_one::<bool>(
        "SELECT r.rolsuper OR current_user = r.rolname OR \
         EXISTS (SELECT 1 FROM pg_roles admin WHERE admin.rolname = 'pg_ripple_admin' AND pg_has_role(current_user, admin.oid, 'member')) \
         FROM pg_database d JOIN pg_roles r ON r.oid = d.datdba WHERE d.datname = current_database()",
    )
    .unwrap_or_else(|e| pgrx::error!("prefix authorization SPI error: {e}"))
    .unwrap_or(false);
    if !allowed {
        pgrx::error!("PT0578: prefix mutation is not authorized");
    }
}

// ─── Statement Identifier API (v0.4.0) ────────────────────────────────────────

/// Look up a statement by its globally-unique statement identifier (SID).
///
/// Searches the `_pg_ripple.statements` range-mapping catalog first, then
/// falls back to a brute-force scan if the catalog is empty.
/// Returns decoded N-Triples–formatted `(s, p, o, g)` strings, or `None`.
pub fn get_statement_by_sid(sid: i64) -> Option<(String, String, String, String)> {
    // Try the range mapping catalog first (fast path).
    let pred_from_catalog: Option<i64> = Spi::connect(|c| {
        c.select(
            "SELECT predicate_id \
             FROM _pg_ripple.statements \
             WHERE sid_min <= $1 AND sid_max >= $1 \
             ORDER BY sid_min DESC LIMIT 1",
            Some(1),
            &[DatumWithOid::from(sid)],
        )
        .ok()
        .and_then(|rows| {
            rows.filter_map(|row| row.get::<i64>(1).ok().flatten())
                .next()
        })
    });

    if let Some(p_id) = pred_from_catalog {
        let table = format!("_pg_ripple.vp_{p_id}");
        if let Some((s_id, o_id, g_id)) = fetch_sog_by_sid(&table, sid) {
            return Some(decode_sog(s_id, p_id, o_id, g_id));
        }
    }

    // Fallback: scan all dedicated VP tables for the SID.
    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("predicates scan SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        if let Some((s_id, o_id, g_id)) = fetch_sog_by_sid(&table, sid) {
            return Some(decode_sog(s_id, p_id, o_id, g_id));
        }
    }

    // Also check vp_rare.
    Spi::connect(|c| {
        c.select(
            "SELECT s, p, o, g FROM _pg_ripple.vp_rare WHERE i = $1 LIMIT 1",
            Some(1),
            &[DatumWithOid::from(sid)],
        )
        .ok()
        .and_then(|rows| {
            rows.filter_map(|row| {
                let s = row.get::<i64>(1).ok().flatten()?;
                let p = row.get::<i64>(2).ok().flatten()?;
                let o = row.get::<i64>(3).ok().flatten()?;
                let g = row.get::<i64>(4).ok().flatten()?;
                Some(decode_sog(s, p, o, g))
            })
            .next()
        })
    })
}

/// Fetch `(s_id, o_id, g_id)` from a VP table by SID.
fn fetch_sog_by_sid(table: &str, sid: i64) -> Option<(i64, i64, i64)> {
    Spi::connect(|c| {
        c.select(
            &format!("SELECT s, o, g FROM {table} WHERE i = $1 LIMIT 1"),
            Some(1),
            &[DatumWithOid::from(sid)],
        )
        .ok()
        .and_then(|rows| {
            rows.filter_map(|row| {
                let s = row.get::<i64>(1).ok().flatten()?;
                let o = row.get::<i64>(2).ok().flatten()?;
                let g = row.get::<i64>(3).ok().flatten()?;
                Some((s, o, g))
            })
            .next()
        })
    })
}

/// Decode `(s_id, p_id, o_id, g_id)` to N-Triples strings.
fn decode_sog(s_id: i64, p_id: i64, o_id: i64, g_id: i64) -> (String, String, String, String) {
    (
        dictionary::format_ntriples(s_id),
        dictionary::format_ntriples(p_id),
        dictionary::format_ntriples(o_id),
        if g_id == 0 {
            String::new()
        } else {
            dictionary::format_ntriples(g_id)
        },
    )
}

// ─── v0.5.1 additions ─────────────────────────────────────────────────────────

/// Insert a triple by pre-encoded dictionary IDs.
/// Alias for `insert_encoded_triple` for use from the SPARQL Update executor.
/// # Callers
/// Direct callers must be the mutation journal flush function only.
pub(crate) fn insert_triple_by_ids(s_id: i64, p_id: i64, o_id: i64, g_id: i64) -> i64 {
    let sid = insert_encoded_triple(s_id, p_id, o_id, g_id);
    // MJOURNAL-01/02: record in mutation journal; flush deferred to
    // XACT_EVENT_PRE_COMMIT via xact_callback_c (FLUSH-01).
    mutation_journal::record_write(g_id);
    sid
}

/// Delete a triple by pre-encoded dictionary IDs.  Returns the number of deleted rows.
/// # Callers
/// Direct callers must be the mutation journal flush function only.
pub(crate) fn delete_triple_by_ids(s_id: i64, p_id: i64, o_id: i64, g_id: i64) -> i64 {
    let mut deleted = 0i64;

    // Try dedicated VP table (HTAP: delta first, then tombstone).
    if let Some(_view) = get_dedicated_vp_table(p_id) {
        let delta = format!("_pg_ripple.vp_{p_id}_delta");
        let tombs = format!("_pg_ripple.vp_{p_id}_tombstones");

        let d = Spi::get_one_with_args::<i64>(
            &format!(
                "WITH d AS (DELETE FROM {delta} WHERE s=$1 AND o=$2 AND g=$3 RETURNING 1) \
                 SELECT count(*)::bigint FROM d"
            ),
            &[
                DatumWithOid::from(s_id),
                DatumWithOid::from(o_id),
                DatumWithOid::from(g_id),
            ],
        )
        .unwrap_or_else(|e| pgrx::error!("delete_triple_by_ids delta SPI error: {e}"))
        .unwrap_or(0);

        if d > 0 {
            deleted += d;
        } else {
            // Add tombstone to suppress from main.
            crate::error::fault_injection::hit("tombstone_application");
            Spi::run_with_args(
                &format!(
                    "INSERT INTO {tombs} (s, o, g) VALUES ($1, $2, $3) \
                     ON CONFLICT DO NOTHING"
                ),
                &[
                    DatumWithOid::from(s_id),
                    DatumWithOid::from(o_id),
                    DatumWithOid::from(g_id),
                ],
            )
            .unwrap_or_else(|e| pgrx::error!("tombstone insert SPI error: {e}"));

            // M15-05 (v0.96.0): update tombstone_count.
            // If prior count was 0 the view may be in tombstone-skip form; switch back.
            let prev_count: i64 = Spi::get_one_with_args::<i64>(
                "UPDATE _pg_ripple.predicates \
                 SET tombstone_count = tombstone_count + 1 \
                 WHERE id = $1 \
                 RETURNING tombstone_count - 1",
                &[DatumWithOid::from(p_id)],
            )
            .unwrap_or(None)
            .unwrap_or(1);
            if prev_count == 0 {
                crate::storage::merge::rebuild_htap_view(p_id, true);
            }

            let in_main = Spi::get_one_with_args::<i64>(
                &format!(
                    "SELECT count(*)::bigint FROM _pg_ripple.vp_{p_id}_main \
                     WHERE s = $1 AND o = $2 AND g = $3"
                ),
                &[
                    DatumWithOid::from(s_id),
                    DatumWithOid::from(o_id),
                    DatumWithOid::from(g_id),
                ],
            )
            .unwrap_or(None)
            .unwrap_or(0);
            deleted += in_main;
        }

        if deleted > 0 {
            Spi::run_with_args(
                "UPDATE _pg_ripple.predicates \
                 SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
                &[DatumWithOid::from(p_id), DatumWithOid::from(deleted)],
            )
            .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
        }
    }

    // Also try vp_rare.
    let d = Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM _pg_ripple.vp_rare \
         WHERE p=$1 AND s=$2 AND o=$3 AND g=$4 RETURNING 1) \
         SELECT count(*)::bigint FROM d",
        &[
            DatumWithOid::from(p_id),
            DatumWithOid::from(s_id),
            DatumWithOid::from(o_id),
            DatumWithOid::from(g_id),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("vp_rare delete_by_ids SPI error: {e}"))
    .unwrap_or(0);
    if d > 0 {
        Spi::run_with_args(
            "UPDATE _pg_ripple.predicates \
             SET triple_count = GREATEST(0, triple_count - $2) WHERE id = $1",
            &[DatumWithOid::from(p_id), DatumWithOid::from(d)],
        )
        .unwrap_or_else(|e| pgrx::error!("predicate count update SPI error: {e}"));
        deleted += d;
    }

    // MJOURNAL-01/02: record deletion in mutation journal; flush deferred to
    // XACT_EVENT_PRE_COMMIT via xact_callback_c (FLUSH-01).
    if deleted > 0 {
        mutation_journal::record_delete(g_id);
        // CONF-GC-01a: cascade-delete confidence rows for any SID we just deleted.
        // We don't know the SID here, so we clean up orphan confidence rows lazily
        // via vacuum_confidence() or the next SHACL score computation.  For
        // explicit deletes we use a lightweight scan limited to the predicate VP table.
    }

    deleted
}

// LOAD_GEN_CACHE is defined in super (ops/mod.rs) as pub(super) static

/// Return the current load generation counter (used for blank-node scoping).
/// Session-local cache of the current load generation value.
/// Updated by both `next_load_generation()` and on first access by `current_load_generation()`.
///
/// Wraps `next_load_generation` but does NOT advance the generation — it just
/// reads the current in-session value.
pub fn current_load_generation() -> i64 {
    let g = super::LOAD_GEN_CACHE.load(std::sync::atomic::Ordering::Relaxed);
    if g == 0 {
        // Fetch from DB on first call.
        let g2 = Spi::get_one::<i64>("SELECT last_value FROM _pg_ripple.load_generation_seq")
            .ok()
            .flatten()
            .unwrap_or(1);
        super::LOAD_GEN_CACHE.store(g2, std::sync::atomic::Ordering::Relaxed);
        g2
    } else {
        g
    }
}

/// Return all `(predicate_id, object_id)` pairs where the given `subject_id`
/// appears as the subject.  Used by the CBD DESCRIBE algorithm.
pub fn triples_for_subject(subject_id: i64) -> Vec<(i64, i64)> {
    let mut result = Vec::new();

    // Scan all dedicated VP tables.
    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("describe predicates SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        let pairs: Vec<(i64, i64)> = Spi::connect(|c| {
            c.select(
                &format!("SELECT $1, o FROM {table} WHERE s = $2"),
                None,
                &[DatumWithOid::from(p_id), DatumWithOid::from(subject_id)],
            )
            .unwrap_or_else(|e| pgrx::error!("describe vp SPI error: {e}"))
            .filter_map(|row| {
                Some((
                    row.get::<i64>(1).ok().flatten()?,
                    row.get::<i64>(2).ok().flatten()?,
                ))
            })
            .collect()
        });
        result.extend(pairs);
    }

    // Also scan vp_rare.
    let rare_pairs: Vec<(i64, i64)> = Spi::connect(|c| {
        c.select(
            "SELECT p, o FROM _pg_ripple.vp_rare WHERE s = $1",
            None,
            &[DatumWithOid::from(subject_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("describe vp_rare SPI error: {e}"))
        .filter_map(|row| {
            Some((
                row.get::<i64>(1).ok().flatten()?,
                row.get::<i64>(2).ok().flatten()?,
            ))
        })
        .collect()
    });
    result.extend(rare_pairs);

    result
}

/// Return all `(subject_id, predicate_id)` pairs where the given `object_id`
/// appears as the object.  Used by the symmetric CBD DESCRIBE algorithm.
pub fn triples_for_object(object_id: i64) -> Vec<(i64, i64)> {
    let mut result = Vec::new();

    let pred_ids: Vec<i64> = Spi::connect(|c| {
        c.select(
            "SELECT id FROM _pg_ripple.predicates WHERE table_oid IS NOT NULL",
            None,
            &[],
        )
        .unwrap_or_else(|e| pgrx::error!("describe_incoming predicates SPI error: {e}"))
        .filter_map(|row| row.get::<i64>(1).ok().flatten())
        .collect()
    });

    for p_id in pred_ids {
        let table = format!("_pg_ripple.vp_{p_id}");
        let pairs: Vec<(i64, i64)> = Spi::connect(|c| {
            c.select(
                &format!("SELECT s, $1 FROM {table} WHERE o = $2"),
                None,
                &[DatumWithOid::from(p_id), DatumWithOid::from(object_id)],
            )
            .unwrap_or_else(|e| pgrx::error!("describe_incoming vp SPI error: {e}"))
            .filter_map(|row| {
                Some((
                    row.get::<i64>(1).ok().flatten()?,
                    row.get::<i64>(2).ok().flatten()?,
                ))
            })
            .collect()
        });
        result.extend(pairs);
    }

    let rare_pairs: Vec<(i64, i64)> = Spi::connect(|c| {
        c.select(
            "SELECT s, p FROM _pg_ripple.vp_rare WHERE o = $1",
            None,
            &[DatumWithOid::from(object_id)],
        )
        .unwrap_or_else(|e| pgrx::error!("describe_incoming vp_rare SPI error: {e}"))
        .filter_map(|row| {
            Some((
                row.get::<i64>(1).ok().flatten()?,
                row.get::<i64>(2).ok().flatten()?,
            ))
        })
        .collect()
    });
    result.extend(rare_pairs);

    result
}
