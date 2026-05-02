//! GUC (Grand Unified Configuration) parameter declarations for pg_ripple.
//!
//! This module re-exports all GUC statics from per-subsystem submodules so
//! that `pub(crate) use gucs::*;` in `lib.rs` continues to expose every GUC
//! at the crate root (e.g. `crate::SOME_GUC`).
//!
//! ## Submodule layout (v0.53.0 split)
//!
//! | Module | Subsystem |
//! |---|---|
//! | `storage` | VP tables, HTAP merge, dictionary cache, CDC bridge |
//! | `sparql` | SPARQL engine, property paths, WCOJ, DoS limits |
//! | `datalog` | Datalog inference, semi-naive, DRed, parallel strata |
//! | `shacl` | SHACL validation mode |
//! | `federation` | SPARQL federation, connection pooling, source selection |
//! | `llm` | Embeddings, vector index, NL→SPARQL LLM integration |
//! | `observability` | OpenTelemetry tracing, export limits |

pub mod datalog;
pub mod federation;
pub mod llm;
pub mod observability;
pub mod pagerank;
pub mod registration;
pub mod shacl;
pub mod sparql;
pub mod storage;

pub use datalog::*;
pub use federation::*;
pub use llm::*;
pub use observability::*;
pub use pagerank::*;
pub use shacl::*;
pub use sparql::*;
pub use storage::*;
