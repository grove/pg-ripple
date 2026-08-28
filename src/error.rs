//! Error taxonomy for pg_ripple.
//!
//! Error code ranges:
//! - PT001–PT099: dictionary errors
//! - PT100–PT199: storage errors
//! - PT301–PT307: uncertain knowledge engine errors (v0.87.0)
//! - PT601–PT607: embedding / vector errors (v0.27.0)
//! - PT640–PT642: result-set / export overflow errors (v0.40.0)

// A16-CQ: All error enum variants are part of the public API surface and may be
// instantiated by extension consumers or future callers outside this crate.
// Consolidating individual #[allow(dead_code)] items into a single module-level
// attribute is intentional (L16-01 reduction target).
#![allow(dead_code)]

use thiserror::Error;

/// Deterministic, feature-gated fault points used by resilience tests.
pub(crate) mod fault_injection {
    #[cfg(feature = "fault_injection")]
    use std::collections::HashMap;
    #[cfg(feature = "fault_injection")]
    use std::sync::{Mutex, OnceLock};

    #[cfg(feature = "fault_injection")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Action {
        Terminate,
        TransactionError,
        Delay(u64),
        SpiFailure,
    }

    #[cfg(feature = "fault_injection")]
    static TEST_HOOKS: OnceLock<Mutex<HashMap<String, Action>>> = OnceLock::new();

    /// Trigger a named fault point. This is a no-op without `fault_injection`.
    #[inline]
    pub(crate) fn hit(point: &str) {
        #[cfg(not(feature = "fault_injection"))]
        let _ = point;

        #[cfg(feature = "fault_injection")]
        if let Some(action) = configured(point) {
            match action {
                Action::Terminate => std::process::abort(),
                Action::TransactionError => {
                    pgrx::error!("fault injection at {point}: transaction error")
                }
                Action::Delay(ms) => std::thread::sleep(std::time::Duration::from_millis(ms)),
                Action::SpiFailure => {
                    pgrx::error!("fault injection at {point}: simulated SPI failure")
                }
            }
        }
    }

    #[cfg(feature = "fault_injection")]
    fn configured(point: &str) -> Option<Action> {
        if let Some(hooks) = TEST_HOOKS.get() {
            if let Ok(hooks) = hooks.lock() {
                if let Some(action) = hooks.get(point).copied() {
                    return Some(action);
                }
            }
        }
        std::env::var("PG_RIPPLE_FAULT_INJECTION")
            .ok()
            .and_then(|spec| {
                spec.split(';').find_map(|entry| {
                    let (name, action) = entry.split_once('=')?;
                    (name.trim() == point).then(|| parse_action(action.trim()))
                })
            })
            .flatten()
    }

    #[cfg(feature = "fault_injection")]
    fn parse_action(action: &str) -> Option<Action> {
        match action {
            "terminate" => Some(Action::Terminate),
            "transaction_error" | "error" => Some(Action::TransactionError),
            "spi_failure" | "spi" => Some(Action::SpiFailure),
            value => value
                .strip_prefix("delay:")?
                .parse()
                .ok()
                .map(Action::Delay),
        }
    }

    /// Install one deterministic hook for a unit/integration test.
    #[cfg(feature = "fault_injection")]
    pub(crate) fn set_for_tests(point: &str, action: &str) -> bool {
        let Some(action) = parse_action(action) else {
            return false;
        };
        TEST_HOOKS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .map(|mut hooks| hooks.insert(point.to_owned(), action).is_none())
            .unwrap_or(false)
    }

    /// Remove all test hooks.
    #[cfg(feature = "fault_injection")]
    pub(crate) fn clear_for_tests() {
        if let Some(hooks) = TEST_HOOKS.get() {
            if let Ok(mut hooks) = hooks.lock() {
                hooks.clear();
            }
        }
    }

    #[cfg(all(test, feature = "fault_injection"))]
    mod tests {
        use super::*;

        #[test]
        fn test_hook_is_deterministic() {
            assert!(set_for_tests("unit", "delay:0"));
            assert_eq!(configured("unit"), Some(Action::Delay(0)));
            clear_for_tests();
        }
    }
}

/// Uncertain knowledge engine errors (PT0301–PT0307) — v0.87.0.
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[derive(Debug, Error)]
pub enum UncertainKnowledgeError {
    /// PT0301 — `@weight` value outside [0.0, 1.0] or NaN.
    #[error("rule weight must be in [0.0, 1.0]; got {value} (PT0301)")]
    InvalidWeight { value: f64 },

    /// PT0302 — `pg:fuzzy_match()` or `pg:token_set_ratio()` called when `pg_trgm` is not installed.
    #[error(
        "pg_trgm extension is required for pg:fuzzy_match(); \
         install it with CREATE EXTENSION pg_trgm (PT0302)"
    )]
    PgTrgmNotInstalled,

    /// PT0303 — Cyclic Datalog rule set detected with `prob_datalog_cyclic = off`.
    #[error(
        "cyclic rule set detected with probabilistic_datalog on; \
         set pg_ripple.prob_datalog_cyclic = on to allow approximate evaluation (PT0303)"
    )]
    CyclicRuleSetWithoutFlag,

    /// PT0304 — `pg:confidence()` called with all three arguments unbound.
    #[error(
        "pg:confidence() requires at least one bound argument \
         to prevent a full confidence table scan (PT0304)"
    )]
    ConfidenceAllUnbound,

    /// PT0305 — `pg:confidence()` or other `pg:` confidence function inside a SERVICE clause.
    #[error(
        "{fn_name}() cannot be evaluated at a remote SERVICE endpoint; \
         move the expression outside the SERVICE clause (PT0305)"
    )]
    ConfidenceFunctionInService { fn_name: String },

    /// PT0306 — `sh:severityWeight` value outside [0.0, ∞) or NaN.
    #[error(
        "sh:severityWeight must be a non-negative finite number; \
         got {value} (PT0306)"
    )]
    InvalidSeverityWeight { value: f64 },

    /// PT0307 — `prob_datalog_max_iterations` reached without convergence.
    #[error(
        "probabilistic Datalog did not converge after {max_iter} iterations \
         (final delta: {final_delta}); set pg_ripple.prob_datalog_cyclic_strict = off \
         to use partial result (PT0307)"
    )]
    ConvergenceTimeout { max_iter: i32, final_delta: f64 },

    /// PT0308 — fuzzy SPARQL input exceeds `pg_ripple.fuzzy_max_input_length` characters.
    #[error(
        "fuzzy SPARQL input exceeds pg_ripple.fuzzy_max_input_length characters ({limit}) — \
         truncate input or raise the GUC (PT0308)"
    )]
    FuzzyInputTooLong { limit: i32 },
}

