//! GUC parameters for the LLM / AI integration subsystem (embeddings,
//! vector index, entity resolution, and NL→SPARQL generation).

// ─── v0.27.0 LLM / embedding GUCs ────────────────────────────────────────────

/// GUC: embedding model name tag.
pub static EMBEDDING_MODEL: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: vector dimension count; must match the actual model output (default: 1536).
pub static EMBEDDING_DIMENSIONS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(1536);

/// GUC: base URL for an OpenAI-compatible embedding API.
pub static EMBEDDING_API_URL: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: API key for the embedding endpoint.  Superuser-only.
pub static EMBEDDING_API_KEY: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: runtime switch; set to `false` to disable all pgvector-dependent code paths.
pub static PGVECTOR_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);

/// GUC: index type created on `_pg_ripple.embeddings` — `'hnsw'` or `'ivfflat'`.
pub static EMBEDDING_INDEX_TYPE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: embedding storage precision — `'single'`, `'half'`, or `'binary'`.
pub static EMBEDDING_PRECISION: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

// ─── v0.28.0 LLM / embedding GUCs ────────────────────────────────────────────

/// GUC: master switch for trigger-based auto-embedding of new dictionary entries.
pub static AUTO_EMBED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: number of entities dequeued and embedded per background worker batch.
pub static EMBEDDING_BATCH_SIZE: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100);

/// GUC: when `true`, serialize each entity's RDF neighborhood before embedding.
pub static USE_GRAPH_CONTEXT: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: HTTP timeout in milliseconds for calls to external vector service endpoints.
pub static VECTOR_FEDERATION_TIMEOUT_MS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(5000);

// ─── v0.49.0 LLM GUCs ────────────────────────────────────────────────────────

/// GUC: LLM API base URL for natural-language → SPARQL generation (v0.49.0).
pub static LLM_ENDPOINT: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: LLM model identifier used for NL → SPARQL generation (v0.49.0).
pub static LLM_MODEL: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: name of the environment variable that holds the LLM API key (v0.49.0).
pub static LLM_API_KEY_ENV: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: when `on` (default), include active SHACL shapes as semantic context
/// in the prompt sent to the LLM endpoint (v0.49.0).
pub static LLM_INCLUDE_SHAPES: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(true);
