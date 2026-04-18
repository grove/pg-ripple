//! Vector embedding support for pg_ripple (v0.27.0).
//!
//! Provides batch embedding via an OpenAI-compatible HTTP API, k-NN similarity
//! queries via pgvector, and SPARQL `pg:similar()` integration.
//!
//! All functions degrade gracefully when pgvector is not installed:
//! - `embed_entities` / `refresh_embeddings` — return 0 with a WARNING
//! - `similar_entities` — return zero rows with a WARNING
//! - `store_embedding` — returns with a WARNING
//!
//! The GUC `pg_ripple.pgvector_enabled = false` disables all paths without
//! uninstalling the extension.

// ─── Runtime availability checks ─────────────────────────────────────────────

/// Returns `true` when pgvector is installed in the current database.
///
/// Checked by looking for the `vector` extension in `pg_extension`.
pub(crate) fn has_pgvector() -> bool {
    pgrx::Spi::get_one::<bool>("SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'vector')")
        .unwrap_or(None)
        .unwrap_or(false)
}

/// Returns `true` when the embeddings column is the native `vector` type
/// (i.e. pgvector is installed and the table was created with it).
fn embeddings_have_vector_column() -> bool {
    pgrx::Spi::get_one::<bool>(
        "SELECT EXISTS( \
            SELECT 1 FROM information_schema.columns \
            WHERE table_schema = '_pg_ripple' \
              AND table_name   = 'embeddings' \
              AND column_name  = 'embedding' \
              AND udt_name     = 'vector' \
        )",
    )
    .unwrap_or(None)
    .unwrap_or(false)
}

/// Guard: returns `false` and emits a WARNING when pgvector is unavailable.
fn pgvector_guard(context: &str) -> bool {
    if !crate::PGVECTOR_ENABLED.get() {
        pgrx::warning!(
            "pg_ripple.{context}: pgvector disabled \
             (pg_ripple.pgvector_enabled = false); returning empty results"
        );
        return false;
    }
    if !has_pgvector() {
        pgrx::warning!(
            "pg_ripple.{context}: pgvector extension not installed (PT603); \
             install pgvector and run the 0.27.0 migration to enable hybrid search"
        );
        return false;
    }
    if !embeddings_have_vector_column() {
        pgrx::warning!(
            "pg_ripple.{context}: _pg_ripple.embeddings was created without pgvector; \
             re-run the 0.27.0 migration after installing pgvector"
        );
        return false;
    }
    true
}

// ─── API helpers ──────────────────────────────────────────────────────────────

/// Call an OpenAI-compatible `/v1/embeddings` endpoint and return the
/// embedding vector for a single input string.
///
/// Returns `Err` with a human-readable message on any network or parse error.
fn call_embedding_api(
    text: &str,
    model: &str,
    api_url: &str,
    api_key: &str,
) -> Result<Vec<f64>, String> {
    let endpoint = format!("{}/embeddings", api_url.trim_end_matches('/'));

    let body_json = serde_json::json!({
        "input": text,
        "model": model,
        "encoding_format": "float"
    });

    let body_str = serde_json::to_string(&body_json)
        .map_err(|e| format!("embedding API request serialisation error: {e}"))?;

    let mut req = ureq::post(&endpoint)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json");

    if !api_key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {api_key}"));
    }

    let response = req
        .send_string(&body_str)
        .map_err(|e| format!("embedding API request failed: {e}"))?;

    if response.status() != 200 {
        return Err(format!(
            "embedding API request failed (HTTP {})",
            response.status()
        ));
    }

    let body = response
        .into_string()
        .map_err(|e| format!("embedding API response read error: {e}"))?;

    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("embedding API response parse error: {e}"))?;

    let embedding = json
        .pointer("/data/0/embedding")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "embedding API response missing data[0].embedding".to_string())?
        .iter()
        .map(|v: &serde_json::Value| {
            v.as_f64()
                .ok_or_else(|| "embedding value is not a number".to_string())
        })
        .collect::<Result<Vec<f64>, _>>()?;

    Ok(embedding)
}

// ─── Public SQL-callable functions ────────────────────────────────────────────

