//! Budgeted, logging provider: every arm invokes the model through this.
//!
//! Each arm run gets its own [`MeteredProvider`] bound to a [`Scope`] (arm,
//! task, replicate, budget). Before the run starts, the controller commits
//! the scope's budget against the global cap ([`Meter::commit`]), so
//! concurrent runs can never exceed it. Invocations beyond the scope budget
//! or past the wall-clock deadline are refused with
//! `ProviderError::InvocationFailed` and recorded. Every invocation's prompt
//! and response are saved under the run directory and hashed into the ledger.

use crate::ledger::{Ledger, sha256_hex};
use a2d_core::provider::{InvocationRequest, InvocationResponse, Provider, ProviderError};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const REFUSAL_PREFIX: &str = "lift controller refused invocation";

/// One arm run's identity and budget.
#[derive(Debug)]
pub struct Scope {
    pub arm: String,
    pub task: String,
    pub replicate: u32,
    pub budget: usize,
    used: AtomicUsize,
    tokens: AtomicUsize,
    provider_ms: AtomicU64,
    coder_calls: AtomicUsize,
    tool_calls: AtomicUsize,
    /// Set when an invocation was refused for a reason other than the arm's
    /// own budget (wall clock, audit failure): the tuple is void.
    void_reason: Mutex<Option<String>>,
}

impl Scope {
    pub fn new(arm: &str, task: &str, replicate: u32, budget: usize) -> Arc<Self> {
        Arc::new(Self {
            arm: arm.to_string(),
            task: task.to_string(),
            replicate,
            budget,
            used: AtomicUsize::new(0),
            tokens: AtomicUsize::new(0),
            provider_ms: AtomicU64::new(0),
            coder_calls: AtomicUsize::new(0),
            tool_calls: AtomicUsize::new(0),
            void_reason: Mutex::new(None),
        })
    }

    /// Provider-reported tokens across this scope's invocations.
    pub fn tokens(&self) -> usize {
        self.tokens.load(Ordering::SeqCst)
    }

    /// Wall-clock milliseconds spent inside provider calls.
    pub fn provider_ms(&self) -> u64 {
        self.provider_ms.load(Ordering::SeqCst)
    }

    /// Invocations made by coder enzymes.
    pub fn coder_calls(&self) -> usize {
        self.coder_calls.load(Ordering::SeqCst)
    }

    /// Tool calls the coder lane reported (must stay 0 on the no-tools lane).
    pub fn tool_calls(&self) -> usize {
        self.tool_calls.load(Ordering::SeqCst)
    }

    pub fn void_reason(&self) -> Option<String> {
        self.void_reason
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn mark_void(&self, reason: &str) {
        self.void_reason
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get_or_insert_with(|| reason.to_string());
    }

    /// Invocations spent so far (refusals excluded).
    pub fn used(&self) -> usize {
        self.used.load(Ordering::SeqCst)
    }

    fn take_slot(&self) -> Option<usize> {
        self.used
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
                (used < self.budget).then_some(used + 1)
            })
            .ok()
    }
}

pub struct Meter {
    pub ledger: Ledger,
    /// Budgets of in-flight scopes plus invocations of finished ones.
    committed: usize,
    global_cap: usize,
    audit_failed: bool,
    deadline: Instant,
    artifacts_dir: PathBuf,
}

pub type SharedMeter = Arc<Mutex<Meter>>;

