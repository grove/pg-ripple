//! SPARQL algebra → SQL translation sub-modules.
//!
//! Each module handles one algebra node family.  The main translation logic
//! lives in `crate::sparql::sqlgen` and delegates here via the exported
//! per-node translation functions.
//!
//! # Module layout
//!
//! | Module | Handles |
//! |--------|---------|
//! | `bgp`  | Basic Graph Pattern — triple patterns + reorder |
//! | `join` | `Join` (inner join of two patterns) |
//! | `left_join` | `LeftJoin` (optional / OPTIONAL clause) |
//! | `union` | `Union` (UNION ALL) |
//! | `filter` | `Filter` (WHERE filter expressions) |
//! | `graph` | `Graph` (named-graph scoped patterns) |
//! | `group` | `Group` (GROUP BY + aggregates) |
//! | `distinct` | `Distinct` / `Reduced` (SELECT DISTINCT) |
//!
//! ## Shared context (`TranslateCtx`)
//!
//! A `TranslateCtx` is threaded through every recursive call, carrying:
//! - The per-query IRI/literal encoding cache.
//! - A handle to the `PredicateCatalog` for VP-table lookups.
//! - Query-level state (alias counter, path counter, raw-numeric variable set).

pub mod bgp;
pub mod distinct;
pub mod filter;
pub mod graph;
pub mod group;
pub mod join;
pub mod left_join;
pub mod union;
