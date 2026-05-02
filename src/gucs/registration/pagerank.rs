//! GUC registration for PageRank and graph analytics engine (v0.88.0).

#[allow(unused_imports)]
use crate::gucs::*;
use pgrx::guc::{GucContext, GucFlags};
#[allow(unused_imports)]
use pgrx::prelude::*;

/// Validate `pagerank_dangling_policy`: `'redistribute'` or `'ignore'`.
#[allow(dead_code)]
unsafe extern "C-unwind" fn check_pagerank_dangling_policy(
    newval: *mut *mut std::ffi::c_char,
    _extra: *mut *mut std::ffi::c_void,
    _source: pgrx::pg_sys::GucSource::Type,
) -> bool {
    if newval.is_null() {
        return true;
    }
    // SAFETY: newval is a GUC check-hook argument; valid for the duration of this call.
    let s = unsafe {
        if (*newval).is_null() {
            return true;
        }
        std::ffi::CStr::from_ptr(*newval).to_str().unwrap_or("")
    };
    matches!(s, "redistribute" | "ignore")
}

pub(crate) fn register() {
    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.pagerank_enabled",
        c"Master switch for the Datalog-native PageRank engine. Default off. (v0.88.0 PR-DATALOG-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_ENABLED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.pagerank_rules",
        c"Comma-separated IRI list of edge predicates for PageRank. \
          Empty string means all object-valued predicates. (v0.88.0 PR-DATALOG-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_RULES,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.pagerank_max_iterations",
        c"Maximum PageRank iteration count before termination. Default 100. (v0.88.0 PR-ITER-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_MAX_ITERATIONS,
        1,
        10000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_float_guc(
        c"pg_ripple.pagerank_convergence_delta",
        c"Convergence threshold for PageRank: iteration stops when max delta < this value. \
          Default 0.0001. (v0.88.0 PR-ITER-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_CONVERGENCE_DELTA,
        0.0,
        1.0,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_float_guc(
        c"pg_ripple.pagerank_damping",
        c"PageRank damping factor. Default 0.85. (v0.88.0 PR-DAMPING-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_DAMPING,
        0.0,
        1.0,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.pagerank_dangling_policy",
        c"Dangling-node redistribution policy: 'redistribute' (default) or 'ignore'. \
          (v0.88.0 PR-DAMPING-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_DANGLING_POLICY,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.pagerank_include_blank_nodes",
        c"When off (default), blank nodes are excluded from the PageRank edge set. \
          (v0.88.0 PR-BLANK-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_INCLUDE_BLANK_NODES,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.pagerank_on_demand",
        c"When on, pg:pagerank() triggers an on-demand run if the view is stale. \
          Default off. (v0.88.0 PR-SPARQL-FN-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_ON_DEMAND,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.pagerank_incremental",
        c"Enable pg-trickle incremental K-hop refresh for PageRank. Default off. \
          (v0.88.0 PR-TRICKLE-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_INCREMENTAL,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.pagerank_khop_limit",
        c"Maximum K-hop propagation depth for incremental PageRank updates. \
          Default 30. (v0.88.0 PR-TRICKLE-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_KHOP_LIMIT,
        1,
        1000,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_string_guc(
        c"pg_ripple.pagerank_refresh_schedule",
        c"Cron expression for scheduled full pagerank_run(). Default '0 3 * * *'. \
          (v0.88.0 PR-TRICKLE-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_REFRESH_SCHEDULE,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.pagerank_confidence_weighted",
        c"Multiply edge weights by confidence from _pg_ripple.confidence. \
          Default off. (v0.88.0 PR-CONF-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_CONFIDENCE_WEIGHTED,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_float_guc(
        c"pg_ripple.pagerank_confidence_default",
        c"Default confidence weight for edges without a confidence row. \
          Default 1.0. (v0.88.0 PR-CONF-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_CONFIDENCE_DEFAULT,
        0.0,
        1.0,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.pagerank_partition",
        c"Enable graph-partitioned parallel PageRank computation. Default off. \
          (v0.88.0 PR-PARTITION-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_PARTITION,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_float_guc(
        c"pg_ripple.pagerank_selective_threshold",
        c"Minimum score below which dirty nodes skip immediate re-propagation. \
          Default 0.0 (disabled). (v0.88.0 PR-SELECTIVE-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_SELECTIVE_THRESHOLD,
        0.0,
        1.0,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.pagerank_federation_blend",
        c"Fetch remote SERVICE edges into a local temp graph before pagerank_run(). \
          Default off. (v0.88.0 PR-FED-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_FEDERATION_BLEND,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_int_guc(
        c"pg_ripple.pagerank_queue_warn_threshold",
        c"Log a WARNING when the dirty-edges queue exceeds this count. \
          Default 100000. (v0.88.0 PR-IVM-METRICS-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_QUEUE_WARN_THRESHOLD,
        1,
        i32::MAX,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.pagerank_trickle_confidence_attenuation",
        c"Attenuate K-hop rank deltas by edge confidence when incremental mode is active. \
          Default on. (v0.88.0 PR-TRICKLE-CONF-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_TRICKLE_CONFIDENCE_ATTENUATION,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_bool_guc(
        c"pg_ripple.pagerank_probabilistic",
        c"Enable probabilistic PageRank via @weight Datalog rules. Default off. \
          Requires probabilistic_datalog = on. (v0.88.0 PR-PROB-DATALOG-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_PROBABILISTIC,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_float_guc(
        c"pg_ripple.pagerank_shacl_threshold",
        c"Exclude nodes whose shacl_score() is below this threshold from PageRank. \
          Default 0.5; 0.0 disables SHACL-based exclusion. (v0.88.0 PR-SHACL-01)",
        c"",
        &crate::gucs::pagerank::PAGERANK_SHACL_THRESHOLD,
        0.0,
        1.0,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_float_guc(
        c"pg_ripple.federation_minimum_confidence",
        c"Minimum confidence for remote SERVICE edges in federation blend mode. \
          Default 0.5. (v0.88.0 PR-FED-CONF-01)",
        c"",
        &crate::gucs::pagerank::FEDERATION_MINIMUM_CONFIDENCE,
        0.0,
        1.0,
        GucContext::Userset,
        GucFlags::default(),
    );

    pgrx::GucRegistry::define_float_guc(
        c"pg_ripple.katz_alpha",
        c"Attenuation factor for Katz centrality computation. \
          Default 0.01. (v0.88.0 PR-CENTRALITY-01)",
        c"",
        &crate::gucs::pagerank::KATZ_ALPHA,
        0.0,
        1.0,
        GucContext::Userset,
        GucFlags::default(),
    );
}
