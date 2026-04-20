//! Parallel W3C SPARQL 1.1 test runner.
//!
//! # Design
//!
//! Tests are distributed across worker threads via a `crossbeam_channel` work
//! queue.  Each worker owns a dedicated PostgreSQL connection and runs tests
//! sequentially within that connection, using transaction rollback for isolation.
//!
//! For the smoke subset a single thread is sufficient.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use super::loader;
use super::manifest::{TestCase, TestType};
use super::validator;

// ── Public types ──────────────────────────────────────────────────────────────

/// Configuration for a test run.
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Number of parallel worker threads (each owns a dedicated DB connection).
    pub threads: usize,
    /// Per-test timeout in seconds.  Tests that exceed this are marked `Timeout`.
    pub timeout_secs: u64,
    /// SPARQL test categories to run (e.g., `["aggregates", "optional"]`).
    pub categories: Vec<String>,
    /// Optional cap on total tests run (for smoke subsets).
    pub max_tests: Option<usize>,
    /// Path to the known-failures file (`tests/w3c/known_failures.txt`).
    pub known_failures_path: Option<std::path::PathBuf>,
}

/// The outcome of running a single test case.
#[derive(Debug, Clone)]
pub enum TestOutcome {
    /// Test passed.
    Pass,
    /// Test failed with a diagnostic message.
    Fail(String),
    /// Test was skipped (unsupported type, missing file, etc.).
    Skip(String),
    /// Test exceeded the per-test timeout.
    Timeout,
    /// Test was in the known-failures list and failed as expected.
    XFail(String),
    /// Test was in the known-failures list but unexpectedly passed.
    XPass,
}

impl TestOutcome {
    /// Returns `true` if this outcome represents an unexpected failure.
    pub fn is_unexpected_failure(&self) -> bool {
        matches!(
            self,
            TestOutcome::Fail(_) | TestOutcome::Timeout | TestOutcome::XPass
        )
    }
}

/// A single test result record.
#[derive(Debug, Clone)]
pub struct TestResult {
    pub test_case: TestCase,
    pub outcome: TestOutcome,
    pub duration_ms: u64,
}

/// Aggregated report for a complete test run.
#[derive(Debug, Default)]
pub struct RunReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub timeout: usize,
    pub xfail: usize,
    pub xpass: usize,
    pub results: Vec<TestResult>,
}

impl RunReport {
    pub fn add(&mut self, result: TestResult) {
        self.total += 1;
        match &result.outcome {
            TestOutcome::Pass => self.passed += 1,
            TestOutcome::Fail(_) => self.failed += 1,
            TestOutcome::Skip(_) => self.skipped += 1,
            TestOutcome::Timeout => self.timeout += 1,
            TestOutcome::XFail(_) => self.xfail += 1,
            TestOutcome::XPass => self.xpass += 1,
        }
        self.results.push(result);
    }

    /// Returns `true` if there are no unexpected failures (`Fail`, `Timeout`, or `XPass`).
    pub fn is_clean(&self) -> bool {
        self.failed == 0 && self.timeout == 0 && self.xpass == 0
    }

    /// Human-readable summary line.
    pub fn summary(&self) -> String {
        format!(
            "{} passed, {} failed, {} skipped, {} timeout, {} xfail, {} xpass / {} total",
            self.passed,
            self.failed,
            self.skipped,
            self.timeout,
            self.xfail,
            self.xpass,
            self.total,
        )
    }
}

// ── Main entry point ──────────────────────────────────────────────────────────

