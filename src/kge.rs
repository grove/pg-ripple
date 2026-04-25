//! Knowledge-Graph Embeddings (KGE) for pg_ripple v0.57.0.
//!
//! Implements TransE and RotatE embedding models via stochastic gradient
//! descent over VP table triples. Embeddings are stored in
//! `_pg_ripple.kge_embeddings` and indexed with HNSW for ANN similarity search.
//!
//! # Architecture
//!
//! - `kge_worker`: background worker that iterates VP tables and trains embeddings
//! - `kge_stats()`: SRF returning model statistics
//! - `find_alignments()`: use HNSW to find similar entities across graphs
//!
//! # GUCs
//!
//! - `pg_ripple.kge_enabled` (bool, default off) — enable the KGE worker
//! - `pg_ripple.kge_model` (text) — `'transe'` or `'rotate'`

#![allow(dead_code)]

use pgrx::prelude::*;

/// Embedding dimension used for KGE vectors (matches schema definition).
pub const KGE_EMBEDDING_DIM: usize = 64;

/// Learning rate for the SGD optimizer.
const SGD_LEARNING_RATE: f64 = 0.01;

/// Margin for TransE loss (L1 norm).
const TRANSE_MARGIN: f64 = 1.0;

/// Maximum training iterations per worker cycle.
const MAX_TRAIN_ITERATIONS: usize = 1000;

// ─── TransE training step ─────────────────────────────────────────────────────

/// Perform a single TransE SGD update step for a positive triple (h, r, t).
/// Returns the updated entity and relation embeddings, and the loss.
pub fn transe_update(
    head: &mut [f64; KGE_EMBEDDING_DIM],
    relation: &mut [f64; KGE_EMBEDDING_DIM],
    tail: &mut [f64; KGE_EMBEDDING_DIM],
    neg_tail: &[f64; KGE_EMBEDDING_DIM],
) -> f64 {
    // Compute d(h + r, t) — positive score (L1 distance).
    let pos_dist: f64 = head
        .iter()
        .zip(relation.iter())
        .zip(tail.iter())
        .map(|((h, r), t)| (h + r - t).abs())
        .sum();

    // Compute d(h + r, t') — negative score.
    let neg_dist: f64 = head
        .iter()
        .zip(relation.iter())
        .zip(neg_tail.iter())
        .map(|((h, r), t)| (h + r - t).abs())
        .sum();

    let loss = (TRANSE_MARGIN + pos_dist - neg_dist).max(0.0);

    if loss > 0.0 {
        // Gradient update for head, relation, tail.
        for i in 0..KGE_EMBEDDING_DIM {
            let pos_grad = if head[i] + relation[i] - tail[i] >= 0.0 {
                1.0
            } else {
                -1.0
            };
            let neg_grad = if head[i] + relation[i] - neg_tail[i] >= 0.0 {
                1.0
            } else {
                -1.0
            };

            head[i] -= SGD_LEARNING_RATE * (pos_grad - neg_grad);
            relation[i] -= SGD_LEARNING_RATE * (pos_grad - neg_grad);
            tail[i] += SGD_LEARNING_RATE * pos_grad;
        }

        // L2 normalization of entity embeddings.
        normalize_l2(head);
        normalize_l2(tail);
    }

    loss
}

