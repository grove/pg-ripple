//! SHACL Core validation engine for pg_ripple v0.7.0.
//!
//! ## Module layout
//!
//! | Sub-module | Contents |
//! |---|---|
//! | `parser`    | SHACL Turtle parser (hand-rolled subset) |
//! | `validator` | Focus-node collection, constraint dispatch, sync/async validation |
//! | `af_rules`  | SHACL-AF `sh:rule` bridge (Datalog registration) |
//! | `spi`       | Shape persistence and loading via SPI |
//! | `hints`     | Query-planner hints + pg_trickle DAG monitors |
//! | `constraints` | Per-constraint-family checkers |

pub mod af_rules;
pub mod constraints;
pub mod hints;
pub mod parser;
pub mod spi;
pub mod validator;

use serde::{Deserialize, Serialize};

// ─── Shared Shape IR ─────────────────────────────────────────────────────────

/// The type of SHACL target declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShapeTarget {
    /// `sh:targetClass <IRI>` — all instances of a class.
    Class(String),
    /// `sh:targetNode <IRI>` — specific node(s).
    Node(Vec<String>),
    /// `sh:targetSubjectsOf <IRI>` — subjects of a predicate.
    SubjectsOf(String),
    /// `sh:targetObjectsOf <IRI>` — objects of a predicate.
    ObjectsOf(String),
    /// No explicit target.
    None,
}

/// A single SHACL constraint within a shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShapeConstraint {
    MinCount(i64),
    MaxCount(i64),
    Datatype(String),
    In(Vec<String>),
    Pattern(String, Option<String>),
    Class(String),
    Node(String),
    Or(Vec<String>),
    And(Vec<String>),
    Not(String),
    QualifiedValueShape {
        shape_iri: String,
        min_count: Option<i64>,
        max_count: Option<i64>,
    },
    HasValue(String),
    NodeKind(String),
    LanguageIn(Vec<String>),
    UniqueLang,
    LessThan(String),
    LessThanOrEquals(String),
    GreaterThan(String),
    Closed {
        ignored_properties: Vec<String>,
    },
    Equals(String),
    Disjoint(String),
    MinLength(i64),
    MaxLength(i64),
    Xone(Vec<String>),
    MinExclusive(String),
    MaxExclusive(String),
    MinInclusive(String),
    MaxInclusive(String),
    SparqlConstraint {
        sparql_query: String,
        message: Option<String>,
    },
}

/// A SHACL PropertyShape (associated with a path via `sh:path`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyShape {
    pub shape_iri: String,
    pub path_iri: String,
    pub constraints: Vec<ShapeConstraint>,
}

/// A SHACL NodeShape or PropertyShape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shape {
    pub shape_iri: String,
    pub target: ShapeTarget,
    pub constraints: Vec<ShapeConstraint>,
    pub properties: Vec<PropertyShape>,
    pub deactivated: bool,
}

// ─── Re-exports (preserve original public API paths) ─────────────────────────

pub use hints::{compile_dag_monitors, drop_dag_monitors, list_dag_monitors};
pub use spi::parse_and_store_shapes;
pub use validator::{
    Violation, decode_id_safe, process_validation_batch, run_validate, validate_sync,
};

// pub(crate) re-exports consumed by constraints/* and other internal callers
pub(crate) use validator::{
    compare_dictionary_values, encode_shacl_in_value, get_language_tag, get_value_ids,
    node_conforms_to_shape, value_has_datatype, value_has_node_kind, value_has_rdf_type,
};