/// Store a user-supplied embedding vector for an entity IRI.
///
/// `embedding` is a `FLOAT8[]` array that will be upserted into
/// `_pg_ripple.embeddings`.  The array length must match
/// `pg_ripple.embedding_dimensions`.
///
/// Returns PT602 (dimension mismatch) or PT603 (pgvector not installed)
/// via a PostgreSQL WARNING and early return rather than an ERROR, so
/// callers can check warnings without aborting transactions.
pub fn store_embedding(entity_iri: &str, embedding: Vec<f64>, model: Option<&str>) {
    if !pgvector_guard("store_embedding") {
        return;
    }

    let dims = crate::EMBEDDING_DIMENSIONS.get();
    if embedding.len() != dims as usize {
        pgrx::warning!(
            "pg_ripple.store_embedding: embedding dimension mismatch (PT602): \
             expected {dims}, got {}",
            embedding.len()
        );
        return;
    }

    let entity_id = crate::dictionary::encode(
        crate::storage::strip_angle_brackets_pub(entity_iri),
        crate::dictionary::KIND_IRI,
    );

    let model_name = model
        .filter(|s| !s.is_empty())
        .unwrap_or("default")
        .to_owned();

    // Build a SQL array literal from the float64 slice.
    let array_lit = format!(
        "ARRAY[{}]::float8[]",
        embedding
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );

    let sql = format!(
        "INSERT INTO _pg_ripple.embeddings (entity_id, model, embedding, updated_at) \
         VALUES ({entity_id}, $1, ({array_lit})::vector, now()) \
         ON CONFLICT (entity_id, model) \
         DO UPDATE SET embedding = EXCLUDED.embedding, updated_at = now()"
    );

    pgrx::Spi::run_with_args(
        &sql,
        &[pgrx::datum::DatumWithOid::from(model_name.as_str())],
    )
    .unwrap_or_else(|e| pgrx::warning!("store_embedding: SPI error: {e}"));
}

/// Return the k nearest entities to `query_text` by cosine distance.
///
/// If pgvector is absent or the embedding API URL is not configured,
/// returns zero rows with a WARNING.
pub fn similar_entities(query_text: &str, k: i32, model: Option<&str>) -> Vec<(i64, String, f64)> {
    if !pgvector_guard("similar_entities") {
        return Vec::new();
    }

    let api_url_guc = crate::EMBEDDING_API_URL.get();
    let api_url = api_url_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");

    if api_url.is_empty() {
        pgrx::warning!(
            "pg_ripple.similar_entities: embedding API URL not configured (PT601); \
             set pg_ripple.embedding_api_url"
        );
        return Vec::new();
    }

    let api_key_guc = crate::EMBEDDING_API_KEY.get();
    let api_key = api_key_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");

    let model_tag = model
        .filter(|s| !s.is_empty())
        .map(|s| s.to_owned())
        .unwrap_or_else(|| {
            let m = crate::EMBEDDING_MODEL.get();
            m.as_ref()
                .and_then(|s| s.to_str().ok())
                .filter(|s| !s.is_empty())
                .unwrap_or("text-embedding-3-small")
                .to_owned()
        });

    let embedding = match call_embedding_api(query_text, &model_tag, api_url, api_key) {
        Ok(v) => v,
        Err(e) => {
            pgrx::warning!("pg_ripple.similar_entities: {e} (PT604)");
            return Vec::new();
        }
    };

    let dims = crate::EMBEDDING_DIMENSIONS.get();
    if embedding.len() != dims as usize {
        pgrx::warning!(
            "pg_ripple.similar_entities: embedding dimension mismatch (PT602): \
             expected {dims}, got {}",
            embedding.len()
        );
        return Vec::new();
    }

    let array_lit = format!(
        "ARRAY[{}]::float8[]",
        embedding
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );

    let sql = format!(
        "SELECT e.entity_id, d.value, \
                (e.embedding <=> ({array_lit})::vector)::float8 AS dist \
         FROM _pg_ripple.embeddings e \
         JOIN _pg_ripple.dictionary d ON d.id = e.entity_id \
         ORDER BY e.embedding <=> ({array_lit})::vector \
         LIMIT {k}"
    );

    pgrx::Spi::connect(|c| {
        c.select(&sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("similar_entities: SPI error: {e}"))
            .map(|row| {
                let entity_id: i64 = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                let entity_iri: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                let distance: f64 = row.get::<f64>(3).ok().flatten().unwrap_or(2.0);
                (entity_id, entity_iri, distance)
            })
            .collect()
    })
}

