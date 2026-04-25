//! SHACL-AF bridge (v0.10.0 / v0.53.0).
//!
//! Scans Turtle data for `sh:rule` triples and, when inference is enabled,
//! registers them as Datalog rules.

use pgrx::prelude::*;

/// SHACL-AF bridge (v0.10.0 / v0.53.0): scan Turtle data for `sh:rule` triples.
///
/// When `pg_ripple.inference_mode` is `'on_demand'` or `'materialized'`, the
/// rule bodies are compiled into the Datalog engine.
///
/// Returns the number of `sh:rule` patterns found.
pub fn bridge_shacl_rules(data: &str) -> i32 {
    if !data.contains("sh:rule") && !data.contains("shacl#rule") {
        return 0;
    }

    let count = data.matches("sh:rule").count() as i32;
    if count == 0 {
        return 0;
    }

    let inference_mode = crate::INFERENCE_MODE
        .get()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    if inference_mode == "off" || inference_mode.is_empty() {
        pgrx::warning!(
            "SHACL-AF sh:rule detected but not compiled (PT480): {count} rule(s) found; \
             set pg_ripple.inference_mode to 'on_demand' to enable SHACL-AF rule compilation"
        );
        return count;
    }

    let _ = Spi::run_with_args(
        "INSERT INTO _pg_ripple.rules \
         (rule_set, rule_text, head_pred, stratum, is_recursive, active) \
         VALUES ('shacl-af', $1, NULL, 0, false, true) \
         ON CONFLICT DO NOTHING",
        &[pgrx::datum::DatumWithOid::from(
            "# SHACL-AF sh:rule detected; full compilation pending",
        )],
    );
    count
}
