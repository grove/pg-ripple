//! Error taxonomy for pg_ripple.
//!
//! Error code ranges:
//! - PT001–PT099: dictionary errors
//! - PT100–PT199: storage errors
//! - PT601–PT606: embedding / vector errors (v0.27.0)

use thiserror::Error;

/// Dictionary-layer errors (PT001–PT099).
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum DictionaryError {
    /// The term string exceeded the maximum allowed length.
    #[error("term too long: {len} bytes (max 65535)")]
    TermTooLong { len: usize },

    /// A hash collision was detected between two distinct terms.
    #[error("hash collision detected for term: {term}")]
    HashCollision { term: String },

    /// SPI execution failed during dictionary lookup or insert.
    #[error("dictionary SPI error: {msg}")]
    Spi { msg: String },
}

/// Storage-layer errors (PT100–PT199).
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum StorageError {
    /// The predicate VP table could not be located in the catalog.
    #[error("predicate not found in catalog: id={id}")]
    PredicateNotFound { id: i64 },

    /// Dynamic SQL generation produced an invalid identifier.
    #[error("invalid VP table name for predicate: id={id}")]
    InvalidTableName { id: i64 },

    /// SPI execution failed during triple insert, delete, or query.
    #[error("storage SPI error: {msg}")]
    Spi { msg: String },
}

/// Embedding / vector subsystem errors (PT601–PT607) — v0.27.0 / v0.28.0.
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum EmbeddingError {
    /// PT601 — embedding API URL not configured.
    #[error("embedding API URL not configured; set pg_ripple.embedding_api_url")]
    ApiUrlNotConfigured,

    /// PT602 — embedding dimension mismatch.
    #[error(
        "embedding dimension mismatch: expected {expected} dimensions \
         (pg_ripple.embedding_dimensions), got {got}"
    )]
    DimensionMismatch { expected: i32, got: usize },

    /// PT603 — pgvector extension not installed.
    #[error(
        "pgvector extension not installed; install pgvector and recreate \
         _pg_ripple.embeddings to enable hybrid search"
    )]
    PgvectorNotInstalled,

    /// PT604 — embedding API request failed.
    #[error("embedding API request failed (HTTP {status}): {detail}")]
    ApiRequestFailed { status: u16, detail: String },

    /// PT605 — entity has no embedding.
    #[error("entity has no embedding: {entity_iri}")]
    EntityHasNoEmbedding { entity_iri: String },

    /// PT606 — no stale embeddings found (NOTICE level).
    #[error("no stale embeddings found")]
    NoStaleEmbeddings,

    /// PT607 — vector service endpoint not registered (v0.28.0).
    #[error(
        "vector service endpoint not registered: {url}; \
         register it with pg_ripple.register_vector_endpoint() first"
    )]
    VectorEndpointNotRegistered { url: String },
}

/// Datalog optimization errors (PT501–PT502) — v0.29.0.
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum DatalogOptError {
    /// PT501 — magic sets transformation failed due to a circular binding pattern.
    ///
    /// Occurs when adornment propagation produces a circular dependency in the
    /// magic predicate generation graph, preventing goal-directed inference.
    /// Fallback: run full materialization and filter post-hoc.
    #[error(
        "magic sets transformation failed for goal '{goal}': \
         circular binding pattern detected in rule set '{rule_set}'; \
         falling back to full materialization (PT501)"
    )]
    MagicSetsCircularBinding { goal: String, rule_set: String },

    /// PT502 — cost-based body atom reordering skipped (statistics unavailable).
    ///
    /// Emitted as a WARNING (not ERROR) when `pg_class.reltuples` returns -1
    /// (unanalyzed table) for one or more VP tables referenced by a rule body.
    /// The rule is compiled with the original atom order in this case.
    #[error(
        "cost-based reordering skipped for rule '{rule_text}': \
         VP table statistics unavailable (run ANALYZE on _pg_ripple schema); \
         using original atom order (PT502)"
    )]
    CostReorderSkipped { rule_text: String },
}

/// Datalog aggregation errors (PT510–PT511) — v0.30.0.
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum DatalogAggError {
    /// PT510 — aggregation-stratification violation.
    ///
    /// Occurs when an aggregate body literal references a predicate that is
    /// computed in the same stratum as the head predicate (or depends on the
    /// head predicate through positive rules), creating an illegal recursive
    /// aggregate dependency.  The program has no unique minimal model.
    #[error(
        "aggregation-stratification violation in rule set '{rule_set}': \
         predicate '{agg_pred}' is being aggregated but it is not fully computed \
         before the aggregate rule fires — this creates a cycle through aggregation \
         which is not allowed (PT510)"
    )]
    AggStratificationViolation { rule_set: String, agg_pred: String },

    /// PT511 — unsupported aggregate function in rule body.
    ///
    /// Emitted when a rule body uses an aggregate function that the engine
    /// does not yet support (e.g. a user-defined function name).
    #[error(
        "unsupported aggregate function '{func}' in rule body '{rule_text}'; \
         supported functions are COUNT, SUM, MIN, MAX, AVG (PT511)"
    )]
    UnsupportedAggFunc { func: String, rule_text: String },
}

/// Well-founded semantics errors (PT520) — v0.32.0.
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum WfsError {
    /// PT520 — well-founded fixpoint did not converge within `wfs_max_iterations`.
    ///
    /// The alternating fixpoint passes (positive closure + full inference) are
    /// each bounded by `pg_ripple.wfs_max_iterations`.  If either pass reaches
    /// this limit without converging, this error is emitted as a WARNING and the
    /// (possibly partial) results are returned.  Increase
    /// `pg_ripple.wfs_max_iterations` or simplify the rule set to eliminate
    /// very long derivation chains.
    #[error(
        "well-founded fixpoint did not converge within {max_iter} iterations \
         for rule set '{rule_set}'; results may be incomplete (PT520)"
    )]
    FixpointNotConverged { rule_set: String, max_iter: i32 },
}