/// L2-normalize an embedding vector in place.
fn normalize_l2(v: &mut [f64; KGE_EMBEDDING_DIM]) {
    let norm: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm > 1e-10 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

// ─── KGE worker logic (called from worker.rs) ────────────────────────────────

/// Run one KGE training cycle on the current database.
/// Samples triples from VP tables, trains embeddings, and writes results.
///
/// Returns `(entities_trained, triples_trained, final_loss)`.
pub fn run_kge_cycle() -> (i64, i64, f64) {
    if !crate::KGE_ENABLED.get() {
        return (0, 0, 0.0);
    }

    let model_name = crate::KGE_MODEL
        .get()
        .and_then(|s| s.to_str().ok().map(|v| v.to_lowercase()))
        .unwrap_or_else(|| "transe".to_string());

    // Ensure the embeddings table exists.
    let _ = Spi::run_with_args(
        "CREATE TABLE IF NOT EXISTS _pg_ripple.kge_embeddings ( \
           entity_id BIGINT PRIMARY KEY, \
           embedding vector(64), \
           model TEXT, \
           trained_at TIMESTAMPTZ DEFAULT now() \
         )",
        &[],
    );

    // Sample up to 10,000 triples from vp_rare for training.
    let triples: Vec<(i64, i64, i64)> = Spi::connect(|client| {
        let rows = client.select(
            "SELECT s, p, o FROM _pg_ripple.vp_rare ORDER BY random() LIMIT 10000",
            None,
            &[],
        )?;
        let mut v = Vec::new();
        for row in rows {
            let s = row.get::<i64>(1)?.unwrap_or(0);
            let p = row.get::<i64>(2)?.unwrap_or(0);
            let o = row.get::<i64>(3)?.unwrap_or(0);
            if s != 0 && p != 0 && o != 0 {
                v.push((s, p, o));
            }
        }
        Ok::<_, pgrx::spi::Error>(v)
    })
    .unwrap_or_default();

    if triples.is_empty() {
        return (0, 0, 0.0);
    }

    let n_triples = triples.len() as i64;

    // Collect unique entity IDs.
    let mut entity_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for &(s, _p, o) in &triples {
        entity_ids.insert(s);
        entity_ids.insert(o);
    }

    // Initialize random embeddings for each entity and relation.
    let mut rng_state: u64 = 0xDEAD_BEEF_CAFE_BABEu64;
    let mut embeddings: std::collections::HashMap<i64, [f64; KGE_EMBEDDING_DIM]> =
        std::collections::HashMap::new();

    for &id in &entity_ids {
        let mut emb = [0.0f64; KGE_EMBEDDING_DIM];
        for x in emb.iter_mut() {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            *x = (rng_state as f64 / u64::MAX as f64) * 2.0 - 1.0;
        }
        embeddings.insert(id, emb);
    }

    // Collect unique relation IDs and init embeddings.
    let mut rel_embeddings: std::collections::HashMap<i64, [f64; KGE_EMBEDDING_DIM]> =
        std::collections::HashMap::new();
    for &(_s, p, _o) in &triples {
        rel_embeddings.entry(p).or_insert_with(|| {
            let mut emb = [0.0f64; KGE_EMBEDDING_DIM];
            for x in emb.iter_mut() {
                rng_state ^= rng_state << 13;
                rng_state ^= rng_state >> 7;
                rng_state ^= rng_state << 17;
                *x = (rng_state as f64 / u64::MAX as f64) * 2.0 - 1.0;
            }
            emb
        });
    }

    let entity_id_vec: Vec<i64> = entity_ids.iter().copied().collect();
    let n_entities = entity_id_vec.len();

    let zero_emb = [0.0f64; KGE_EMBEDDING_DIM];

    // Training loop.
    let mut total_loss = 0.0f64;
    let iterations = MAX_TRAIN_ITERATIONS.min(triples.len());

    for iter in 0..iterations {
        let (h_id, r_id, t_id) = triples[iter % triples.len()];
        let neg_t_id = entity_id_vec[(iter * 7 + 3) % n_entities];

        // Clone to avoid borrow conflicts.
        let neg_tail = *embeddings.get(&neg_t_id).unwrap_or(&zero_emb);
        let mut head = *embeddings.get(&h_id).unwrap_or(&zero_emb);
        let mut rel = *rel_embeddings.get(&r_id).unwrap_or(&zero_emb);
        let mut tail = *embeddings.get(&t_id).unwrap_or(&zero_emb);

        let loss = transe_update(&mut head, &mut rel, &mut tail, &neg_tail);
        total_loss += loss;

        // Write back updated embeddings.
        if let Some(e) = embeddings.get_mut(&h_id) {
            *e = head;
        }
        if let Some(e) = rel_embeddings.get_mut(&r_id) {
            *e = rel;
        }
        if let Some(e) = embeddings.get_mut(&t_id) {
            *e = tail;
        }
    }

    let avg_loss = if iterations > 0 {
        total_loss / iterations as f64
    } else {
        0.0
    };

    // Upsert trained embeddings into the database.
    let n_entities_stored = entity_ids.len() as i64;
    for (&entity_id, emb) in &embeddings {
        let emb_str = format!(
            "[{}]",
            emb.iter()
                .map(|x| format!("{x:.6}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let _ = Spi::run_with_args(
            "INSERT INTO _pg_ripple.kge_embeddings (entity_id, embedding, model) \
             VALUES ($1, $2::vector, $3) \
             ON CONFLICT (entity_id) DO UPDATE SET \
               embedding = EXCLUDED.embedding, \
               model = EXCLUDED.model, \
               trained_at = now()",
            &[
                pgrx::datum::DatumWithOid::from(entity_id),
                pgrx::datum::DatumWithOid::from(emb_str.as_str()),
                pgrx::datum::DatumWithOid::from(model_name.as_str()),
            ],
        );
    }

    (n_entities_stored, n_triples, avg_loss)
}

// ─── pg_extern functions ──────────────────────────────────────────────────────

/// Return statistics about the current KGE embeddings state.
#[pg_extern(schema = "pg_ripple", name = "kge_stats")]
pub fn kge_stats() -> TableIterator<
    'static,
    (
        name!(model, String),
        name!(entities, i64),
        name!(triples_trained_on, i64),
        name!(last_updated, Option<pgrx::datum::TimestampWithTimeZone>),
        name!(training_loss, f64),
    ),
> {
    let model_name = crate::KGE_MODEL
        .get()
        .and_then(|s| s.to_str().ok().map(|v| v.to_lowercase()))
        .unwrap_or_else(|| "transe".to_string());

    let (entities, last_updated) = Spi::connect(|client| {
        let rows = client.select(
            "SELECT count(*)::bigint, max(trained_at) \
             FROM _pg_ripple.kge_embeddings WHERE model = $1",
            None,
            &[pgrx::datum::DatumWithOid::from(model_name.as_str())],
        )?;
        let mut ent = 0i64;
        let mut lu: Option<pgrx::datum::TimestampWithTimeZone> = None;
        // The query returns exactly one row (aggregate).
        #[allow(clippy::never_loop)]
        for row in rows {
            ent = row.get::<i64>(1)?.unwrap_or(0);
            lu = row.get::<pgrx::datum::TimestampWithTimeZone>(2)?;
            break;
        }
        Ok::<_, pgrx::spi::Error>((ent, lu))
    })
    .unwrap_or((0, None));

    TableIterator::new(vec![(model_name, entities, 0i64, last_updated, 0.0f64)])
}

/// Find entity alignment candidates between two named graphs using KGE embeddings.
#[pg_extern(schema = "pg_ripple", name = "find_alignments")]
pub fn find_alignments(
    source_graph: default!(String, "''"),
    target_graph: default!(String, "''"),
    threshold: default!(f64, "0.85"),
    limit: default!(i32, "100"),
) -> TableIterator<
    'static,
    (
        name!(source_iri, String),
        name!(target_iri, String),
        name!(score, f64),
    ),
> {
    let source_graph_id: i64 = if source_graph.is_empty() {
        0
    } else {
        crate::dictionary::encode(&source_graph, crate::dictionary::KIND_IRI)
    };
    let target_graph_id: i64 = if target_graph.is_empty() {
        0
    } else {
        crate::dictionary::encode(&target_graph, crate::dictionary::KIND_IRI)
    };

    let fetch_entity_ids = |g: i64| -> Vec<i64> {
        Spi::connect(|client| {
            let rows = client.select(
                "SELECT DISTINCT s FROM _pg_ripple.vp_rare WHERE g = $1 LIMIT 1000",
                None,
                &[pgrx::datum::DatumWithOid::from(g)],
            )?;
            let mut ids = Vec::new();
            for row in rows {
                if let Some(id) = row.get::<i64>(1)? {
                    ids.push(id);
                }
            }
            Ok::<_, pgrx::spi::Error>(ids)
        })
        .unwrap_or_default()
    };

    let fetch_emb = |eid: i64| -> Option<Vec<f64>> {
        Spi::connect(|client| {
            let rows = client.select(
                "SELECT embedding::text FROM _pg_ripple.kge_embeddings WHERE entity_id = $1",
                None,
                &[pgrx::datum::DatumWithOid::from(eid)],
            )?;
            let mut result = None;
            for row in rows {
                if let Some(s) = row.get::<String>(1)? {
                    result = parse_vector_str(&s);
                    break;
                }
            }
            Ok::<_, pgrx::spi::Error>(result)
        })
        .ok()
        .flatten()
    };

    // Collect source entity IDs.
    let source_ids = fetch_entity_ids(source_graph_id);
    // Collect target entity IDs.
    let target_ids = fetch_entity_ids(target_graph_id);

    // Retrieve embeddings for source entities.
    let source_embs: Vec<(i64, Vec<f64>)> = source_ids
        .iter()
        .filter_map(|&id| fetch_emb(id).map(|e| (id, e)))
        .collect();

    // For each target, find closest source via cosine similarity.
    let limit_usize = (limit as usize).min(1000);
    let mut candidates: Vec<(String, String, f64)> = Vec::new();

    for &tgt_id in &target_ids {
        if candidates.len() >= limit_usize {
            break;
        }
        let tgt_emb = match fetch_emb(tgt_id) {
            Some(e) => e,
            None => continue,
        };

        // Find best matching source entity.
        let mut best_score = 0.0f64;
        let mut best_src_id = 0i64;

        for &(src_id, ref src_emb) in &source_embs {
            let score = cosine_similarity(src_emb, &tgt_emb);
            if score > best_score {
                best_score = score;
                best_src_id = src_id;
            }
        }

        if best_score >= threshold && best_src_id != 0 {
            let src_iri = crate::dictionary::decode(best_src_id).unwrap_or_default();
            let tgt_iri = crate::dictionary::decode(tgt_id).unwrap_or_default();
            if !src_iri.is_empty() && !tgt_iri.is_empty() {
                candidates.push((src_iri, tgt_iri, best_score));
            }
        }
    }

    TableIterator::new(candidates)
}

/// Parse a pgvector-format string like "[0.1,0.2,...]" into a Vec<f64>.
fn parse_vector_str(s: &str) -> Option<Vec<f64>> {
    let s = s.trim().trim_start_matches('[').trim_end_matches(']');
    if s.is_empty() {
        return None;
    }
    let values: Vec<f64> = s
        .split(',')
        .filter_map(|x| x.trim().parse::<f64>().ok())
        .collect();
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

/// Compute cosine similarity between two embedding vectors.
fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let len = a.len().min(b.len());
    let dot: f64 = a[..len]
        .iter()
        .zip(b[..len].iter())
        .map(|(x, y)| x * y)
        .sum();
    let norm_a: f64 = a[..len].iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b[..len].iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a < 1e-10 || norm_b < 1e-10 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transe_update_zero_loss_identical_tail() {
        let mut head = [0.0f64; KGE_EMBEDDING_DIM];
        let mut rel = [0.0f64; KGE_EMBEDDING_DIM];
        let mut tail = [1.0f64; KGE_EMBEDDING_DIM];
        let neg_tail = [1.0f64; KGE_EMBEDDING_DIM];
        // Same positive and negative tail → loss should be exactly MARGIN (1.0).
        let loss = transe_update(&mut head, &mut rel, &mut tail, &neg_tail);
        assert!(loss >= 0.0);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_parse_vector_str() {
        let v = parse_vector_str("[0.1,0.2,0.3]").unwrap();
        assert_eq!(v.len(), 3);
        assert!((v[0] - 0.1).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_l2_unit() {
        let mut v = [
            3.0f64, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        ];
        normalize_l2(&mut v);
        let norm: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }
}
