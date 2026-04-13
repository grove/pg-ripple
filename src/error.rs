//! Error taxonomy for pg_ripple.
//!
//! Error code ranges:
//! - PT001–PT099: dictionary errors
//! - PT100–PT199: storage errors

use thiserror::Error;

/// Dictionary-layer errors (PT001–PT099).
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