pub fn lock(meter: &SharedMeter) -> MutexGuard<'_, Meter> {
    meter
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Meter {
    pub fn new(
        ledger: Ledger,
        global_cap: usize,
        wall_clock: Duration,
        artifacts_dir: PathBuf,
    ) -> SharedMeter {
        Arc::new(Mutex::new(Self {
            ledger,
            committed: 0,
            global_cap,
            audit_failed: false,
            deadline: Instant::now() + wall_clock,
            artifacts_dir,
        }))
    }

    pub fn past_deadline(&self) -> bool {
        Instant::now() >= self.deadline
    }

    /// Commit `budget` invocations for a new scope, or say why not.
    pub fn commit(&mut self, budget: usize) -> Result<(), &'static str> {
        if self.audit_failed {
            return Err("audit trail write failed");
        }
        if self.past_deadline() {
            return Err("wall-clock cap reached");
        }
        if self.committed + budget > self.global_cap {
            return Err("global invocation cap would be exceeded");
        }
        self.committed += budget;
        Ok(())
    }

    /// Return a finished scope's unused budget.
    pub fn settle(&mut self, scope: &Scope) {
        self.committed -= scope.budget - scope.used().min(scope.budget);
    }

    fn refuse(
        &mut self,
        scope: &Scope,
        request: &InvocationRequest,
        reason: &str,
    ) -> ProviderError {
        let _ = self.ledger.append(
            "invocation_refused",
            json!({
                "arm": scope.arm,
                "task": scope.task,
                "replicate": scope.replicate,
                "enzyme": request.enzyme_id.0,
                "reason": reason,
            }),
        );
        ProviderError::InvocationFailed(format!("{REFUSAL_PREFIX}: {reason}"))
    }

    fn record(
        &mut self,
        scope: &Scope,
        index: usize,
        provider: &str,
        request: &InvocationRequest,
        result: &Result<InvocationResponse, ProviderError>,
        elapsed: Duration,
    ) {
        let dir = self
            .artifacts_dir
            .join(&scope.arm)
            .join(&scope.task)
            .join(format!("r{}", scope.replicate));
        let stem = format!("inv{:02}-{}", index, file_safe(&request.enzyme_id.0));
        let prompt = format!("SYSTEM:\n{}\n\nPROMPT:\n{}", request.system, request.prompt);
        let response = match result {
            Ok(response) => response.text.clone(),
            Err(error) => format!("ERROR: {error}"),
        };
        let prompt_path = dir.join(format!("{stem}.prompt.txt"));
        let response_path = dir.join(format!("{stem}.response.txt"));
        let saved = fs::create_dir_all(&dir)
            .and_then(|_| fs::write(&prompt_path, &prompt))
            .and_then(|_| fs::write(&response_path, &response));
        let usage = result.as_ref().ok().map(|r| {
            json!({
                "prompt_tokens": r.usage.prompt_tokens,
                "completion_tokens": r.usage.completion_tokens,
                "thinking_tokens": r.usage.thinking_tokens,
                "reported_total": r.usage.reported_total,
            })
        });
        // Provider transcript (Codex stderr): saved for audit; its tool-call
        // count shows whether the coder ran commands (e.g. read files).
        let transcript = result.as_ref().ok().and_then(|r| r.transcript.as_deref());
        let saved = saved.and_then(|_| match transcript {
            Some(transcript) => fs::write(dir.join(format!("{stem}.transcript.txt")), transcript),
            None => Ok(()),
        });
        let tool_calls = transcript.map(a2d_providers::cli::codex_tool_calls);
        let appended = self.ledger.append(
            "invocation",
            json!({
                "arm": scope.arm,
                "task": scope.task,
                "replicate": scope.replicate,
                "index": index,
                "enzyme": request.enzyme_id.0,
                "provider": provider,
                "ok": result.is_ok(),
                "error": result.as_ref().err().map(|e| e.to_string()),
                "duration_ms": elapsed.as_millis() as u64,
                "prompt_sha256": sha256_hex(prompt.as_bytes()),
                "response_sha256": sha256_hex(response.as_bytes()),
                "prompt_path": prompt_path.display().to_string(),
                "response_path": response_path.display().to_string(),
                "usage": usage,
                "tool_calls": tool_calls,
            }),
        );
        if saved.is_err() || appended.is_err() {
            // Fail closed: no new arm run starts without a complete audit trail.
            self.audit_failed = true;
            scope.mark_void("audit trail write failed");
        }
    }
}

/// Enzyme ids come from the (mutable) germline; never let them shape paths.
fn file_safe(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(48)
        .collect()
}

pub struct MeteredProvider {
    inner: Arc<dyn Provider>,
    meter: SharedMeter,
    scope: Arc<Scope>,
}

impl MeteredProvider {
    pub fn new(inner: Arc<dyn Provider>, meter: SharedMeter, scope: Arc<Scope>) -> Self {
        Self {
            inner,
            meter,
            scope,
        }
    }
}

impl Provider for MeteredProvider {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn invoke(&self, request: &InvocationRequest) -> Result<InvocationResponse, ProviderError> {
        if lock(&self.meter).past_deadline() {
            self.scope.mark_void("wall-clock cap reached");
            return Err(lock(&self.meter).refuse(&self.scope, request, "wall-clock cap reached"));
        }
        let Some(index) = self.scope.take_slot() else {
            return Err(lock(&self.meter).refuse(&self.scope, request, "arm budget exhausted"));
        };
        let start = Instant::now();
        let result = self.inner.invoke(request);
        let elapsed = start.elapsed();
        self.scope
            .provider_ms
            .fetch_add(elapsed.as_millis() as u64, Ordering::SeqCst);
        if request.enzyme_id.0.contains("coder") {
            self.scope.coder_calls.fetch_add(1, Ordering::SeqCst);
        }
        if let Ok(response) = &result {
            self.scope
                .tokens
                .fetch_add(response.usage.total(), Ordering::SeqCst);
            if let Some(transcript) = &response.transcript {
                self.scope.tool_calls.fetch_add(
                    a2d_providers::cli::codex_tool_calls(transcript),
                    Ordering::SeqCst,
                );
            }
        }
        lock(&self.meter).record(
            &self.scope,
            index,
            self.inner.name(),
            request,
            &result,
            elapsed,
        );
        result
    }
}
