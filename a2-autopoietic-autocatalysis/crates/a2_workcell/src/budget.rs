//! Budget enforcement for workcells.
//!
//! Each workcell has a token budget, call budget, and wall-clock budget.
//! The BudgetTracker monitors usage and signals when limits are approached or exceeded.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use a2_core::protocol::Budget;

/// Thread-safe budget tracker for a single workcell execution.
#[derive(Debug)]
pub struct BudgetTracker {
    limits: Budget,
    tokens_used: AtomicU64,
    calls_used: AtomicU32,
    start: Instant,
    exceeded: AtomicBool,
}

impl BudgetTracker {
    pub fn new(limits: Budget) -> Arc<Self> {
        Arc::new(Self {
            limits,
            tokens_used: AtomicU64::new(0),
            calls_used: AtomicU32::new(0),
            start: Instant::now(),
            exceeded: AtomicBool::new(false),
        })
    }

    /// Record token usage from a model call. Returns Err if budget exceeded.
    pub fn record_usage(&self, tokens_in: u64, tokens_out: u64) -> Result<(), BudgetExceeded> {
        let total = tokens_in + tokens_out;
        let new_total = self.tokens_used.fetch_add(total, Ordering::Relaxed) + total;
        self.calls_used.fetch_add(1, Ordering::Relaxed);

        if new_total > self.limits.max_tokens {
            self.exceeded.store(true, Ordering::Relaxed);
            return Err(BudgetExceeded::Tokens {
                used: new_total,
                limit: self.limits.max_tokens,
            });
        }

        let calls = self.calls_used.load(Ordering::Relaxed);
        if calls > self.limits.max_calls {
            self.exceeded.store(true, Ordering::Relaxed);
            return Err(BudgetExceeded::Calls {
                used: calls,
                limit: self.limits.max_calls,
            });
        }

        if self.elapsed_secs() > self.limits.max_duration_secs as f64 {
            self.exceeded.store(true, Ordering::Relaxed);
            return Err(BudgetExceeded::Duration {
                elapsed: self.elapsed_secs(),
                limit: self.limits.max_duration_secs as f64,
            });
        }

        Ok(())
    }

    pub fn is_exceeded(&self) -> bool {
        self.exceeded.load(Ordering::Relaxed)
            || self.elapsed_secs() > self.limits.max_duration_secs as f64
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    pub fn tokens_used(&self) -> u64 {
        self.tokens_used.load(Ordering::Relaxed)
    }

    pub fn calls_used(&self) -> u32 {
        self.calls_used.load(Ordering::Relaxed)
    }

    pub fn tokens_remaining(&self) -> u64 {
        self.limits
            .max_tokens
            .saturating_sub(self.tokens_used.load(Ordering::Relaxed))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BudgetExceeded {
    #[error("token budget exceeded: used {used}, limit {limit}")]
    Tokens { used: u64, limit: u64 },
    #[error("call budget exceeded: used {used}, limit {limit}")]
    Calls { used: u32, limit: u32 },
    #[error("duration budget exceeded: elapsed {elapsed:.1}s, limit {limit:.1}s")]
    Duration { elapsed: f64, limit: f64 },
}
