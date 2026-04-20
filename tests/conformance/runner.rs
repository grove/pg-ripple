//! Generic parallel test runner for conformance suites.
//!
//! This runner is suite-agnostic: it accepts a list of pre-built [`TestEntry`]
//! items (each carrying a name, IRI, and a closure that runs the test), and
//! distributes them across a thread pool.
//!
//! Individual suites (W3C, Jena, WatDiv) build their `TestEntry` list from
//! their own manifest format and then call [`run_entries`].

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::bounded;

// ── Public types ──────────────────────────────────────────────────────────────

/// Configuration for a conformance test run.
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Number of parallel worker threads.
    pub threads: usize,
    /// Per-test wall-clock timeout in seconds.
    pub timeout_secs: u64,
    /// Optional cap on total tests run (for smoke subsets).
    pub max_tests: Option<usize>,
    /// Set of known-failure keys (test IRIs or template IDs, without suite prefix).
    pub known_failures: HashSet<String>,
    /// Suite name used in reports (e.g. `"w3c"`, `"jena"`, `"watdiv"`).
    pub suite: String,
}

impl Default for RunConfig {
    fn default() -> Self {
        RunConfig {
            threads: 4,
            timeout_secs: 10,
            max_tests: None,
            known_failures: HashSet::new(),
            suite: "unknown".into(),
        }
    }
}

/// The outcome of running a single test.
#[derive(Debug, Clone)]
pub enum TestOutcome {
    /// Test passed.
    Pass,
    /// Test failed with a diagnostic message.
    Fail(String),
    /// Test was skipped (data missing, unsupported type, etc.).
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
    /// Unique key for this test (IRI or template ID).
    pub key: String,
    /// Human-readable name.
    pub name: String,
    /// The test outcome.
    pub outcome: TestOutcome,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

/// Aggregated report for a complete test run.
#[derive(Debug, Default)]
pub struct RunReport {
    pub suite: String,
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
    pub fn new(suite: impl Into<String>) -> Self {
        RunReport {
            suite: suite.into(),
            ..Default::default()
        }
    }

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

    /// Returns `true` when there are no unexpected failures.
    pub fn is_clean(&self) -> bool {
        self.failed == 0 && self.timeout == 0 && self.xpass == 0
    }

    /// Human-readable summary line.
    pub fn summary(&self) -> String {
        format!(
            "[{}] {} passed, {} failed, {} skipped, {} timeout, {} xfail, {} xpass / {} total",
            self.suite,
            self.passed,
            self.failed,
            self.skipped,
            self.timeout,
            self.xfail,
            self.xpass,
            self.total,
        )
    }

    /// Serialise to a JSON value for the unified conformance report.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "suite":   self.suite,
            "total":   self.total,
            "passed":  self.passed,
            "failed":  self.failed,
            "skipped": self.skipped,
            "timeout": self.timeout,
            "xfail":   self.xfail,
            "xpass":   self.xpass,
        })
    }
}

// ── Test entries ──────────────────────────────────────────────────────────────

/// A single test entry ready to run.
pub struct TestEntry {
    /// Unique key (IRI, template ID, etc.).
    pub key: String,
    /// Human-readable name.
    pub name: String,
    /// The test body.  Returns `Ok(())` on pass, `Err(msg)` on failure.
    pub run: Box<dyn FnOnce() -> Result<(), String> + Send + 'static>,
}

// ── Runner ────────────────────────────────────────────────────────────────────

/// Run a list of [`TestEntry`] items using the given config.
///
/// Tests are distributed across `config.threads` worker threads via a channel.
/// Each worker calls its test function directly (no DB connection management
/// at this level — that is the responsibility of the test closure).
pub fn run_entries(entries: Vec<TestEntry>, config: &RunConfig) -> RunReport {
    let known = Arc::new(config.known_failures.clone());
    let timeout = std::time::Duration::from_secs(config.timeout_secs);

    // Cap entries if requested.
    let entries: Vec<_> = if let Some(max) = config.max_tests {
        entries.into_iter().take(max).collect()
    } else {
        entries
    };

    let n = entries.len();
    let (tx, rx) = bounded::<TestEntry>(n.max(1));
    for entry in entries {
        tx.send(entry).ok();
    }
    drop(tx); // Close sender so workers drain cleanly.

    let rx = Arc::new(std::sync::Mutex::new(rx));
    let (res_tx, res_rx) = bounded::<TestResult>(n.max(1));

    let threads = config.threads.min(n.max(1));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let rx = Arc::clone(&rx);
            let res_tx = res_tx.clone();
            let known = Arc::clone(&known);

            std::thread::spawn(move || {
                loop {
                    let entry = {
                        let rx = rx.lock().unwrap();
                        match rx.try_recv() {
                            Ok(e) => e,
                            Err(_) => break,
                        }
                    };
                    let key = entry.key.clone();
                    let name = entry.name.clone();
                    let is_known = known.contains(&key);

                    let t0 = Instant::now();
                    // Run with timeout via a scoped thread.
                    let result = run_with_timeout(entry.run, timeout);
                    let duration_ms = t0.elapsed().as_millis() as u64;

                    let outcome = match result {
                        RunResult::Ok => {
                            if is_known {
                                TestOutcome::XPass
                            } else {
                                TestOutcome::Pass
                            }
                        }
                        RunResult::Err(msg) => {
                            if is_known {
                                TestOutcome::XFail(msg)
                            } else {
                                TestOutcome::Fail(msg)
                            }
                        }
                        RunResult::TimedOut => TestOutcome::Timeout,
                        RunResult::Skipped(reason) => TestOutcome::Skip(reason),
                    };

                    res_tx
                        .send(TestResult {
                            key,
                            name,
                            outcome,
                            duration_ms,
                        })
                        .ok();
                }
            })
        })
        .collect();

    drop(res_tx);
    for h in handles {
        h.join().ok();
    }

    let mut report = RunReport::new(&config.suite);
    while let Ok(r) = res_rx.recv() {
        report.add(r);
    }
    report
}

// ── Internal helpers ──────────────────────────────────────────────────────────

enum RunResult {
    Ok,
    Err(String),
    TimedOut,
    Skipped(String),
}

fn run_with_timeout(
    f: Box<dyn FnOnce() -> Result<(), String> + Send + 'static>,
    timeout: std::time::Duration,
) -> RunResult {
    let (done_tx, done_rx) = bounded::<Result<(), String>>(1);
    std::thread::spawn(move || {
        done_tx.send(f()).ok();
    });
    match done_rx.recv_timeout(timeout) {
        Ok(Ok(())) => RunResult::Ok,
        Ok(Err(msg)) => {
            // Distinguish skip messages by a sentinel prefix.
            if msg.starts_with("SKIP:") {
                RunResult::Skipped(msg[5..].trim().to_string())
            } else {
                RunResult::Err(msg)
            }
        }
        Err(crossbeam_channel::RecvTimeoutError::Timeout) => RunResult::TimedOut,
        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
            RunResult::Err("worker thread panicked".into())
        }
    }
}