/// Load all test manifests from `data_dir` for the configured categories,
/// then run the tests using the given database connection string.
///
/// Returns a [`RunReport`] with per-test results.
pub fn run_test_suite(db_connect_string: &str, data_dir: &Path, config: &RunConfig) -> RunReport {
    let known_failures = config
        .known_failures_path
        .as_deref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| {
            s.lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
                .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
                .collect::<std::collections::HashSet<String>>()
        })
        .unwrap_or_default();
    let known_failures = Arc::new(known_failures);

    // Collect all test cases.
    let mut all_tests: Vec<TestCase> = Vec::new();
    for category in &config.categories {
        let manifest_path = data_dir.join(category).join("manifest.ttl");
        if !manifest_path.exists() {
            // Try the root-level manifest (some test suites have a flat layout).
            let alt = data_dir.join(format!("{category}-manifest.ttl"));
            if !alt.exists() {
                eprintln!("[w3c] manifest not found for category '{category}', skipping");
                continue;
            }
        }
        let manifest_path = if data_dir.join(category).join("manifest.ttl").exists() {
            data_dir.join(category).join("manifest.ttl")
        } else {
            data_dir.join(format!("{category}-manifest.ttl"))
        };
        match super::manifest::parse_manifest(&manifest_path, category) {
            Ok(tests) => all_tests.extend(tests),
            Err(e) => eprintln!("[w3c] parsing manifest for '{category}': {e}"),
        }
    }

    if let Some(max) = config.max_tests {
        all_tests.truncate(max);
    }

    if all_tests.is_empty() {
        return RunReport::default();
    }

    // Distribute tests across worker threads.
    let threads = config.threads.max(1);
    let timeout = Duration::from_secs(config.timeout_secs);

    if threads == 1 {
        run_sequential(db_connect_string, all_tests, &known_failures, timeout)
    } else {
        run_parallel(
            db_connect_string,
            all_tests,
            &known_failures,
            timeout,
            threads,
        )
    }
}

// ── Sequential runner (single connection) ─────────────────────────────────────

fn run_sequential(
    db_connect_string: &str,
    tests: Vec<TestCase>,
    known_failures: &std::collections::HashSet<String>,
    timeout: Duration,
) -> RunReport {
    let mut client = match postgres::Client::connect(db_connect_string, postgres::NoTls) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[w3c] cannot connect to database: {e}");
            return RunReport::default();
        }
    };

    // Install pg_ripple extension if not already present.
    let _ = client.execute("CREATE EXTENSION IF NOT EXISTS pg_ripple CASCADE", &[]);

    let mut report = RunReport::default();
    for tc in tests {
        let result = run_one_test(&mut client, &tc, known_failures, timeout);
        report.add(result);
    }
    report
}

// ── Parallel runner (multiple connections via crossbeam-channel) ───────────────

fn run_parallel(
    db_connect_string: &str,
    tests: Vec<TestCase>,
    known_failures: &std::collections::HashSet<String>,
    timeout: Duration,
    threads: usize,
) -> RunReport {
    let known_failures = Arc::new(known_failures.clone());
    let (work_tx, work_rx): (Sender<TestCase>, Receiver<TestCase>) =
        crossbeam_channel::bounded(tests.len());
    let (result_tx, result_rx): (Sender<TestResult>, Receiver<TestResult>) =
        crossbeam_channel::unbounded();

    for tc in tests {
        work_tx.send(tc).expect("work queue send");
    }
    drop(work_tx); // Signal workers to stop when queue is empty.

    let db_str = db_connect_string.to_string();
    let mut handles = Vec::with_capacity(threads);
    for _ in 0..threads {
        let work_rx = work_rx.clone();
        let result_tx = result_tx.clone();
        let known_failures = Arc::clone(&known_failures);
        let db_str = db_str.clone();

        let handle = std::thread::spawn(move || {
            let mut client = match postgres::Client::connect(&db_str, postgres::NoTls) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[w3c worker] cannot connect: {e}");
                    return;
                }
            };
            let _ = client.execute("CREATE EXTENSION IF NOT EXISTS pg_ripple CASCADE", &[]);
            while let Ok(tc) = work_rx.recv() {
                let result = run_one_test(&mut client, &tc, &known_failures, timeout);
                result_tx.send(result).expect("result send");
            }
        });
        handles.push(handle);
    }
    drop(result_tx); // Signal collector that workers are done once all handles finish.

    let mut report = RunReport::default();
    for result in result_rx {
        report.add(result);
    }

    for handle in handles {
        let _ = handle.join();
    }

    report
}

