//! AI & LLM Integration — v0.49.0
//!
//! Provides two features:
//!
//! 1. **NL → SPARQL via LLM function calling** (`sparql_from_nl`): sends a
//!    plain-English question to a configured OpenAI-compatible chat endpoint
//!    and returns a parseable SPARQL SELECT query string.
//!
//! 2. **Embedding-based `owl:sameAs` candidate generation** (`suggest_sameas`,
//!    `apply_sameas_candidates`): runs an HNSW self-join on the
//!    `_pg_ripple.embeddings` table to surface entity pairs whose cosine
//!    similarity exceeds a configurable threshold, then optionally inserts the
//!    accepted pairs as `owl:sameAs` triples.
//!
//! ## Mock endpoint
//!
//! When `pg_ripple.llm_endpoint` is set to the special value `'mock'`, the
//! HTTP call is bypassed and a canned SPARQL SELECT query is returned.  This
//! allows pg_regress tests to exercise the full code path (prompt assembly,
//! SPARQL extraction, parse validation) without an external LLM dependency.

use pgrx::prelude::*;
use spargebra::SparqlParser;

// ─── LLM endpoint call ────────────────────────────────────────────────────────

/// The canned SPARQL response returned when the endpoint is `'mock'`.
const MOCK_SPARQL: &str = "SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10";

/// Call the configured LLM endpoint and return the raw response body.
///
/// Uses an OpenAI-compatible `/v1/chat/completions` JSON API.
/// Returns `Err` with a human-readable message on any network or HTTP error.
fn call_llm_endpoint(
    endpoint: &str,
    model: &str,
    api_key: &str,
    prompt: &str,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "You are a SPARQL query generator. \
                    Given a natural-language question and a graph schema, \
                    output ONLY a valid SPARQL 1.1 SELECT query with no explanation, \
                    markdown, or extra text."
            },
            {
                "role": "user",
                "content": prompt
            }
        ],
        "temperature": 0.0
    });

    let body_str = serde_json::to_string(&body)
        .map_err(|e| format!("LLM request serialisation error: {e}"))?;

    let timeout = std::time::Duration::from_secs(30);
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();

    let mut req = agent
        .post(&url)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json");

    if !api_key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {api_key}"));
    }

    let response = req
        .send_string(&body_str)
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if response.status() != 200 {
        return Err(format!("HTTP {} from LLM endpoint", response.status()));
    }

    response
        .into_string()
        .map_err(|e| format!("response read error: {e}"))
}

/// Extract a SPARQL query string from an OpenAI-style chat completion response.
///
/// Looks for the `choices[0].message.content` field and strips any leading /
/// trailing whitespace or markdown code-fence markers.  Returns `None` when
/// the content cannot be extracted or appears empty.
fn extract_sparql_from_response(body: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    let content = json
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())?
        .trim()
        .to_owned();

    if content.is_empty() {
        return None;
    }

    // Strip optional markdown code fence.
    let stripped = if let Some(inner) = content
        .strip_prefix("```sparql")
        .or_else(|| content.strip_prefix("```"))
    {
        inner.trim_start().trim_end_matches("```").trim().to_owned()
    } else {
        content
    };

    if stripped.is_empty() {
        None
    } else {
        Some(stripped)
    }
}

