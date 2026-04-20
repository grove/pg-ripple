//! Unified conformance report: per-suite JSON artifact.
//!
//! Writes (or updates) `tests/conformance/report.json` with the latest
//! pass/fail/skip/timeout counts from each suite.  The file is committed as
//! a CI artifact and updated on every run.

use std::path::Path;

use super::runner::RunReport;

/// Write the suite reports to the unified JSON report file.
///
/// If the file already exists its content is merged (existing suites whose
/// keys do not appear in `reports` are preserved).
pub fn write_report(reports: &[&RunReport], output_path: &Path) -> std::io::Result<()> {
    // Load existing report if present.
    let mut root: serde_json::Map<String, serde_json::Value> = if output_path.exists() {
        let raw = std::fs::read_to_string(output_path)?;
        serde_json::from_str(&raw).unwrap_or_default()
    } else {
        serde_json::Map::new()
    };

    for report in reports {
        root.insert(report.suite.clone(), report.to_json());
    }

    let json = serde_json::to_string_pretty(&serde_json::Value::Object(root))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(output_path, json)?;
    Ok(())
}