// ── Per-test execution ─────────────────────────────────────────────────────────

fn run_one_test(
    client: &mut postgres::Client,
    tc: &TestCase,
    known_failures: &std::collections::HashSet<String>,
    timeout: Duration,
) -> TestResult {
    let start = Instant::now();

    let outcome = run_test_inner(client, tc, timeout);

    let duration_ms = start.elapsed().as_millis() as u64;

    // Apply known-failures logic.
    let outcome = if known_failures.contains(&tc.iri) {
        match outcome {
            TestOutcome::Pass => TestOutcome::XPass,
            TestOutcome::Fail(msg) => TestOutcome::XFail(msg),
            other => other,
        }
    } else {
        outcome
    };

    TestResult {
        test_case: tc.clone(),
        outcome,
        duration_ms,
    }
}

fn run_test_inner(client: &mut postgres::Client, tc: &TestCase, timeout: Duration) -> TestOutcome {
    match tc.test_type {
        TestType::NotClassified => {
            return TestOutcome::Skip("test type not classified".into());
        }
        TestType::PositiveSyntax | TestType::NegativeSyntax => {
            return run_syntax_test(client, tc);
        }
        TestType::QueryEvaluation => {}
        TestType::UpdateEvaluation => {
            return run_update_test(client, tc, timeout);
        }
    }

    // QueryEvaluation test.
    let query_file = match &tc.query_file {
        Some(f) => f.clone(),
        None => return TestOutcome::Skip("no query file".into()),
    };

    let query_text = match std::fs::read_to_string(&query_file) {
        Ok(s) => s,
        Err(e) => return TestOutcome::Skip(format!("reading query: {e}")),
    };

    // Inject a BASE declaration so relative IRIs in GRAPH <file> clauses
    // resolve to the test data directory (matching how the W3C manifest parser
    // resolves qt:graphData IRIs relative to the manifest base URI).
    let query_text = prepend_base_if_needed(&query_file, query_text);

    let result_file = match &tc.result_file {
        Some(f) => f.clone(),
        None => return TestOutcome::Skip("no result file".into()),
    };

    // Run inside a transaction (rolled back after, for isolation).
    let mut tx = match client.transaction() {
        Ok(t) => t,
        Err(e) => return TestOutcome::Fail(format!("begin transaction: {e}")),
    };

    // Check timeout early (rough guard — actual timeout enforcement would need threads).
    if timeout < Duration::from_millis(1) {
        let _ = tx.rollback();
        return TestOutcome::Timeout;
    }

    // Load fixtures.
    if let Err(e) = loader::load_fixtures(&mut tx, &tc.data_files, &tc.named_graphs) {
        let _ = tx.rollback();
        return TestOutcome::Fail(format!("loading fixtures: {e}"));
    }

    // Load SERVICE mock data into named graphs and register mock endpoints (v0.42.0).
    // Each qt:serviceData entry specifies an endpoint URL and a data file.
    // We load the data into a named graph whose IRI is the endpoint URL,
    // then register the endpoint with graph_iri = endpoint URL so that
    // SERVICE clauses are rewritten to query the local named graph.
    if !tc.service_data.is_empty() {
        if let Err(e) = loader::load_service_data(&mut tx, &tc.service_data) {
            let _ = tx.rollback();
            return TestOutcome::Fail(format!("loading service data: {e}"));
        }
    }

    let ext = result_file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let validation = if ext == "ttl" || ext == "n3" {
        // CONSTRUCT / DESCRIBE query
        let _start = Instant::now();
        let result = validator::validate_construct(&mut tx, &query_text, &result_file);
        if _start.elapsed() > timeout {
            let _ = tx.rollback();
            return TestOutcome::Timeout;
        }
        result
    } else {
        // SELECT or ASK query
        let _start = Instant::now();
        let result = validator::validate_select_ask(&mut tx, &query_text, &result_file);
        if _start.elapsed() > timeout {
            let _ = tx.rollback();
            return TestOutcome::Timeout;
        }
        result
    };

    let _ = tx.rollback();

    match validation {
        validator::ValidationResult::Pass => TestOutcome::Pass,
        validator::ValidationResult::Fail(msg) => TestOutcome::Fail(msg),
        validator::ValidationResult::Skip(reason) => TestOutcome::Skip(reason),
    }
}