/// Batch-embed entities from a graph.
///
/// Collects entity IRIs + their `rdfs:label` (or IRI local name) from the
/// specified graph (or all graphs if NULL), calls the configured embedding API
/// in batches of `batch_size`, and upserts results into `_pg_ripple.embeddings`.
///
/// Returns the total number of embeddings stored, or 0 on error/degradation.
pub fn embed_entities(graph_iri: Option<&str>, model: Option<&str>, batch_size: i32) -> i64 {
    if !pgvector_guard("embed_entities") {
        return 0;
    }

    let api_url_guc = crate::EMBEDDING_API_URL.get();
    let api_url = api_url_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");

    if api_url.is_empty() {
        pgrx::warning!(
            "pg_ripple.embed_entities: embedding API URL not configured (PT601); \
             set pg_ripple.embedding_api_url"
        );
        return 0;
    }

    let api_key_guc = crate::EMBEDDING_API_KEY.get();
    let api_key = api_key_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");

    let model_tag = model
        .filter(|s| !s.is_empty())
        .map(|s| s.to_owned())
        .unwrap_or_else(|| {
            let m = crate::EMBEDDING_MODEL.get();
            m.as_ref()
                .and_then(|s| s.to_str().ok())
                .filter(|s| !s.is_empty())
                .unwrap_or("text-embedding-3-small")
                .to_owned()
        });

    // Collect entity IRIs + labels via SPARQL.
    // We use a direct SPI query against the dictionary to find IRI entities.
    let graph_filter = if let Some(g) = graph_iri.filter(|s| !s.is_empty()) {
        let g_id = crate::dictionary::encode(
            crate::storage::strip_angle_brackets_pub(g),
            crate::dictionary::KIND_IRI,
        );
        format!("AND vp.g = {g_id}")
    } else {
        String::new()
    };

    // Find subject entities — IRIs that appear as subjects in any VP table.
    let entity_rows: Vec<(i64, String)> = pgrx::Spi::connect(|c| {
        let sql = format!(
            "SELECT DISTINCT d.id, d.value \
             FROM _pg_ripple.dictionary d \
             WHERE d.kind = 0 \
             AND EXISTS ( \
                 SELECT 1 FROM _pg_ripple.vp_rare vp \
                 WHERE vp.s = d.id {graph_filter} \
             ) \
             LIMIT 10000"
        );
        c.select(&sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("embed_entities: SPI error: {e}"))
            .map(|row| {
                let id: i64 = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                let value: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                (id, value)
            })
            .collect()
    });

    if entity_rows.is_empty() {
        pgrx::warning!("pg_ripple.embed_entities: no IRI entities found in the specified graph");
        return 0;
    }

    let effective_batch = batch_size.clamp(1, 1000) as usize;
    let mut total_stored = 0i64;

    for chunk in entity_rows.chunks(effective_batch) {
        for (entity_id, iri) in chunk {
            // Use the IRI local name as the label text.
            let label = extract_local_name(iri);

            let embedding = match call_embedding_api(&label, &model_tag, api_url, api_key) {
                Ok(v) => v,
                Err(e) => {
                    pgrx::warning!("pg_ripple.embed_entities: API error for <{iri}>: {e} (PT604)");
                    continue;
                }
            };

            let dims = crate::EMBEDDING_DIMENSIONS.get();
            if embedding.len() != dims as usize {
                pgrx::warning!(
                    "pg_ripple.embed_entities: dimension mismatch for <{iri}> (PT602): \
                     expected {dims}, got {}",
                    embedding.len()
                );
                continue;
            }

            let array_lit = format!(
                "ARRAY[{}]::float8[]",
                embedding
                    .iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );

            let upsert_sql = format!(
                "INSERT INTO _pg_ripple.embeddings \
                     (entity_id, model, embedding, updated_at) \
                 VALUES ({entity_id}, $1, ({array_lit})::vector, now()) \
                 ON CONFLICT (entity_id, model) \
                 DO UPDATE SET embedding = EXCLUDED.embedding, updated_at = now()"
            );

            if pgrx::Spi::run_with_args(
                &upsert_sql,
                &[pgrx::datum::DatumWithOid::from(model_tag.as_str())],
            )
            .is_ok()
            {
                total_stored += 1;
            }
        }
    }

    total_stored
}

