//! Protocol objects — the typed contracts between A² components.
//!
//! These are the data structures that flow through the system.
//! Git commit is the unit of heredity. PromotionJournalEntry is the
//! unit of germline admission. Bazel target execution is the unit of phenotype.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::id::*;

// ---------------------------------------------------------------------------
// Task lifecycle
// ---------------------------------------------------------------------------

/// What needs to be done, acceptance criteria, budget.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskContract {
    pub id: TaskId,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
    pub budget: Budget,
    pub priority: Priority,
    pub source: TaskSource,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Budget {
    /// Maximum tokens (input + output) across all model calls.
    pub max_tokens: u64,
    /// Maximum wall-clock duration in seconds.
    pub max_duration_secs: u64,
    /// Maximum number of model invocations.
    pub max_calls: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Priority {
    Critical,
    High,
    Normal,
    Low,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TaskSource {
    /// Human-provided objective (food set).
    External { origin: String },
    /// Self-generated from internal analysis.
    Internal { parent_task: Option<TaskId> },
    /// From the sensorium: incident, telemetry, ticket.
    Sensorium { evidence_id: String },
}

// ---------------------------------------------------------------------------
// Context and patches
// ---------------------------------------------------------------------------

/// Relevant code, traces, prior tactics, germline snapshot reference.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextPack {
    pub germline_version: GermlineVersion,
    pub relevant_files: Vec<PathBuf>,
    pub prior_attempts: Vec<LineageId>,
    pub retrieved_motifs: Vec<String>,
}

/// Proposed changes, rationale, test results.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatchBundle {
    pub id: PatchId,
    pub task_id: TaskId,
    pub workcell_id: WorkcellId,
    pub diff: String,
    pub rationale: String,
    pub test_results: TestResults,
    pub model_attribution: ModelAttribution,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestResults {
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub details: Vec<TestDetail>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestDetail {
    pub name: String,
    pub passed: bool,
    pub output: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelAttribution {
    pub provider: String,
    pub model: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

// ---------------------------------------------------------------------------
// Fitness and evaluation
// ---------------------------------------------------------------------------

/// Somatic, germline, and organizational scores.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FitnessRecord {
    pub eval_id: EvalId,
    pub task_id: TaskId,
    pub somatic: SomaticFitness,
    pub germline: Option<GermlineFitness>,
    pub organizational: Option<OrganizationalFitness>,
    pub evaluated_at: DateTime<Utc>,
}

/// Did this workcell complete its task?
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SomaticFitness {
    pub task_completed: bool,
    pub tests_pass: bool,
    pub acceptance_met: Vec<bool>,
    pub tokens_used: u64,
    pub duration_secs: f64,
}

/// Does this mutation help future workcells?
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GermlineFitness {
    pub replay_improvement: f64,
    pub diversity_contribution: f64,
    pub regression_clear: bool,
}

/// Is the factory still healthy?
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrganizationalFitness {
    pub self_host_passes: bool,
    pub repair_coverage: f64,
    pub raf_connectivity: f64,
    pub sentinel_score: f64,
    pub mission_score: f64,
}

// ---------------------------------------------------------------------------
// Lineage and promotion
// ---------------------------------------------------------------------------

/// Provenance chain, model attributions, evaluation trace.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LineageRecord {
    pub id: LineageId,
    pub task_id: TaskId,
    pub patch_id: PatchId,
    pub parent_germline: GermlineVersion,
    pub model_attributions: Vec<ModelAttribution>,
    pub fitness: FitnessRecord,
    pub created_at: DateTime<Utc>,
}

/// Promoter's verdict on a patch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PromotionDecision {
    /// Patch is discarded.
    Discard { reason: String },
    /// Patch merged as somatic fix only (no germline change).
    MergeSomatic,
    /// Patch promoted to germline.
    PromoteGermline { mutation_scope: MutationScope },
    /// Rollback to a previous germline version.
    Rollback { target: GermlineVersion, reason: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MutationScope {
    /// Prompt template change only.
    Prompt,
    /// Policy or membrane rule change.
    Policy,
    /// Tool adapter or catalyst logic.
    Catalyst,
    /// Evaluator or sentinel change (requires extra scrutiny).
    Evaluator,
    /// Constitutional verifier change (requires attestation).
    Constitutional,
}

/// Append-only germline admission record.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromotionJournalEntry {
    pub id: PromotionId,
    pub patch_id: PatchId,
    pub germline_before: GermlineVersion,
    pub germline_after: GermlineVersion,
    pub decision: PromotionDecision,
    pub gate_results: HashMap<String, bool>,
    pub promoted_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Boundary and membrane
// ---------------------------------------------------------------------------

/// Current soft membrane: what tools, permissions, scopes are available.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapabilityMap {
    pub allowed_tools: Vec<String>,
    pub denied_tools: Vec<String>,
    pub secret_scopes: Vec<String>,
    pub network_policy: NetworkPolicy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetworkPolicy {
    /// No network access.
    Isolated,
    /// Only specific endpoints.
    AllowList(Vec<String>),
    /// Full network access.
    Open,
}

/// Root-of-trust constraints + soft membrane rules.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoundaryPolicy {
    pub hard_shell: HardShell,
    pub soft_membrane: CapabilityMap,
}

/// Externally anchored, human-maintained trust root.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HardShell {
    pub root_of_trust_hash: String,
    pub constitutional_spec_hash: String,
    pub frozen_sentinel_hash: String,
    pub max_budget: Budget,
}

// ---------------------------------------------------------------------------
// Workcell
// ---------------------------------------------------------------------------

/// Scheduler's assignment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkcellSlot {
    pub workcell_id: WorkcellId,
    pub task: TaskContract,
    pub catalyst_id: CatalystId,
    pub germline_version: GermlineVersion,
    pub budget: Budget,
    pub assigned_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Evolution
// ---------------------------------------------------------------------------

/// Strategist's directives.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvolutionPolicy {
    pub exploration_ratio: f64,
    pub diversity_floor: usize,
    pub max_concurrent_workcells: usize,
    pub mutation_temperature: f64,
}
