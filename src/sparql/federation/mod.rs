//! SPARQL federation: SERVICE clause HTTP executor.
//!
//! # Q13-03 (v0.85.0)
//! Split into sub-modules:
//! - [`circuit`] — Circuit breaker and connection pool
//! - [`policy`]  — Endpoint policy, SSRF allowlist, adaptive timeout, result cache
//! - [`http`]    — Remote SPARQL endpoint HTTP execution
//! - [`decode`]  — Result decoding, health monitoring, vector endpoint federation

#![allow(clippy::type_complexity)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use pgrx::datum::DatumWithOid;
use pgrx::prelude::*;
use serde_json::Value as Json;
use spargebra::algebra::GraphPattern;
use spargebra::term::NamedNodePattern;

use crate::dictionary;

pub mod circuit;
pub mod decode;
pub mod http;
pub mod policy;

// Re-export the public API so callers can use `federation::function_name()` unchanged.
pub(crate) use circuit::get_agent_pub;
pub(crate) use decode::{
    collect_pattern_variables, encode_results, evict_expired_cache, get_view_variables,
    has_health_table, is_endpoint_healthy, record_health,
};
// get_endpoint_complexity is used within federation/decode.rs only; re-exported for
// future external consumers (federation health API).
#[allow(unused_imports)]
pub(crate) use decode::get_endpoint_complexity;
pub(crate) use http::{execute_remote, execute_remote_partial};
pub(crate) use policy::{
    check_endpoint_policy, effective_timeout_secs, get_all_graph_endpoints, get_graph_iri,
    get_local_view, get_service_graph_ids, is_endpoint_allowed,
};
// normalise_federation_url is used within policy.rs only; re-exported for
// future external consumers (URL normalisation utility).
#[allow(unused_imports)]
pub(crate) use policy::normalise_federation_url;
// Vector endpoint API: register_vector_endpoint is used from graphrag_admin.rs;
// is_vector_endpoint_registered and query_vector_endpoint are internal to decode.rs
// but re-exported as part of the public vector search API.
pub use decode::register_vector_endpoint;
#[allow(unused_imports)]
pub use decode::{is_vector_endpoint_registered, query_vector_endpoint};
