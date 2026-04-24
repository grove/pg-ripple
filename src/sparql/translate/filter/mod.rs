//! SPARQL FILTER / expression translation.
//!
//! This module is split into two submodules (v0.53.0 architecture split):
//!
//! - [`filter_dispatch`] — SQL identifier sanitizer, modifier extraction,
//!   ORDER BY / VALUES / BIND translators (pattern-level dispatch utilities).
//! - [`filter_expr`] — SPARQL `Expression` AST compilation to SQL
//!   (comparisons, arithmetic, function calls, EXISTS, BIND value expressions).
//!
//! All public symbols are re-exported at this level so existing callers
//! (`use crate::sparql::translate::filter::translate_expr;`) continue to work.

pub mod filter_dispatch;
pub mod filter_expr;

#[allow(unused_imports)]
pub(crate) use filter_dispatch::{
    Modifiers, encode_ground_term, extract_modifiers, literal_lexical_value, sanitize_sql_ident,
    translate_order_by, translate_values,
};
#[allow(unused_imports)]
pub(crate) use filter_expr::{
    expr_as_text_sql, expr_is_raw_numeric, expr_is_raw_text, translate_comparison_sides,
    translate_expr, translate_expr_value, translate_expr_value_raw, translate_extend,
};