/// Build a VoID description of the current graph for use as LLM context.
fn build_void_description() -> String {
    let triple_count = pgrx::Spi::get_one::<i64>("SELECT COUNT(*) FROM _pg_ripple.predicates")
        .unwrap_or(None)
        .unwrap_or(0);

    // Collect up to 20 predicate IRIs as hints for the LLM.
    let predicates: Vec<String> = pgrx::Spi::connect(|client| {
        let rows = client.select(
            "SELECT d.value \
             FROM _pg_ripple.predicates p \
             JOIN _pg_ripple.dictionary d ON d.id = p.id \
             ORDER BY p.triple_count DESC \
             LIMIT 20",
            None,
            &[],
        )?;
        let mut result = Vec::new();
        for row in rows {
            if let Some(v) = row.get::<&str>(1)? {
                result.push(v.to_owned());
            }
        }
        Ok::<_, pgrx::spi::Error>(result)
    })
    .unwrap_or_default();

    let pred_list = if predicates.is_empty() {
        "(no predicates yet)".to_owned()
    } else {
        predicates
            .iter()
            .map(|p| format!("  <{p}>"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "Graph schema (VoID description):\n\
         - Total predicate types: {triple_count}\n\
         - Known predicates (most frequent first):\n{pred_list}\n"
    )
}

/// Build a SHACL shapes summary string for use as LLM context.
fn build_shapes_summary() -> String {
    let shapes: Vec<String> = pgrx::Spi::connect(|client| {
        let rows = client.select(
            "SELECT shape_iri \
             FROM _pg_ripple.shacl_shapes \
             WHERE active = true \
             ORDER BY shape_iri \
             LIMIT 10",
            None,
            &[],
        );
        let rows = match rows {
            Ok(r) => r,
            Err(_) => return Ok(Vec::new()),
        };
        let mut result = Vec::new();
        for row in rows {
            if let Some(v) = row.get::<&str>(1)? {
                result.push(v.to_owned());
            }
        }
        Ok::<_, pgrx::spi::Error>(result)
    })
    .unwrap_or_default();

    if shapes.is_empty() {
        String::new()
    } else {
        format!(
            "\nActive SHACL shapes (target classes):\n{}\n",
            shapes
                .iter()
                .map(|s| format!("  <{s}>"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

/// Load few-shot examples from `_pg_ripple.llm_examples`.
fn load_few_shot_examples() -> Vec<(String, String)> {
    pgrx::Spi::connect(|client| {
        let rows = client.select(
            "SELECT question, sparql FROM _pg_ripple.llm_examples ORDER BY question LIMIT 20",
            None,
            &[],
        )?;
        let mut result = Vec::new();
        for row in rows {
            let q = row.get::<&str>(1)?.unwrap_or("").to_owned();
            let s = row.get::<&str>(2)?.unwrap_or("").to_owned();
            if !q.is_empty() && !s.is_empty() {
                result.push((q, s));
            }
        }
        Ok::<_, pgrx::spi::Error>(result)
    })
    .unwrap_or_default()
}

// ─── Public SQL-callable functions ────────────────────────────────────────────

/// Convert a natural-language question to a SPARQL query via a configured LLM.
///
/// Behaviour:
/// - PT700: `pg_ripple.llm_endpoint` is empty (not configured)
/// - PT700: the HTTP call to the LLM endpoint fails
/// - PT701: the response does not contain a SPARQL-looking string
/// - PT702: the extracted string fails `spargebra` parsing
///
/// When `pg_ripple.llm_endpoint = 'mock'`, the HTTP call is bypassed and the
/// built-in canned SPARQL query is returned for testing purposes.
#[pg_extern(schema = "pg_ripple", name = "sparql_from_nl")]
pub fn sparql_from_nl(question: &str) -> String {
    let endpoint_raw = crate::LLM_ENDPOINT
        .get()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let endpoint = endpoint_raw.trim().to_owned();

    if endpoint.is_empty() {
        pgrx::error!(
            "LLM endpoint not configured (PT700); \
             set pg_ripple.llm_endpoint to an OpenAI-compatible base URL \
             or 'mock' for testing"
        );
    }

    // Mock path: bypass HTTP and return a canned query for testing.
    if endpoint == "mock" {
        let sparql = MOCK_SPARQL.to_owned();
        // Validate the canned query (sanity check).
        if SparqlParser::new().parse_query(&sparql).is_err() {
            pgrx::error!("mock SPARQL query failed to parse (PT702): {sparql}");
        }
        return sparql;
    }

    // Resolve the API key from the environment variable.
    let key_env = crate::LLM_API_KEY_ENV
        .get()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "PG_RIPPLE_LLM_API_KEY".to_owned());

    let key_env_trimmed = key_env.trim().to_owned();
    let api_key = if key_env_trimmed.is_empty() {
        String::new()
    } else {
        // SAFETY: std::env::var reads from the process environment; no mutation occurs.
        std::env::var(&key_env_trimmed).unwrap_or_default()
    };

    let model = crate::LLM_MODEL
        .get()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "gpt-4o".to_owned());

    // Assemble the prompt.
    let void_desc = build_void_description();
    let shapes_ctx = if crate::LLM_INCLUDE_SHAPES.get() {
        build_shapes_summary()
    } else {
        String::new()
    };

    let examples = load_few_shot_examples();
    let few_shot = if examples.is_empty() {
        String::new()
    } else {
        let pairs = examples
            .iter()
            .map(|(q, s)| format!("Q: {q}\nSPARQL: {s}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        format!("\n\nExamples:\n{pairs}\n")
    };

    let prompt = format!(
        "{void_desc}{shapes_ctx}{few_shot}\n\
         Question: {question}\n\
         Output ONLY the SPARQL query, nothing else."
    );

    // Call the LLM endpoint.
    let raw_body = call_llm_endpoint(&endpoint, &model, &api_key, &prompt).unwrap_or_else(|e| {
        pgrx::error!("LLM endpoint unreachable or returned HTTP error: {e} (PT700)")
    });

    // Extract the SPARQL string from the chat completion response.
    let sparql = extract_sparql_from_response(&raw_body).unwrap_or_else(|| {
        pgrx::error!(
            "LLM response did not contain a valid SPARQL query (PT701); \
             raw response: {}",
            &raw_body[..raw_body.len().min(500)]
        )
    });

    // Validate parsability.
    if let Err(e) = SparqlParser::new().parse_query(&sparql) {
        pgrx::error!(
            "LLM-generated SPARQL query failed to parse (PT702): {e}; \
             query text: {sparql}"
        );
    }

    sparql
}

/// Store a few-shot question/SPARQL example for use as LLM context.
///
/// Rows are persisted in `_pg_ripple.llm_examples` and loaded automatically
/// by `sparql_from_nl()` on each call.
#[pg_extern(schema = "pg_ripple", name = "add_llm_example")]
pub fn add_llm_example(question: &str, sparql: &str) {
    pgrx::Spi::run_with_args(
        "INSERT INTO _pg_ripple.llm_examples (question, sparql) \
         VALUES ($1, $2) \
         ON CONFLICT (question) DO UPDATE SET sparql = EXCLUDED.sparql",
        &[
            pgrx::datum::DatumWithOid::from(question),
            pgrx::datum::DatumWithOid::from(sparql),
        ],
    )
    .unwrap_or_else(|e| pgrx::error!("add_llm_example: SPI error: {e}"));
}

// ─── Embedding-based owl:sameAs candidate generation ─────────────────────────

/// Return candidate `owl:sameAs` entity pairs by HNSW cosine self-join.
///
/// Requires pgvector to be installed and `_pg_ripple.embeddings` to contain
/// at least two rows.  Degrades gracefully with a WARNING when:
/// - pgvector is not installed
/// - `pg_ripple.pgvector_enabled = false`
/// - the embeddings table has fewer than 2 entities
///
/// Each row contains the IRI strings of the two candidate entities and their
/// cosine similarity score.  Pairs with `similarity >= threshold` are returned.
/// Self-matches (same entity_id) are excluded.
#[pg_extern(schema = "pg_ripple", name = "suggest_sameas")]
pub fn suggest_sameas(
    threshold: default!(f32, "0.9"),
) -> TableIterator<'static, (name!(s1, String), name!(s2, String), name!(similarity, f32))> {
    // Graceful degradation when pgvector is unavailable.
    if !crate::PGVECTOR_ENABLED.get() {
        pgrx::warning!(
            "pg_ripple.suggest_sameas: pgvector disabled \
             (pg_ripple.pgvector_enabled = false); returning empty results"
        );
        return TableIterator::new(std::iter::empty());
    }

    if !crate::sparql::embedding::has_pgvector() {
        pgrx::warning!(
            "pg_ripple.suggest_sameas: pgvector extension not installed (PT603); \
             install pgvector and run the 0.27.0 migration to enable similarity search"
        );
        return TableIterator::new(std::iter::empty());
    }

    // Clamp threshold to [0.0, 1.0].
    let threshold = threshold.clamp(0.0_f32, 1.0_f32);

    // Self-join: find pairs (a, b) where cosine_distance(a.embedding, b.embedding)
    // is small enough that 1 - distance >= threshold.
    // We use `<=>` (cosine distance) from pgvector; similarity = 1 - distance.
    let query = format!(
        "SELECT \
             da.value AS s1, \
             db.value AS s2, \
             (1.0 - (a.embedding <=> b.embedding))::real AS similarity \
         FROM _pg_ripple.embeddings a \
         JOIN _pg_ripple.embeddings b \
             ON a.entity_id < b.entity_id \
         JOIN _pg_ripple.dictionary da ON da.id = a.entity_id \
         JOIN _pg_ripple.dictionary db ON db.id = b.entity_id \
         WHERE a.model = b.model \
           AND da.kind = 0 \
           AND db.kind = 0 \
           AND (1.0 - (a.embedding <=> b.embedding)) >= {threshold}"
    );

    let rows: Vec<(String, String, f32)> = pgrx::Spi::connect(|client| {
        let result = client.select(&query, None, &[])?;
        let mut out = Vec::new();
        for row in result {
            let s1 = row.get::<&str>(1)?.unwrap_or("").to_owned();
            let s2 = row.get::<&str>(2)?.unwrap_or("").to_owned();
            let sim = row.get::<f32>(3)?.unwrap_or(0.0);
            if !s1.is_empty() && !s2.is_empty() {
                out.push((s1, s2, sim));
            }
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or_else(|e| {
        pgrx::warning!("suggest_sameas: SPI error: {e}");
        Vec::new()
    });

    TableIterator::new(rows)
}

/// Insert accepted `owl:sameAs` candidate pairs as triples and trigger
/// cluster merging.
///
/// Runs `suggest_sameas(min_similarity)` and, for each returned pair, inserts
/// an `owl:sameAs` triple (both directions).  The cluster-size guard from
/// `pg_ripple.sameas_max_cluster_size` (PT550) is respected via the normal
/// storage path.
///
/// Returns the number of new `owl:sameAs` triples inserted (each direction
/// counts separately, so a single pair contributes 2 if both directions are new).
#[pg_extern(schema = "pg_ripple", name = "apply_sameas_candidates")]
pub fn apply_sameas_candidates(min_similarity: default!(f32, "0.95")) -> i64 {
    const OWL_SAME_AS: &str = "<http://www.w3.org/2002/07/owl#sameAs>";

    let candidates: Vec<(String, String)> = pgrx::Spi::connect(|client| {
        let threshold = min_similarity.clamp(0.0_f32, 1.0_f32);

        if !crate::PGVECTOR_ENABLED.get() || !crate::sparql::embedding::has_pgvector() {
            return Ok(Vec::new());
        }

        let query = format!(
            "SELECT \
                 da.value AS s1, \
                 db.value AS s2 \
             FROM _pg_ripple.embeddings a \
             JOIN _pg_ripple.embeddings b \
                 ON a.entity_id < b.entity_id \
             JOIN _pg_ripple.dictionary da ON da.id = a.entity_id \
             JOIN _pg_ripple.dictionary db ON db.id = b.entity_id \
             WHERE a.model = b.model \
               AND da.kind = 0 \
               AND db.kind = 0 \
               AND (1.0 - (a.embedding <=> b.embedding)) >= {threshold}"
        );

        let result = client.select(&query, None, &[])?;
        let mut out = Vec::new();
        for row in result {
            let s1 = row.get::<&str>(1)?.unwrap_or("").to_owned();
            let s2 = row.get::<&str>(2)?.unwrap_or("").to_owned();
            if !s1.is_empty() && !s2.is_empty() {
                out.push((s1, s2));
            }
        }
        Ok::<_, pgrx::spi::Error>(out)
    })
    .unwrap_or_default();

    let mut inserted: i64 = 0;
    for (s1, s2) in candidates {
        let iri_s1 = format!("<{s1}>");
        let iri_s2 = format!("<{s2}>");

        // Forward: s1 owl:sameAs s2
        let sid_fwd = crate::storage::insert_triple(&iri_s1, OWL_SAME_AS, &iri_s2, 0);
        if sid_fwd > 0 {
            inserted += 1;
        }

        // Reverse: s2 owl:sameAs s1
        let sid_rev = crate::storage::insert_triple(&iri_s2, OWL_SAME_AS, &iri_s1, 0);
        if sid_rev > 0 {
            inserted += 1;
        }
    }

    inserted
}
