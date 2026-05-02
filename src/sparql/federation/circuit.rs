//! Federation circuit breaker and connection pool (G-3, v0.56.0).
//!
//! Split from `federation.rs` in v0.85.0 (Q13-03).

use super::*;

/// State of a per-endpoint circuit breaker.
#[derive(Debug, Clone)]
enum CircuitState {
    /// Circuit is closed: requests flow normally.
    Closed,
    /// Circuit is open: requests are rejected immediately (PT605).
    Open { opened_at: Instant },
    /// Circuit is half-open: one probe request is allowed through.
    HalfOpen,
}

/// Per-endpoint circuit breaker tracking consecutive failures and state.
#[derive(Debug, Clone)]
struct CircuitBreaker {
    state: CircuitState,
    consecutive_failures: u32,
}

impl CircuitBreaker {
    fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
        }
    }

    /// Record a successful call: reset failures and close circuit.
    fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.state = CircuitState::Closed;
    }

    /// Record a failed call. Opens the circuit when the threshold is hit.
    fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        let threshold = crate::gucs::federation::FEDERATION_CIRCUIT_BREAKER_THRESHOLD.get() as u32;
        if threshold > 0 && self.consecutive_failures >= threshold {
            self.state = CircuitState::Open {
                opened_at: Instant::now(),
            };
        }
    }

    /// Returns `true` when the circuit is open and the call should be blocked.
    fn is_open(&mut self) -> bool {
        let reset_secs =
            crate::gucs::federation::FEDERATION_CIRCUIT_BREAKER_RESET_SECONDS.get() as u64;
        if let CircuitState::Open { opened_at } = self.state {
            if opened_at.elapsed().as_secs() >= reset_secs {
                // Transition to half-open to allow one probe.
                self.state = CircuitState::HalfOpen;
                return false;
            }
            return true;
        }
        false
    }
}

thread_local! {
    /// Per-backend circuit breaker map keyed by endpoint URL.
    static CIRCUIT_BREAKERS: RefCell<HashMap<String, CircuitBreaker>> =
        RefCell::new(HashMap::new());
}

/// Check whether the circuit breaker for `url` is open.
/// Returns `true` when the call should be blocked (PT605).
pub(super) fn circuit_is_open(url: &str) -> bool {
    let threshold = crate::gucs::federation::FEDERATION_CIRCUIT_BREAKER_THRESHOLD.get();
    if threshold <= 0 {
        return false; // Disabled.
    }
    CIRCUIT_BREAKERS.with(|cb| {
        let mut map = cb.borrow_mut();
        let breaker = map
            .entry(url.to_owned())
            .or_insert_with(CircuitBreaker::new);
        breaker.is_open()
    })
}

pub(super) fn circuit_record_success(url: &str) {
    CIRCUIT_BREAKERS.with(|cb| {
        let mut map = cb.borrow_mut();
        if let Some(breaker) = map.get_mut(url) {
            breaker.record_success();
        }
    });
}

pub(super) fn circuit_record_failure(url: &str) {
    CIRCUIT_BREAKERS.with(|cb| {
        let mut map = cb.borrow_mut();
        let breaker = map
            .entry(url.to_owned())
            .or_insert_with(CircuitBreaker::new);
        breaker.record_failure();
    });
}

// ─── Thread-local connection pool (v0.19.0) ──────────────────────────────────

thread_local! {
    /// Shared HTTP agent for the current PostgreSQL backend.
    /// Created lazily on first use; reuses TCP/TLS connections across calls.
    static SHARED_AGENT: RefCell<Option<ureq::Agent>> = const { RefCell::new(None) };
}

/// Strip the platform-specific "(os error NNN)" suffix from ureq error strings.
///
/// macOS uses ECONNREFUSED = 61, Linux uses 111.  Normalising the message makes
/// pg_regress expected outputs portable across operating systems.
pub(super) fn normalize_http_err(e: impl std::fmt::Display) -> String {
    let s = format!("{e}");
    // Locate the last "(os error " pattern and strip the parenthesised suffix.
    if let Some(start) = s.rfind(" (os error ") {
        let end = s[start..]
            .find(')')
            .map(|i| start + i + 1)
            .unwrap_or(s.len());
        let mut out = s[..start].to_string();
        if end < s.len() {
            out.push_str(&s[end..]);
        }
        out
    } else {
        s
    }
}

/// Return the per-thread shared ureq agent, creating it on first call.
///
/// If the `pool_size` has changed since the agent was created the agent is
/// recreated (this is rare — pool_size is a session GUC).
pub(super) fn get_agent(timeout: Duration, pool_size: usize) -> ureq::Agent {
    SHARED_AGENT.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            *opt = Some(
                ureq::AgentBuilder::new()
                    .timeout(timeout)
                    .max_idle_connections_per_host(pool_size)
                    .build(),
            );
        }
        // opt is Some(…) because we just set it above when it was None.
        // Q13-07 (v0.86.0): use pgrx::error! instead of unreachable! so a
        // regression in the invariant produces a catchable PostgreSQL error
        // rather than a backend panic.
        opt.as_ref()
            .unwrap_or_else(|| {
                pgrx::error!(
                    "internal: get_agent: agent should be Some after init -- please report"
                )
            })
            .clone()
    })
}

/// Public wrapper around `get_agent` for use by `federation_planner` (v0.42.0).
pub(crate) fn get_agent_pub(timeout: Duration, pool_size: usize) -> ureq::Agent {
    get_agent(timeout, pool_size)
}

// ─── Endpoint policy check (v0.55.0) ─────────────────────────────────────────
