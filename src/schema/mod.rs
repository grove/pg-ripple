//! pg_ripple schema DDL — all `extension_sql!` blocks that create
//! internal tables, sequences, views, and helper functions at
//! CREATE EXTENSION time.
//!
//! # Q13-02 (v0.85.0)
//! Split into sub-modules:
//! - [`tables`] — Foundation tables, sequences, and indexes (v0.1.0 – v0.28.0)
//! - [`views`]  — View catalog and supplementary tables (v0.11.0 – v0.55.0)
//! - [`triggers`] — Schema additions and trigger infrastructure (v0.56.0 – v0.73.0)
//! - [`rls`] — Late additions, RLS policies, and BIDI relay tables (v0.74.0+)

pub mod rls;
pub mod tables;
pub mod triggers;
pub mod views;
