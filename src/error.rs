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

/// Embedding / vector subsystem errors (PT601–PT606) — v0.27.0.
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
}