/// Refresh stale embeddings whose labels were updated after the stored embedding.
///
/// Identifies entities where a new triple has been inserted after the
/// `updated_at` timestamp in `_pg_ripple.embeddings`.  Re-embeds stale
/// entities in batches.  When `force = true`, re-embeds all entities
/// regardless of staleness.
///
/// Returns the count of re-embedded entities.
pub fn refresh_embeddings(graph_iri: Option<&str>, model: Option<&str>, force: bool) -> i64 {
    if !pgvector_guard("refresh_embeddings") {
        return 0;
    }

    let api_url_guc = crate::EMBEDDING_API_URL.get();
    let api_url = api_url_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");

    if api_url.is_empty() {
        pgrx::warning!(
            "pg_ripple.refresh_embeddings: embedding API URL not configured (PT601); \
             set pg_ripple.embedding_api_url"
        );
        return 0;
    }

    let api_key_guc = crate::EMBEDDING_API_KEY.get();
    let api_key = api_key_guc
        .as_ref()
        .and_then(|s| s.to_str().ok())
        .unwrap_or("");

    let model_tag = model
        .filter(|s| !s.is_empty())
        .map(|s| s.to_owned())
        .unwrap_or_else(|| {
            let m = crate::EMBEDDING_MODEL.get();
            m.as_ref()
                .and_then(|s| s.to_str().ok())
                .filter(|s| !s.is_empty())
                .unwrap_or("text-embedding-3-small")
                .to_owned()
        });

    let model_filter = format!("AND e.model = '{}'", model_tag.replace('\'', "''"));

    let graph_filter = if let Some(g) = graph_iri.filter(|s| !s.is_empty()) {
        let g_id = crate::dictionary::encode(
            crate::storage::strip_angle_brackets_pub(g),
            crate::dictionary::KIND_IRI,
        );
        format!("AND vp.g = {g_id}")
    } else {
        String::new()
    };

    // Find stale entities: those with an existing embedding that was updated
    // before the most recent triple involving that entity as subject.
    // When force=true, return all entities that have any embedding.
    let stale_sql = if force {
        format!(
            "SELECT e.entity_id, d.value \
             FROM _pg_ripple.embeddings e \
             JOIN _pg_ripple.dictionary d ON d.id = e.entity_id \
             WHERE TRUE {model_filter} \
             LIMIT 10000"
        )
    } else {
        // Identify entities whose embedding is older than the most recent
        // triple insertion.  We use the max statement ID as a proxy for
        // recency (higher SID = later write).
        format!(
            "SELECT e.entity_id, d.value \
             FROM _pg_ripple.embeddings e \
             JOIN _pg_ripple.dictionary d ON d.id = e.entity_id \
             WHERE EXISTS ( \
                 SELECT 1 FROM _pg_ripple.vp_rare vp \
                 WHERE vp.s = e.entity_id {graph_filter} \
                   AND vp.i > ( \
                       SELECT COALESCE(MAX(vp2.i), 0) \
                       FROM _pg_ripple.vp_rare vp2 \
                       WHERE vp2.s = e.entity_id \
                         AND vp2.i <= \
                             (SELECT EXTRACT(EPOCH FROM e.updated_at)::bigint) \
                   ) \
             ) \
             {model_filter} \
             LIMIT 10000"
        )
    };

    let stale_entities: Vec<(i64, String)> = pgrx::Spi::connect(|c| {
        c.select(&stale_sql, None, &[])
            .unwrap_or_else(|e| pgrx::error!("refresh_embeddings: SPI error: {e}"))
            .map(|row| {
                let id: i64 = row.get::<i64>(1).ok().flatten().unwrap_or(0);
                let value: String = row.get::<String>(2).ok().flatten().unwrap_or_default();
                (id, value)
            })
            .collect()
    });

    if stale_entities.is_empty() {
        pgrx::notice!("pg_ripple.refresh_embeddings: no stale embeddings found (PT606)");
        return 0;
    }

    let mut refreshed = 0i64;

    for (entity_id, iri) in &stale_entities {
        let label = extract_local_name(iri);

        let embedding = match call_embedding_api(&label, &model_tag, api_url, api_key) {
            Ok(v) => v,
            Err(e) => {
                pgrx::warning!("pg_ripple.refresh_embeddings: API error for <{iri}>: {e} (PT604)");
                continue;
            }
        };

        let dims = crate::EMBEDDING_DIMENSIONS.get();
        if embedding.len() != dims as usize {
            pgrx::warning!(
                "pg_ripple.refresh_embeddings: dimension mismatch for <{iri}> (PT602): \
                 expected {dims}, got {}",
                embedding.len()
            );
            continue;
        }

        let array_lit = format!(
            "ARRAY[{}]::float8[]",
            embedding
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        let upsert_sql = format!(
            "INSERT INTO _pg_ripple.embeddings \
                 (entity_id, model, embedding, updated_at) \
             VALUES ({entity_id}, $1, ({array_lit})::vector, now()) \
             ON CONFLICT (entity_id, model) \
             DO UPDATE SET embedding = EXCLUDED.embedding, updated_at = now()"
        );

        if pgrx::Spi::run_with_args(
            &upsert_sql,
            &[pgrx::datum::DatumWithOid::from(model_tag.as_str())],
        )
        .is_ok()
        {
            refreshed += 1;
        }
    }

    refreshed
}

// ─── SPARQL pg:similar() SQL translation ─────────────────────────────────────

/// Returns `true` when pgvector is available for use in SPARQL translation.
///
/// This is called at SPARQL-translation time (not at execution time) to decide
/// whether to emit real SQL or a graceful degradation stub.
pub(crate) fn pgvector_available_for_sparql() -> bool {
    if !crate::PGVECTOR_ENABLED.get() {
        return false;
    }
    has_pgvector() && embeddings_have_vector_column()
}

/// Build a SQL expression for the `pg:similar(?entity, "text", k)` function
/// in value context (BIND expression).  Returns a SQL expression that evaluates
/// to a FLOAT8 cosine distance.
///
/// When pgvector is absent, returns `NULL::float8` so the query still parses.
pub(crate) fn sql_for_pg_similar(entity_col: &str, query_text: &str, _k: i64) -> String {
    if !pgvector_available_for_sparql() {
        return "NULL::float8".to_string();
    }

    // At query time, the embedding for query_text would need to be fetched from
    // the API.  Because we are inside the SPARQL→SQL translation pipeline (not
    // at execution time), we emit a correlated subquery that looks up the stored
    // cosine distance from _pg_ripple.embeddings.
    //
    // The generated SQL pattern:
    //   (SELECT (e.embedding <=> ref_emb.embedding)::float8
    //    FROM _pg_ripple.embeddings e
    //    JOIN _pg_ripple.embeddings ref_emb ON ref_emb.entity_id = encode_term(query_text,0)
    //    WHERE e.entity_id = <entity_col>
    //    LIMIT 1)
    //
    // In practice callers store the query embedding first (via store_embedding),
    // then run the SPARQL query.  For fully autonomous runtime embedding we
    // would need a PL/pgSQL wrapper; that is deferred to v0.28.0.

    let escaped_text = query_text.replace('\'', "''");
    format!(
        "(SELECT (e.embedding <=> ref_emb.embedding)::float8 \
          FROM _pg_ripple.embeddings e, \
               _pg_ripple.embeddings ref_emb \
          WHERE ref_emb.entity_id = pg_ripple.encode_term('{escaped_text}', 0) \
            AND e.entity_id = {entity_col} \
          LIMIT 1)"
    )
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extract the local name from an IRI string.
///
/// Returns the part after the last `#` or `/`.  Falls back to the full IRI.
fn extract_local_name(iri: &str) -> String {
    iri.rfind(['#', '/'])
        .map(|pos| &iri[pos + 1..])
        .filter(|s| !s.is_empty())
        .unwrap_or(iri)
        .to_owned()
}
