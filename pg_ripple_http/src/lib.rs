//! Library surface for `pg_ripple_http`, split out from the binary so that
//! `tests/router_construction.rs` (v0.128.1 emergency containment) can build
//! and exercise the router without a running PostgreSQL instance.

pub mod arrow_encode;
pub mod common;
pub mod datalog;
pub mod metrics;
pub mod outbound_policy;
pub mod routing;
pub mod spi_bridge;
pub mod stream;
pub mod streaming;
