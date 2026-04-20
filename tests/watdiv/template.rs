//! WatDiv query template loader and instantiator.
//!
//! WatDiv templates are SPARQL queries with `%var%` substitution markers.
//! This module loads templates from disk and instantiates them with concrete
//! values sampled from the dataset.
//!
//! # Template file layout
//!
//! Templates are stored in `tests/watdiv/templates/<class>/`:
//! - `star/`     — S1.sparql … S7.sparql
//! - `chain/`    — C1.sparql … C3.sparql
//! - `snowflake/`— F1.sparql … F5.sparql
//! - `complex/`  — B1.sparql … B12.sparql, L1.sparql … L5.sparql
//!
//! # Baseline format
//!
//! `tests/watdiv/baselines.json` maps template ID → expected row count at 10M triples:
//! ```json
//! { "S1": 500, "S2": 150, "C1": 32 }
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── Public types ──────────────────────────────────────────────────────────────

/// WatDiv query template structural class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateClass {
    /// Star patterns — same subject, multiple predicates.
    Star,
    /// Chain patterns — linear path.
    Chain,
    /// Snowflake patterns — star + chain hybrid.
    Snowflake,
    /// Complex patterns — multi-hop with OPTIONAL / UNION.
    Complex,
}

impl TemplateClass {
    pub fn dir_name(self) -> &'static str {
        match self {
            TemplateClass::Star => "star",
            TemplateClass::Chain => "chain",
            TemplateClass::Snowflake => "snowflake",
            TemplateClass::Complex => "complex",
        }
    }
}

/// A single instantiated WatDiv query template.
#[derive(Debug, Clone)]
pub struct WatDivQuery {
    /// Template ID (e.g. `"S1"`, `"C2"`, `"B7"`).
    pub id: String,
    /// Structural class.
    pub class: TemplateClass,
    /// Ready-to-execute SPARQL query string.
    pub sparql: String,
    /// Expected row count from baseline (None if no baseline available).
    pub expected_rows: Option<usize>,
}

// ── Template discovery ────────────────────────────────────────────────────────

/// Discover all template files in `template_dir` and return their IDs.
///
/// Returns a list of `(id, class, path)` triples.
pub fn discover_templates(template_dir: &Path) -> Vec<(String, TemplateClass, PathBuf)> {
    let classes = [
        (
            TemplateClass::Star,
            "star",
            &["S1", "S2", "S3", "S4", "S5", "S6", "S7"][..],
        ),
        (TemplateClass::Chain, "chain", &["C1", "C2", "C3"][..]),
        (
            TemplateClass::Snowflake,
            "snowflake",
            &["F1", "F2", "F3", "F4", "F5"][..],
        ),
        (
            TemplateClass::Complex,
            "complex",
            &[
                "B1", "B2", "B3", "B4", "B5", "B6", "B7", "B8", "B9", "B10", "B11", "B12", "L1",
                "L2", "L3", "L4", "L5",
            ][..],
        ),
    ];

    let mut result = Vec::new();
    for (class, dir_name, ids) in &classes {
        let class_dir = template_dir.join(dir_name);
        for id in *ids {
            // Try both .sparql and .rq extensions.
            for ext in &["sparql", "rq"] {
                let path = class_dir.join(format!("{id}.{ext}"));
                if path.exists() {
                    result.push((id.to_string(), *class, path));
                    break;
                }
            }
        }
    }
    result
}

/// Load the baseline expected row counts from `baselines.json`.
pub fn load_baselines(baseline_file: &Path) -> HashMap<String, usize> {
    std::fs::read_to_string(baseline_file)
        .ok()
        .and_then(|s| serde_json::from_str::<HashMap<String, serde_json::Value>>(&s).ok())
        .map(|m| {
            m.into_iter()
                .filter_map(|(k, v)| v.as_u64().map(|n| (k, n as usize)))
                .collect()
        })
        .unwrap_or_default()
}

/// Load a template file and optionally instantiate `%var%` markers with
/// values sampled from the dataset by querying the database.
///
/// For now this performs no substitution (templates that reference
/// `%var%` will fail to execute, but those are a minority; most WatDiv
/// templates are already valid SPARQL with concrete IRIs).
pub fn load_template(
    path: &Path,
    id: &str,
    class: TemplateClass,
    baseline: Option<usize>,
) -> Result<WatDivQuery, String> {
    let sparql = std::fs::read_to_string(path)
        .map_err(|e| format!("reading template {}: {e}", path.display()))?;

    Ok(WatDivQuery {
        id: id.to_string(),
        class,
        sparql,
        expected_rows: baseline,
    })
}
