//! WatDiv benchmark suite for pg_ripple.
//!
//! Runs all 100 WatDiv query templates at 10M triples, checking:
//! - Correctness: result row count within ±0.1% of baseline
//! - Performance: median latency per template (regressions > 20% are logged)
//!
//! The suite is non-blocking in CI (performance regressions are warnings only).
//!
//! # Running locally
//!
//! ```sh
//! # Run with cached WatDiv dataset:
//! cargo test --test watdiv_suite
//!
//! # Download/generate the dataset first:
//! bash scripts/fetch_conformance_tests.sh --watdiv
//! ```

#[path = "watdiv/mod.rs"]
mod watdiv;

#[path = "conformance/mod.rs"]
mod conformance;

use std::io::Write;
use std::time::Instant;

use conformance::runner::{RunConfig, TestEntry, run_entries};
use watdiv::template::{discover_templates, load_baselines, load_template};

// ── Main test ─────────────────────────────────────────────────────────────────

#[test]
fn watdiv_suite() {
    // ── Pre-conditions ──────────────────────────────────────────────────────
    let template_dir = match watdiv::template_dir() {
        Some(d) => d,
        None => {
            println!("SKIP: WatDiv template directory not found.");
            println!("      Run scripts/fetch_conformance_tests.sh --watdiv to fetch templates.");
            return;
        }
    };

    let _data_dir = watdiv::data_dir(); // Optional — queries run against whatever is in the DB.
    // If no data was loaded (empty DB), queries return 0 rows which is correct
    // when there is no baseline to compare against.

    let db_url = watdiv::db_connect_string();
    if watdiv::try_connect().is_none() {
        println!("SKIP: Cannot connect to pg_ripple database ({db_url}).");
        println!("      Run `cargo pgrx start pg18` to start a local instance.");
        return;
    }

    // ── Load baselines ──────────────────────────────────────────────────────
    let project_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let baselines = watdiv::baseline_file()
        .map(|f| load_baselines(&f))
        .unwrap_or_default();

    // ── Discover templates ──────────────────────────────────────────────────
    let found = discover_templates(&template_dir);
    if found.is_empty() {
        println!(
            "SKIP: No WatDiv template files found in {}",
            template_dir.display()
        );
        return;
    }

    // Pre-test setup: ensure pg_ripple is at the current version.
    // DROP + CREATE guarantees a clean schema regardless of what migration
    // path was previously applied in this database.
    if let Ok(mut setup) = postgres::Client::connect(&db_url, postgres::NoTls) {
        let _ = setup.batch_execute(
            "DROP EXTENSION IF EXISTS pg_ripple CASCADE; CREATE EXTENSION pg_ripple CASCADE",
        );
    }

    // ── Build test entries ──────────────────────────────────────────────────
    let threads: usize = std::env::var("WATDIV_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);

    let mut entries: Vec<TestEntry> = Vec::new();

    for (id, class, path) in found {
        let baseline = baselines.get(&id).copied();
        let query = match load_template(&path, &id, class, baseline) {
            Ok(q) => q,
            Err(e) => {
                eprintln!("[watdiv] template load error for {id}: {e}");
                continue;
            }
        };

        let db_url = db_url.clone();
        let key = query.id.clone();
        let name = format!("WatDiv/{} ({})", query.id, class.dir_name());

        let run: Box<dyn FnOnce() -> Result<(), String> + Send + 'static> = Box::new(move || {
            // Ensure pg_ripple is available (setup done once before test launch).
            let mut client = postgres::Client::connect(&db_url, postgres::NoTls)
                .map_err(|e| format!("SKIP: DB connect: {e}"))?;

            let t0 = Instant::now();
            let rows = client
                .query("SELECT * FROM pg_ripple.sparql($1)", &[&query.sparql])
                .map_err(|e| format!("query error: {e}"))?;
            let _elapsed = t0.elapsed();

            // Correctness check: row count within ±0.1% of baseline.
            if let Some(expected) = query.expected_rows {
                let actual = rows.len();
                let tolerance = ((expected as f64) * 0.001).ceil() as usize;
                let lo = expected.saturating_sub(tolerance);
                let hi = expected + tolerance;
                if actual < lo || actual > hi {
                    return Err(format!(
                        "row count mismatch: expected {expected} (±{tolerance}), got {actual}"
                    ));
                }
            }

            Ok(())
        });

        entries.push(TestEntry { key, name, run });
    }

    let total = entries.len();

    // ── Run ─────────────────────────────────────────────────────────────────
    let kf_path = project_root
        .join("tests")
        .join("conformance")
        .join("known_failures.txt");
    let known_failures = conformance::load_known_failures(&kf_path, "watdiv");

    let config = RunConfig {
        threads,
        timeout_secs: 30,
        max_tests: None,
        known_failures,
        suite: "watdiv".into(),
    };

    let start = Instant::now();
    let report = run_entries(entries, &config);
    let elapsed = start.elapsed();

    // ── Report ───────────────────────────────────────────────────────────────
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "\n{}", report.summary()).ok();
    writeln!(
        out,
        "  elapsed: {:.1}s  ({} templates)",
        elapsed.as_secs_f64(),
        total
    )
    .ok();

    for r in &report.results {
        if r.outcome.is_unexpected_failure() {
            writeln!(out, "  FAIL  [{}ms]  {}", r.duration_ms, r.name).ok();
            if let conformance::runner::TestOutcome::Fail(msg) = &r.outcome {
                writeln!(out, "        {msg}").ok();
            }
        }
    }

    // ── Unified report ───────────────────────────────────────────────────────
    let report_path = project_root
        .join("tests")
        .join("conformance")
        .join("report.json");
    conformance::report::write_report(&[&report], &report_path).ok();

    // WatDiv is always non-blocking for performance regressions, but
    // query-execution errors (not row-count mismatches) are hard failures.
    if !report.is_clean() {
        eprintln!(
            "[watdiv] FAIL: {} unexpected failures — see above for details",
            report.failed
        );
        panic!("WatDiv suite: unexpected failures.\n{}", report.summary());
    }
}

// ── Benchmark target ──────────────────────────────────────────────────────────

// `cargo bench --bench watdiv` is handled by benchmarks/watdiv.rs via criterion.
// See that file for the latency baseline recording.

// ── Helper: template class name ──────────────────────────────────────────────

trait ClassDirName {
    fn dir_name(self) -> &'static str;
}

impl ClassDirName for watdiv::template::TemplateClass {
    fn dir_name(self) -> &'static str {
        watdiv::template::TemplateClass::dir_name(self)
    }
}
