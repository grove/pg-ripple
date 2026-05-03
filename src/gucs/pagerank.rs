//! GUC parameters for the PageRank and graph analytics engine (v0.88.0).

// ─── v0.88.0 PageRank GUCs ────────────────────────────────────────────────────

/// GUC: master switch for the Datalog-native PageRank engine (v0.88.0 PR-DATALOG-01).
pub static PAGERANK_ENABLED: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: comma-separated IRI list of edge predicates for PageRank.
/// Empty string = all object-valued predicates. (v0.88.0 PR-DATALOG-01)
pub static PAGERANK_RULES: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: maximum PageRank iteration count (v0.88.0 PR-ITER-01).
pub static PAGERANK_MAX_ITERATIONS: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(100);

/// GUC: convergence threshold for PageRank (v0.88.0 PR-ITER-01).
/// Iteration stops when the maximum score delta falls below this value.
pub static PAGERANK_CONVERGENCE_DELTA: pgrx::GucSetting<f64> = pgrx::GucSetting::<f64>::new(0.0001);

/// GUC: PageRank damping factor (v0.88.0 PR-DAMPING-01). Default 0.85.
pub static PAGERANK_DAMPING: pgrx::GucSetting<f64> = pgrx::GucSetting::<f64>::new(0.85);

/// GUC: dangling-node redistribution policy: 'redistribute' | 'ignore' (v0.88.0 PR-DAMPING-01).
pub static PAGERANK_DANGLING_POLICY: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: include blank nodes in PageRank computation (v0.88.0 PR-BLANK-01). Default false.
pub static PAGERANK_INCLUDE_BLANK_NODES: pgrx::GucSetting<bool> =
    pgrx::GucSetting::<bool>::new(false);

/// GUC: trigger on-demand PageRank run when view is stale (v0.88.0 PR-SPARQL-FN-01).
pub static PAGERANK_ON_DEMAND: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: enable pg-trickle incremental K-hop refresh (v0.88.0 PR-TRICKLE-01).
pub static PAGERANK_INCREMENTAL: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: maximum K-hop propagation depth for incremental updates (v0.88.0 PR-TRICKLE-01).
pub static PAGERANK_KHOP_LIMIT: pgrx::GucSetting<i32> = pgrx::GucSetting::<i32>::new(30);

/// GUC: cron expression for scheduled full pagerank_run() (v0.88.0 PR-TRICKLE-01).
pub static PAGERANK_REFRESH_SCHEDULE: pgrx::GucSetting<Option<std::ffi::CString>> =
    pgrx::GucSetting::<Option<std::ffi::CString>>::new(None);

/// GUC: multiply edge weights by confidence from _pg_ripple.confidence (v0.88.0 PR-CONF-01).
pub static PAGERANK_CONFIDENCE_WEIGHTED: pgrx::GucSetting<bool> =
    pgrx::GucSetting::<bool>::new(false);

/// GUC: default confidence weight for edges without a confidence row (v0.88.0 PR-CONF-01).
pub static PAGERANK_CONFIDENCE_DEFAULT: pgrx::GucSetting<f64> = pgrx::GucSetting::<f64>::new(1.0);

/// GUC: enable graph-partitioned parallel PageRank computation (v0.88.0 PR-PARTITION-01).
pub static PAGERANK_PARTITION: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: minimum score below which dirty nodes skip immediate re-propagation (v0.88.0 PR-SELECTIVE-01).
pub static PAGERANK_SELECTIVE_THRESHOLD: pgrx::GucSetting<f64> = pgrx::GucSetting::<f64>::new(0.0);

/// GUC: fetch remote SERVICE edges for global-graph ranking (v0.88.0 PR-FED-01).
pub static PAGERANK_FEDERATION_BLEND: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: log a WARNING when the dirty-edges queue exceeds this count (v0.88.0 PR-IVM-METRICS-01).
pub static PAGERANK_QUEUE_WARN_THRESHOLD: pgrx::GucSetting<i32> =
    pgrx::GucSetting::<i32>::new(100000);

/// GUC: attenuate K-hop rank deltas by edge confidence (v0.88.0 PR-TRICKLE-CONF-01).
pub static PAGERANK_TRICKLE_CONFIDENCE_ATTENUATION: pgrx::GucSetting<bool> =
    pgrx::GucSetting::<bool>::new(true);

/// GUC: enable probabilistic PageRank via @weight Datalog rules (v0.88.0 PR-PROB-DATALOG-01).
pub static PAGERANK_PROBABILISTIC: pgrx::GucSetting<bool> = pgrx::GucSetting::<bool>::new(false);

/// GUC: exclude nodes whose shacl_score() is below this threshold (v0.88.0 PR-SHACL-01).
/// Range [0.0, 1.0]; 0.0 disables SHACL-based exclusion.
pub static PAGERANK_SHACL_THRESHOLD: pgrx::GucSetting<f64> = pgrx::GucSetting::<f64>::new(0.5);

/// GUC: minimum confidence for remote SERVICE edges in federation blend mode (v0.88.0 PR-FED-CONF-01).
pub static FEDERATION_MINIMUM_CONFIDENCE: pgrx::GucSetting<f64> = pgrx::GucSetting::<f64>::new(0.5);

/// GUC: attenuation factor for Katz centrality (v0.88.0 PR-CENTRALITY-01).
pub static KATZ_ALPHA: pgrx::GucSetting<f64> = pgrx::GucSetting::<f64>::new(0.01);