fn run_update_test(
    client: &mut postgres::Client,
    tc: &TestCase,
    _timeout: Duration,
) -> TestOutcome {
    let query_file = match &tc.query_file {
        Some(f) => f.clone(),
        None => return TestOutcome::Skip("no update query file".into()),
    };
    let query_text = match std::fs::read_to_string(&query_file) {
        Ok(s) => s,
        Err(e) => return TestOutcome::Skip(format!("reading update query: {e}")),
    };

    // Run inside a transaction: load initial state → execute update → compare → rollback.
    let mut tx = match client.transaction() {
        Ok(t) => t,
        Err(e) => return TestOutcome::Fail(format!("begin transaction: {e}")),
    };

    // Load initial fixtures.
    if let Err(e) = loader::load_fixtures(&mut tx, &tc.data_files, &tc.named_graphs) {
        let _ = tx.rollback();
        return TestOutcome::Fail(format!("loading fixtures: {e}"));
    }

    // Execute the SPARQL UPDATE (inside the same transaction).
    if let Err(e) = tx.execute("SELECT pg_ripple.sparql_update($1)", &[&query_text]) {
        let _ = tx.rollback();
        return TestOutcome::Fail(format!("sparql_update error: {e}"));
    }

    // Compare resulting graph state against expected.
    let result =
        validator::validate_update(&mut tx, &tc.update_result_data, &tc.update_result_graphs);

    let _ = tx.rollback();

    match result {
        validator::ValidationResult::Pass => TestOutcome::Pass,
        validator::ValidationResult::Fail(msg) => TestOutcome::Fail(msg),
        validator::ValidationResult::Skip(reason) => TestOutcome::Skip(reason),
    }
}

fn run_syntax_test(client: &mut postgres::Client, tc: &TestCase) -> TestOutcome {
    let query_file = match &tc.query_file {
        Some(f) => f.clone(),
        None => return TestOutcome::Skip("no query file".into()),
    };
    let query_text = match std::fs::read_to_string(&query_file) {
        Ok(s) => s,
        Err(e) => return TestOutcome::Skip(format!("reading query: {e}")),
    };
    let query_text = prepend_base_if_needed(&query_file, query_text);

    let mut tx = match client.transaction() {
        Ok(t) => t,
        Err(e) => return TestOutcome::Fail(format!("begin transaction: {e}")),
    };

    let expect_valid = tc.test_type == TestType::PositiveSyntax;
    let result = validator::validate_syntax(&mut tx, &query_text, expect_valid);
    let _ = tx.rollback();

    match result {
        validator::ValidationResult::Pass => TestOutcome::Pass,
        validator::ValidationResult::Fail(msg) => TestOutcome::Fail(msg),
        validator::ValidationResult::Skip(reason) => TestOutcome::Skip(reason),
    }
}

/// Prepend `BASE <file:///path/to/dir/>` to a SPARQL query if it does not
/// already have a BASE declaration.  This allows relative IRIs in GRAPH clauses
/// (e.g. `GRAPH <ng-01.ttl>`) to be resolved against the test data directory,
/// matching how the W3C manifest parser resolves `qt:graphData` references.
fn prepend_base_if_needed(query_file: &std::path::Path, query_text: String) -> String {
    // Only inject if the query doesn't already declare a BASE.
    let has_base = query_text
        .split_whitespace()
        .next()
        .map(|w| w.eq_ignore_ascii_case("BASE"))
        .unwrap_or(false);
    if has_base {
        return query_text;
    }
    if let Some(parent) = query_file.parent() {
        if let Ok(abs) = parent.canonicalize() {
            if let Some(dir) = abs.to_str() {
                let base = format!("file://{}/", dir.trim_end_matches('/'));
                return format!("BASE <{base}>\n{query_text}");
            }
        }
    }
    query_text
}