/// Dictionary-layer errors (PT001–PT099).
// Q15-01: internal API field; kept for public API surface or future extension consumers.
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
// Q15-01: internal API field; kept for public API surface or future extension consumers.
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
// Q15-01: internal API field; kept for public API surface or future extension consumers.
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
// Q15-01: internal API field; kept for public API surface or future extension consumers.
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
// Q15-01: internal API field; kept for public API surface or future extension consumers.
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
// Q15-01: internal API field; kept for public API surface or future extension consumers.
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

/// LLM integration errors (PT700–PT702) — v0.49.0.
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[derive(Debug, Error)]
pub enum LlmError {
    /// PT700 — LLM endpoint not configured or unreachable.
    ///
    /// Raised when `pg_ripple.llm_endpoint` is empty or the HTTP call fails.
    #[error(
        "LLM endpoint unreachable or returned HTTP error: {detail} (PT700); \
         set pg_ripple.llm_endpoint to an OpenAI-compatible base URL"
    )]
    EndpointUnreachable { detail: String },

    /// PT701 — LLM response did not contain a SPARQL query.
    ///
    /// The HTTP call succeeded but the response body did not contain
    /// a recognisable SPARQL SELECT/CONSTRUCT/ASK/DESCRIBE statement.
    #[error(
        "LLM response did not contain a valid SPARQL query (PT701); \
         raw response: {raw}"
    )]
    NonSparqlResponse { raw: String },

    /// PT702 — LLM-generated SPARQL failed to parse.
    ///
    /// The response contained a SPARQL-looking string but `spargebra` rejected it.
    #[error(
        "LLM-generated SPARQL query failed to parse (PT702): {parse_error}; \
         query text: {query}"
    )]
    ParseFailed { parse_error: String, query: String },
}

/// SHACL-AF errors (PT480–PT481) — v0.53.0.
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[derive(Debug, Error)]
pub enum ShAFError {
    /// PT480 — `sh:rule` (SHACL-AF) detected but not compiled.
    ///
    /// Emitted when `load_shacl()` encounters one or more `sh:rule` triples
    /// in the supplied Turtle document and the `pg_ripple.inference_mode` is
    /// set to `'off'` (so compilation into the Datalog engine is disabled).
    /// Change `inference_mode` to `'on_demand'` or `'materialized'` to enable
    /// SHACL-AF rule compilation, or remove the `sh:rule` triples if they are
    /// not needed.
    #[error(
        "SHACL-AF sh:rule detected but not compiled (PT480): {count} rule(s) found; \
         set pg_ripple.inference_mode to 'on_demand' to enable compilation"
    )]
    ShAFRuleUnsupported { count: i32 },

    /// PT481 — `sh:sparql` (SHACL-SPARQL) constraint query failed to execute.
    #[error("SHACL-SPARQL constraint query execution failed (PT481): {detail}")]
    SparqlConstraintFailed { detail: String },
}

/// NS-RL evaluation errors (PT0461–PT0462) — v0.110.0.
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[derive(Debug, Error)]
pub enum NsrlEvaluationError {
    /// PT0461 — `evaluate_resolution()` gold graph is empty or does not exist.
    #[error("evaluate_resolution: gold graph '{graph}' is empty or does not exist (PT0461)")]
    GoldGraphEmpty { graph: String },

    /// PT0462 — `explain_rule()` rule not found.
    #[error("explain_rule: rule {rule_id} not found (PT0462)")]
    RuleNotFound { rule_id: i64 },
}

/// Lattice errors (PT540–PT541) — v0.36.0 / v0.45.0.
// Q15-01: internal API field; kept for public API surface or future extension consumers.
#[derive(Debug, Error)]
pub enum LatticeError {
    /// PT540 — lattice fixpoint did not converge within the iteration limit.
    ///
    /// Emitted when the `ON CONFLICT DO UPDATE` cycle count exceeds
    /// `pg_ripple.lattice_max_iterations`.  Partial results are returned.
    #[error(
        "lattice fixpoint did not converge within the iteration limit for \
         lattice '{lattice}'; partial results returned (PT540)"
    )]
    FixpointNotConverged { lattice: String },

    /// PT541 — user-supplied `join_fn` for `create_lattice()` could not be
    /// resolved as a PostgreSQL procedure reference.
    ///
    /// Raised at `create_lattice()` time when the supplied `join_fn` string
    /// does not parse as a valid `regprocedure` (i.e., PG cannot resolve it to
    /// a unique, existing function).  This prevents search-path injection via
    /// ambiguous function names.
    #[error(
        "lattice join function '{join_fn}' could not be resolved as a PostgreSQL \
         procedure reference (PT541); use a schema-qualified name such as \
         'myschema.myfunc(bigint, bigint)'"
    )]
    LatticeJoinFnInvalid { join_fn: String },
}
