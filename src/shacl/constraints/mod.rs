//! Per-constraint-family helpers for SHACL property shape validation.
//!
//! Each sub-module handles one family of `sh:*` constraints.  The top-level
//! dispatcher lives in `crate::shacl::validate_property_shape`.

pub mod count;
pub mod logical;
pub mod property_path;
pub mod relational;
pub mod shape_based;
pub mod sparql_constraint;
pub mod string_based;
pub mod value_type;

/// Context passed to every per-constraint checker function.
pub struct ConstraintArgs<'a> {
    /// Focus-node dictionary ID.
    pub focus: i64,
    /// Pre-computed value count along `path_id` for this focus node.
    pub count: i64,
    /// Path predicate dictionary ID.
    pub path_id: i64,
    /// Graph scope: `0` for the default graph, `> 0` for a named graph,
    /// `< 0` means "all graphs".
    pub graph_id: i64,
    /// Shape IRI string (for violation records).
    pub shape_iri: &'a str,
    /// Path IRI string (for violation records).
    pub path_iri: &'a str,
    /// All loaded shapes -- needed for sh:node / sh:or / sh:and / sh:not
    /// recursive validation.
    pub all_shapes: &'a [super::Shape],
}

// Re-export items that sub-modules need via `super::*`.
pub use super::Violation;
pub(crate) use super::node_conforms_to_shape;
pub(crate) use super::{
    compare_dictionary_values, encode_shacl_in_value, get_language_tag, get_value_ids,
    value_has_datatype, value_has_node_kind, value_has_rdf_type,
};
