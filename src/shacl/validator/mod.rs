//! SHACL validation engine.
//!
//! # Module layout (v0.114.0)
//!
//! | Sub-module  | Contents |
//! |---|---|
//! | `severity`  | `Violation` struct |
//! | `node`      | `node_conforms_to_shape`, `run_validate`, `validate_sync`, `process_validation_batch` |
//! | `property`  | `validate_property_shape`, `dispatch_constraint`, query helpers |
//! | `sparql`    | `validate_sync_with_shapes` |

pub mod node;
pub mod property;
pub mod severity;
pub mod sparql;

// Public API re-exports (keep exact visibility from original)
pub(crate) use node::node_conforms_to_shape;
pub use node::{process_validation_batch, run_validate, validate_sync};
pub use property::decode_id_safe;
pub use severity::Violation;

// pub(crate) helpers used by shacl/constraints/*.rs via `super::validator::{...}`
pub(crate) use property::{
    compare_dictionary_values, get_language_tag, get_value_ids, get_vp_table_name,
    shacl_value_matches, value_has_datatype, value_has_node_kind, value_has_rdf_type,
};
