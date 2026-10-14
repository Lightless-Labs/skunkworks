//! Budgeted, logging provider: every arm invokes the model through this.
//!
//! The controller opens a scope (arm, task, replicate, budget) before each
//! arm run. Invocations beyond the scope budget, the global cap, or the
//! wall-clock deadline are refused with `ProviderError::InvocationFailed`
//! and recorded. Every invocation's prompt and response are saved under the
//! run directory and hashed into the ledger.

use crate::ledger::{Ledger, sha256_hex};
use a2d_core::provider::{InvocationRequest, InvocationResponse, Provider, ProviderError};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct Scope {
    arm: String,
    task: String,
    replicate: u32,
    budget: usize,
    used: usize,
}

pub struct Meter {
    pub ledger: Ledger,
    scope: Option<Scope>,
    global_used: usize,
    global_cap: usize,
    deadline: Instant,
    artifacts_dir: PathBuf,
}

pub type SharedMeter = Arc<Mutex<Meter>>;

pub fn lock(meter: &SharedMeter) -> MutexGuard<'_, Meter> {
    meter
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Ticket {
    scope: Scope,
    index: usize,
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
            scope: None,
            global_used: 0,
            global_cap,
            deadline: Instant::now() + wall_clock,
            artifacts_dir,
        }))
    }

    pub fn begin(&mut self, arm: &str, task: &str, replicate: u32, budget: usize) {
        self.scope = Some(Scope {
            arm: arm.to_string(),
            task: task.to_string(),
            replicate,
            budget,
            used: 0,
        });
    }

    /// Close the scope; returns invocations used in it.
    pub fn end(&mut self) -> usize {
        self.scope.take().map(|s| s.used).unwrap_or(0)
    }

    pub fn global_used(&self) -> usize {
        self.global_used
    }

    pub fn global_cap(&self) -> usize {
        self.global_cap
    }

    pub fn past_deadline(&self) -> bool {
        Instant::now() >= self.deadline
    }

    fn reserve(&mut self, request: &InvocationRequest) -> Result<Ticket, ProviderError> {
        let refusal = match &self.scope {
            None => Some("no open scope"),
            Some(scope) if scope.used >= scope.budget => Some("arm budget exhausted"),
            Some(_) if self.global_used >= self.global_cap => Some("global invocation cap reached"),
            Some(_) if self.past_deadline() => Some("wall-clock cap reached"),
            Some(_) => None,
        };
        if let Some(reason) = refusal {
            let scope = self.scope.clone();
            let _ = self.ledger.append(
                "invocation_refused",
                json!({
                    "arm": scope.as_ref().map(|s| s.arm.clone()),
                    "task": scope.as_ref().map(|s| s.task.clone()),
                    "replicate": scope.as_ref().map(|s| s.replicate),
                    "enzyme": request.enzyme_id.0,
                    "reason": reason,
                }),
            );
            return Err(ProviderError::InvocationFailed(format!(
                "lift controller refused invocation: {reason}"
            )));
        }
        let scope = self.scope.as_mut().expect("scope checked above");
        let index = scope.used;
        scope.used += 1;
        self.global_used += 1;
        Ok(Ticket {
            scope: scope.clone(),
            index,
        })
    }

    fn record(
        &mut self,
        ticket: &Ticket,
        provider: &str,
        request: &InvocationRequest,
        result: &Result<InvocationResponse, ProviderError>,
        elapsed: Duration,
    ) {
        let dir = self
            .artifacts_dir
            .join(&ticket.scope.arm)
            .join(&ticket.scope.task)
            .join(format!("r{}", ticket.scope.replicate));
        let stem = format!("inv{:02}-{}", ticket.index, request.enzyme_id.0);
        let prompt = format!("SYSTEM:\n{}\n\nPROMPT:\n{}", request.system, request.prompt);
        let response = match result {
            Ok(response) => response.text.clone(),
            Err(error) => format!("ERROR: {error}"),
        };
        let _ = fs::create_dir_all(&dir);
        let prompt_path = dir.join(format!("{stem}.prompt.txt"));
        let response_path = dir.join(format!("{stem}.response.txt"));
        let _ = fs::write(&prompt_path, &prompt);
        let _ = fs::write(&response_path, &response);
        let usage = result.as_ref().ok().map(|r| {
            json!({
                "prompt_tokens": r.usage.prompt_tokens,
                "completion_tokens": r.usage.completion_tokens,
                "thinking_tokens": r.usage.thinking_tokens,
            })
        });
        let _ = self.ledger.append(
            "invocation",
            json!({
                "arm": ticket.scope.arm,
                "task": ticket.scope.task,
                "replicate": ticket.scope.replicate,
                "index": ticket.index,
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
            }),
        );
    }
}

pub struct MeteredProvider {
    inner: Arc<dyn Provider>,
    meter: SharedMeter,
}

impl MeteredProvider {
    pub fn new(inner: Arc<dyn Provider>, meter: SharedMeter) -> Self {
        Self { inner, meter }
    }
}

impl Provider for MeteredProvider {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn invoke(&self, request: &InvocationRequest) -> Result<InvocationResponse, ProviderError> {
        let ticket = lock(&self.meter).reserve(request)?;
        let start = Instant::now();
        let result = self.inner.invoke(request);
        let elapsed = start.elapsed();
        lock(&self.meter).record(&ticket, self.inner.name(), request, &result, elapsed);
        result
    }
}
