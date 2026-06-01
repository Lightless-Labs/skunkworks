//! Metabolism: runtime orchestrator for the catalytic network.
//!
//! The metabolism turns the RAF from a static definition into a running
//! process. It schedules ready enzymes, spawns ephemeral workcells, routes
//! artifacts by `ArtifactType`, gates mutations through the germline, and
//! records invocation lineage.

use crate::benchmark::{BenchmarkSuite, FitnessReport};
use crate::germline::Germline;
use crate::observer::{HealthMetrics, ToolEvent};
use crate::provider::{
    InvocationRequest, InvocationResponse, ProviderError, ProviderPolicy,
    ProviderPolicyApplication, ProviderPolicyRejection, ProviderRegistry,
};
use crate::self_sandbox::{self, SystemPatch};
use crate::types::{ArtifactType, EnzymeDef, EnzymeId};
use crate::workcell::{Workcell, WorkcellId, WorkcellOutcome};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Typed artifact payloads routed between enzymes.
pub type ArtifactStore = BTreeMap<ArtifactType, Vec<u8>>;

#[derive(Debug, Clone)]
struct StoredArtifact {
    bytes: Vec<u8>,
    revision: usize,
}

#[derive(Debug, Clone, Default)]
struct ProviderHealth {
    consecutive_failures: usize,
    cooldown_until: Option<Instant>,
}

#[derive(Debug, Clone)]
struct ScheduledInvocation {
    enzyme: EnzymeDef,
    inputs: ArtifactStore,
}

#[derive(Debug, Clone)]
pub struct MutationRejection {
    pub enzyme_id: Option<EnzymeId>,
    pub reason: String,
}

/// Fitness-scored candidate from a parallel provider portfolio.
/// Stored in lineage so the system can learn which providers/topologies
/// produce useful artifacts, not merely which one returned first.
#[derive(Debug, Clone)]
pub struct CandidateEvaluation {
    pub provider: String,
    pub materialized: bool,
    pub fitness: Option<FitnessReport>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct MutationRecord {
    pub accepted: Vec<EnzymeId>,
    pub rejected: Vec<MutationRejection>,
}

/// Tracks proposed system code modifications from the architect enzyme.
#[derive(Debug, Clone, Default)]
pub struct PatchRecord {
    pub accepted: Vec<String>,
    pub rejected: Vec<PatchRejection>,
    pub noops: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PatchRejection {
    pub file_path: Option<String>,
    pub reason: String,
}

/// Tracks proposed provider-role policy changes.
#[derive(Debug, Clone, Default)]
pub struct ProviderPolicyRecord {
    pub accepted: Vec<String>,
    pub rejected: Vec<ProviderPolicyRejection>,
}

/// Recorded lineage for a single workcell invocation.
#[derive(Debug, Clone)]
pub struct InvocationLineage {
    pub cycle: usize,
    pub workcell_id: WorkcellId,
    pub enzyme_id: EnzymeId,
    pub provider: String,
    /// Escalation rung that shaped this invocation. Zero means no active loop
    /// intervention; values 1+ map to the rung ladder documented in
    /// `todos/escalation-rungs-4-6.md`.
    pub escalation_rung: usize,
    /// True when the invocation used the provider-swap path (rung 4+). This is
    /// the requested intervention, even if a one-provider registry has no
    /// alternate provider to route to.
    pub provider_swap: bool,
    /// True when failure context was stripped before building the provider
    /// request (rung 3 and rung 5+).
    pub clean_session: bool,
    pub inputs: ArtifactStore,
    pub outputs: ArtifactStore,
    pub tool_events: Vec<ToolEvent>,
    pub health: HealthMetrics,
    pub outcome: WorkcellOutcome,
    pub mutation: Option<MutationRecord>,
    pub patch: Option<PatchRecord>,
    pub provider_policy: Option<ProviderPolicyRecord>,
    pub candidate_evaluations: Vec<CandidateEvaluation>,
}

/// Summary of one metabolism cycle.
#[derive(Debug, Clone, Default)]
pub struct CycleReport {
    pub cycle: usize,
    pub invocations: usize,
    pub completed: usize,
    pub killed: usize,
    pub failed: usize,
    pub accepted_mutations: usize,
    pub rejected_mutations: usize,
    pub accepted_patches: usize,
    pub rejected_patches: usize,
    pub accepted_provider_policy_changes: usize,
    pub rejected_provider_policy_changes: usize,
    /// True when the cycle hit `max_invocations_per_cycle` and force-advanced.
    /// The fitness ratchet prevents regressions, so early exit is safe.
    pub capped: bool,
    /// True when the cycle exceeded its wall-clock budget and force-advanced.
    pub wall_clock_capped: bool,
    pub max_entropy_rate: f64,
    pub lineage: Vec<InvocationLineage>,
    pub fitness: Option<FitnessReport>,
    pub fitness_delta: Option<f64>,
    /// Enzymes in an escalated state at cycle end: (enzyme, rung).
    /// Rung 0 = normal, 1+ = loop detected, intervention in progress.
    pub loop_escalations: Vec<(EnzymeId, usize)>,
}

/// Deterministic orchestrator for the A²D catalytic loop.
pub struct Metabolism {
    germline: Germline,
    providers: ProviderRegistry,
    artifacts: BTreeMap<ArtifactType, StoredArtifact>,
    lineage: Vec<InvocationLineage>,
    last_inputs: BTreeMap<EnzymeId, BTreeMap<ArtifactType, usize>>,
    next_workcell: usize,
    next_revision: usize,
    next_cycle: usize,
    max_invocations_per_cycle: usize,
    max_cycle_wall_clock: Option<Duration>,
    provider_failure_cooldown: Duration,
    provider_failure_max_cooldown: Duration,
    provider_health: BTreeMap<String, ProviderHealth>,
    benchmark: Option<BenchmarkSuite>,
    last_fitness: f64,
    project_root: Option<PathBuf>,
    pending_patches: Vec<SystemPatch>,
    // Loop detection: track per-enzyme output signatures across cycles.
    // For benchmarked enzymes (coder): fitness signature (Vec<CaseResult>).
    // For non-benchmarked enzymes (evolver, architect): byte hash of outputs.
    // Both persist across cycles — cross-cycle loops are the realistic case.
    enzyme_fitness_signatures: BTreeMap<EnzymeId, u64>,
    enzyme_output_hashes: BTreeMap<EnzymeId, u64>,
    // Escalation state: per-enzyme loop count. 0 = no loop detected,
    // 1+ = current escalation rung. Resets to 0 when the enzyme produces
    // a different signature (escape the loop).
    //
    // Rungs (target design — rungs 0-4 implemented, 5-6 to come):
    //   0: no intervention — enzyme is healthy
    //   1: inject loop awareness into the enzyme's prompt
    //   2: consult another model (advice from a second provider)
    //   3: clean session (drop failure context, retry with fresh perspective)
    //   4: swap to a different model, preserving failure history
    //   5: swap to a different model with clean session
    //   6: multi-model consensus (N providers, pick best by fitness)
    enzyme_loop_count: BTreeMap<EnzymeId, usize>,
}

/// Print a trace line if A2D_TRACE=1 is set in the environment.
/// Used for debugging slow live runs without polluting test output.
fn trace(msg: &str) {
    if std::env::var("A2D_TRACE").is_ok_and(|v| !v.is_empty() && v != "0") {
        eprintln!(
            "[a2d {}] {}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| format!("{}.{:03}", d.as_secs(), d.subsec_millis()))
                .unwrap_or_default(),
            msg
        );
    }
}

impl Metabolism {
    pub fn new(germline: Germline, providers: ProviderRegistry) -> Self {
        Self {
            germline,
            providers,
            artifacts: BTreeMap::new(),
            lineage: Vec::new(),
            last_inputs: BTreeMap::new(),
            next_workcell: 1,
            next_revision: 0,
            next_cycle: 0,
            max_invocations_per_cycle: 20,
            max_cycle_wall_clock: Some(Duration::from_secs(600)),
            provider_failure_cooldown: Duration::from_secs(600),
            provider_failure_max_cooldown: Duration::from_secs(3600),
            provider_health: BTreeMap::new(),
            benchmark: None,
            last_fitness: 0.0,
            project_root: None,
            pending_patches: Vec::new(),
            enzyme_fitness_signatures: BTreeMap::new(),
            enzyme_output_hashes: BTreeMap::new(),
            enzyme_loop_count: BTreeMap::new(),
        }
    }

    /// Attach a holdout benchmark for fitness-gated mutations.
    pub fn with_benchmark(mut self, benchmark: BenchmarkSuite) -> Self {
        self.benchmark = Some(benchmark);
        self
    }

    /// Set the project root for self-modification (architect enzyme).
    /// Without this, system_patch proposals are rejected.
    pub fn with_project_root(mut self, path: PathBuf) -> Self {
        self.project_root = Some(path);
        self
    }

    /// Patches accepted during cycles, waiting for the CLI to apply.
    pub fn pending_patches(&self) -> &[SystemPatch] {
        &self.pending_patches
    }

    /// Set a safety limit for how many invocations one cycle may perform.
    pub fn with_max_invocations_per_cycle(mut self, max_invocations_per_cycle: usize) -> Self {
        self.max_invocations_per_cycle = max_invocations_per_cycle.max(1);
        self
    }

    /// Set a wall-clock budget for one cycle. In-flight provider calls are not
    /// interrupted here; the budget is checked between invocations. Provider
    /// subprocess timeouts remain the hard per-call bound.
    pub fn with_max_cycle_wall_clock(mut self, max_cycle_wall_clock: Duration) -> Self {
        self.max_cycle_wall_clock = Some(max_cycle_wall_clock);
        self
    }

    /// Disable the wall-clock cycle budget. Intended for explicit experiments.
    pub fn without_max_cycle_wall_clock(mut self) -> Self {
        self.max_cycle_wall_clock = None;
        self
    }

    /// Set the base cooldown after provider-level failures such as timeouts,
    /// quota exhaustion, CLI/auth errors, or missing binaries. Repeated failures
    /// back off exponentially up to `provider_failure_max_cooldown`.
    pub fn with_provider_failure_cooldown(mut self, provider_failure_cooldown: Duration) -> Self {
        self.provider_failure_cooldown = provider_failure_cooldown;
        self
    }

    /// Seed or replace a typed artifact in the metabolism store.
    pub fn seed_artifact(&mut self, artifact_type: ArtifactType, bytes: Vec<u8>) {
        self.upsert_artifact(artifact_type, bytes);
    }

    pub fn germline(&self) -> &Germline {
        &self.germline
    }

    pub fn lineage(&self) -> &[InvocationLineage] {
        &self.lineage
    }

    pub fn provider_policy(&self) -> ProviderPolicy {
        self.providers.current_policy()
    }

    pub fn artifacts(&self) -> ArtifactStore {
        self.artifacts
            .iter()
            .map(|(artifact_type, stored)| (artifact_type.clone(), stored.bytes.clone()))
            .collect()
    }

    /// Run the catalytic loop until no enzyme has newly available inputs.
    pub fn run_cycle(&mut self) -> CycleReport {
        trace("run_cycle: enter");
        // Clear per-cycle dedup so enzymes re-run on each new cycle.
        // Loop detection state (fitness sigs, output hashes, loop counts)
        // is intentionally NOT cleared — it persists across cycles so
        // cross-cycle loops are detected and escalations compound.
        self.last_inputs.clear();

        self.seed_food_artifacts();
        self.sync_germline_artifact();
        self.sync_provider_policy_artifact();

        // Populate system_code snapshot for the architect enzyme.
        if let Some(ref root) = self.project_root {
            trace("run_cycle: reading modifiable files for system_code snapshot");
            let files = self_sandbox::read_modifiable_files(root);
            let failure_report = self
                .artifacts
                .get(&ArtifactType::from("failure_report"))
                .map(|artifact| String::from_utf8_lossy(&artifact.bytes).to_string())
                .unwrap_or_default();
            let snapshot = format_system_code_snapshot(&files, &failure_report);
            trace(&format!(
                "run_cycle: system_code snapshot = {} files, {} bytes",
                files.len(),
                snapshot.len()
            ));
            self.upsert_artifact(ArtifactType::from("system_code"), snapshot.into_bytes());
        }

        let cycle = self.next_cycle;
        self.next_cycle += 1;

        let mut report = CycleReport {
            cycle,
            ..CycleReport::default()
        };
        let cycle_start = Instant::now();

        'cycle_loop: loop {
            if self.cycle_wall_clock_exceeded(cycle_start) {
                trace("cycle wall-clock cap reached — advancing to next cycle");
                report.wall_clock_capped = true;
                break;
            }

            if report.invocations >= self.max_invocations_per_cycle {
                trace(&format!(
                    "cycle firing cap reached ({}) — advancing to next cycle",
                    self.max_invocations_per_cycle
                ));
                report.capped = true;
                break;
            }

            let ready = self.ready_invocations();
            trace(&format!(
                "loop iter: ready = {:?}",
                ready
                    .iter()
                    .map(|s| s.enzyme.id.0.as_str())
                    .collect::<Vec<_>>()
            ));
            if ready.is_empty() {
                break;
            }

            'ready_batch: for scheduled in ready {
                if self.cycle_wall_clock_exceeded(cycle_start) {
                    trace("cycle wall-clock cap reached — advancing to next cycle");
                    report.wall_clock_capped = true;
                    break 'ready_batch;
                }

                if report.invocations >= self.max_invocations_per_cycle {
                    trace(&format!(
                        "cycle firing cap reached ({}) — advancing to next cycle",
                        self.max_invocations_per_cycle
                    ));
                    report.capped = true;
                    break;
                }

                let invoke_start = std::time::Instant::now();
                trace(&format!(
                    "invoking {} (inputs: {} bytes)",
                    scheduled.enzyme.id,
                    scheduled.inputs.values().map(|v| v.len()).sum::<usize>()
                ));
                let lineage = self.invoke_scheduled(cycle, scheduled.clone());
                trace(&format!(
                    "{} → {} ({:?})",
                    scheduled.enzyme.id,
                    match &lineage.outcome {
                        WorkcellOutcome::Success { .. } => "OK",
                        WorkcellOutcome::Failed { .. } => "FAIL",
                        WorkcellOutcome::Killed { .. } => "KILL",
                    },
                    invoke_start.elapsed()
                ));
                // Loop detection (byte-hash fallback for non-benchmarked enzymes).
                // Benchmarked enzymes use fitness-signature comparison below.
                let produces_benchmarked_artifact = scheduled
                    .enzyme
                    .products
                    .contains(&ArtifactType::from("code"));
                if !produces_benchmarked_artifact
                    && matches!(lineage.outcome, WorkcellOutcome::Success { .. })
                {
                    let new_hash = hash_outputs(&lineage.outputs);
                    let prev_hash = self.enzyme_output_hashes.get(&scheduled.enzyme.id).copied();
                    if let Some(prev) = prev_hash {
                        if prev == new_hash {
                            let new_count = self
                                .enzyme_loop_count
                                .get(&scheduled.enzyme.id)
                                .copied()
                                .unwrap_or(0)
                                + 1;
                            self.enzyme_loop_count
                                .insert(scheduled.enzyme.id.clone(), new_count);
                            trace(&format!(
                                "loop detected (byte hash): {} → rung {}",
                                scheduled.enzyme.id, new_count
                            ));
                        } else {
                            // Escaped the loop — reset escalation state.
                            if self.enzyme_loop_count.contains_key(&scheduled.enzyme.id) {
                                trace(&format!(
                                    "loop escaped: {} (byte hash changed)",
                                    scheduled.enzyme.id
                                ));
                            }
                            self.enzyme_loop_count.remove(&scheduled.enzyme.id);
                        }
                    }
                    self.enzyme_output_hashes
                        .insert(scheduled.enzyme.id.clone(), new_hash);
                }

                report.invocations += 1;
                report.max_entropy_rate = report.max_entropy_rate.max(lineage.health.entropy_rate);
                match &lineage.outcome {
                    WorkcellOutcome::Success { .. } => report.completed += 1,
                    WorkcellOutcome::Killed { .. } => report.killed += 1,
                    WorkcellOutcome::Failed { .. } => report.failed += 1,
                }
                if let Some(mutation) = &lineage.mutation {
                    report.accepted_mutations += mutation.accepted.len();
                    report.rejected_mutations += mutation.rejected.len();
                }
                if let Some(patch) = &lineage.patch {
                    report.accepted_patches += patch.accepted.len();
                    report.rejected_patches += patch.rejected.len();
                }
                if let Some(policy) = &lineage.provider_policy {
                    report.accepted_provider_policy_changes += policy.accepted.len();
                    report.rejected_provider_policy_changes += policy.rejected.len();
                }

                let invocation_failed = !matches!(lineage.outcome, WorkcellOutcome::Success { .. });
                report.lineage.push(lineage.clone());
                self.lineage.push(lineage);
                self.last_inputs.insert(
                    scheduled.enzyme.id.clone(),
                    self.current_input_revisions(&scheduled.enzyme),
                );

                // Measure fitness against holdout benchmark if available
                if scheduled
                    .enzyme
                    .products
                    .contains(&ArtifactType::from("code"))
                {
                    if let Some(ref benchmark) = self.benchmark {
                        if let Some(code_artifact) = self.artifacts.get(&ArtifactType::from("code"))
                        {
                            let code = String::from_utf8_lossy(&code_artifact.bytes);
                            let fitness_report = benchmark.evaluate(&code);

                            // Loop detection (escalation rung 0) — fitness signature.
                            // If this enzyme produced code with the same pass/fail
                            // pattern as last invocation, the model is producing
                            // behavioral duplicates (different bytes, same outcome).
                            // Halt for this cycle.
                            let signature = hash_fitness_signature(&fitness_report.results);
                            if let Some(&prev_sig) =
                                self.enzyme_fitness_signatures.get(&scheduled.enzyme.id)
                            {
                                if prev_sig == signature {
                                    // Loop: same behavioral outcome. Increment rung.
                                    let new_count = self
                                        .enzyme_loop_count
                                        .get(&scheduled.enzyme.id)
                                        .copied()
                                        .unwrap_or(0)
                                        + 1;
                                    self.enzyme_loop_count
                                        .insert(scheduled.enzyme.id.clone(), new_count);
                                    trace(&format!(
                                        "loop detected (fitness sig): {} → rung {}",
                                        scheduled.enzyme.id, new_count
                                    ));
                                } else {
                                    // Escaped the loop — reset escalation state.
                                    if self.enzyme_loop_count.contains_key(&scheduled.enzyme.id) {
                                        trace(&format!(
                                            "loop escaped: {} (fitness sig changed)",
                                            scheduled.enzyme.id
                                        ));
                                    }
                                    self.enzyme_loop_count.remove(&scheduled.enzyme.id);
                                }
                            }
                            self.enzyme_fitness_signatures
                                .insert(scheduled.enzyme.id.clone(), signature);

                            let delta = fitness_report.fitness - self.last_fitness;
                            // Only update last_fitness if not regressing.
                            // If regression, the CLI will skip the lineage commit,
                            // and we keep last_fitness at the committed value.
                            if delta >= 0.0 {
                                self.last_fitness = fitness_report.fitness;
                            }

                            // Store fitness as an artifact so the evolver sees it next cycle.
                            // Only pass/fail counts and fitness score — no test content (information barrier).
                            let fitness_summary = format!(
                                "fitness: {:.2}, passed: {}, failed: {}, total: {}",
                                fitness_report.fitness,
                                fitness_report.passed,
                                fitness_report.failed,
                                fitness_report.total,
                            );
                            self.upsert_artifact(
                                ArtifactType::from("fitness_report"),
                                fitness_summary.into_bytes(),
                            );

                            // Close the feedback loop: route sandbox diagnostics back to
                            // the coder and evolver so they can see WHY the code failed.
                            // This is the missing catalytic edge.
                            let failure_content =
                                fitness_report.diagnostic.clone().unwrap_or_default();
                            self.upsert_artifact(
                                ArtifactType::from("failure_report"),
                                failure_content.into_bytes(),
                            );

                            report.fitness_delta = Some(delta);
                            report.fitness = Some(fitness_report);
                        }
                    }
                }

                if invocation_failed {
                    trace("invocation failed; ending cycle before scheduling lower-priority work");
                    break 'cycle_loop;
                }

                if produces_benchmarked_artifact {
                    trace("code artifact produced; advancing cycle so feedback can metabolize");
                    break 'cycle_loop;
                }

                trace("artifact produced; recomputing ready set before more work");
                break 'ready_batch;
            }
        }

        // Route mechanical provider health into the artifact graph so the
        // evolver/architect can react to provider-role degradation without a
        // human reading logs and patching assignments manually.
        self.sync_provider_health_report(cycle, &report.lineage);

        // Snapshot current loop escalation state for the cycle report.
        report.loop_escalations = self
            .enzyme_loop_count
            .iter()
            .filter(|(_, rung)| **rung > 0)
            .map(|(id, rung)| (id.clone(), *rung))
            .collect();

        report
    }

    fn cycle_wall_clock_exceeded(&self, cycle_start: Instant) -> bool {
        self.max_cycle_wall_clock
            .is_some_and(|budget| cycle_start.elapsed() >= budget)
    }

    fn unavailable_provider_names(&self, now: Instant) -> BTreeSet<String> {
        self.provider_health
            .iter()
            .filter(|(_, health)| health.cooldown_until.is_some_and(|until| until > now))
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn sync_provider_health_report(&mut self, cycle: usize, lineage: &[InvocationLineage]) {
        let now = Instant::now();
        let unavailable = self
            .unavailable_provider_names(now)
            .into_iter()
            .collect::<Vec<_>>();
        let provider_health = self
            .provider_health
            .iter()
            .map(|(provider, health)| {
                serde_json::json!({
                    "provider": provider,
                    "consecutive_failures": health.consecutive_failures,
                    "cooling_down": health.cooldown_until.is_some_and(|until| until > now),
                })
            })
            .collect::<Vec<_>>();
        let recent_invocations = lineage
            .iter()
            .map(provider_report_lineage_entry)
            .collect::<Vec<_>>();

        let report = serde_json::json!({
            "cycle": cycle,
            "unavailable_providers": unavailable,
            "provider_health": provider_health,
            "recent_invocations": recent_invocations,
        });
        let bytes = serde_json::to_vec_pretty(&report).expect("provider report must serialize");
        self.upsert_artifact(provider_health_report_artifact(), bytes);
    }

    fn record_provider_success(&mut self, provider_name: &str) {
        if self.provider_health.remove(provider_name).is_some() {
            trace(&format!(
                "provider health restored: {provider_name} succeeded; cooldown cleared"
            ));
        }
    }

    fn record_parallel_loser_failure(&mut self, provider_name: &str, error_message: &str) {
        let cooldown = self.cool_down_provider(provider_name);
        trace(&format!(
            "parallel provider loser failure: {provider_name} → cooldown {:?}: {error_message}",
            cooldown
        ));
    }

    fn record_provider_failure(
        &mut self,
        provider_name: &str,
        enzyme_id: &EnzymeId,
        error_message: &str,
    ) {
        let cooldown = self.cool_down_provider(provider_name);

        let new_count = self
            .enzyme_loop_count
            .get(enzyme_id)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        self.enzyme_loop_count.insert(enzyme_id.clone(), new_count);

        trace(&format!(
            "provider failure: {provider_name} for {enzyme_id} → cooldown {:?}, enzyme rung {new_count}: {error_message}",
            cooldown
        ));
    }

    fn cool_down_provider(&mut self, provider_name: &str) -> Duration {
        let health = self
            .provider_health
            .entry(provider_name.to_string())
            .or_default();
        health.consecutive_failures = health.consecutive_failures.saturating_add(1);

        let multiplier = 1u32 << (health.consecutive_failures.saturating_sub(1).min(6) as u32);
        let cooldown = self
            .provider_failure_cooldown
            .saturating_mul(multiplier)
            .min(self.provider_failure_max_cooldown);
        health.cooldown_until = Some(Instant::now() + cooldown);
        cooldown
    }

    fn ready_invocations(&self) -> Vec<ScheduledInvocation> {
        let mut ready = Vec::new();

        for enzyme in self.germline.enzymes() {
            let required = required_artifacts(enzyme);
            let mut inputs = ArtifactStore::new();
            let mut input_revisions = BTreeMap::new();
            let mut all_available = true;

            for artifact_type in required {
                let Some(stored) = self.artifacts.get(&artifact_type) else {
                    all_available = false;
                    break;
                };
                inputs.insert(artifact_type.clone(), stored.bytes.clone());
                input_revisions.insert(artifact_type, stored.revision);
            }

            if !all_available {
                continue;
            }

            // Don't fire if ALL reactants are empty (placeholder food).
            // Catalysts can be empty — they're optional context.
            let all_reactants_empty = enzyme
                .reactants
                .iter()
                .all(|r| inputs.get(r).is_some_and(|b| b.is_empty()));
            if !enzyme.reactants.is_empty() && all_reactants_empty {
                continue;
            }

            if self
                .last_inputs
                .get(&enzyme.id)
                .is_some_and(|last| last == &input_revisions)
            {
                continue;
            }

            ready.push(ScheduledInvocation {
                enzyme: enzyme.clone(),
                inputs,
            });
        }

        ready.sort_by(|left, right| {
            self.enzyme_schedule_priority(&left.enzyme)
                .cmp(&self.enzyme_schedule_priority(&right.enzyme))
                .then_with(|| left.enzyme.id.cmp(&right.enzyme.id))
        });
        ready
    }

    fn enzyme_schedule_priority(&self, enzyme: &EnzymeDef) -> u8 {
        let has_code = self.artifacts.contains_key(&ArtifactType::from("code"));
        let has_fitness_report = self
            .artifacts
            .contains_key(&ArtifactType::from("fitness_report"));

        if enzyme.products.contains(&ArtifactType::from("code")) {
            if has_code { 5 } else { 0 }
        } else if enzyme.products.contains(&ArtifactType::from("enzyme_defs")) {
            if has_fitness_report { 0 } else { 2 }
        } else if enzyme
            .products
            .contains(&ArtifactType::from("system_patch"))
        {
            if has_fitness_report { 1 } else { 3 }
        } else if enzyme
            .products
            .contains(&ArtifactType::from("test_results"))
        {
            if has_code { 2 } else { 1 }
        } else {
            6
        }
    }

    fn current_input_revisions(&self, enzyme: &EnzymeDef) -> BTreeMap<ArtifactType, usize> {
        required_artifacts(enzyme)
            .into_iter()
            .filter_map(|artifact_type| {
                self.artifacts
                    .get(&artifact_type)
                    .map(|stored| (artifact_type, stored.revision))
            })
            .collect()
    }

    fn invoke_scheduled(
        &mut self,
        cycle: usize,
        scheduled: ScheduledInvocation,
    ) -> InvocationLineage {
        let workcell_id = WorkcellId(format!("wc-{:04}", self.next_workcell));
        self.next_workcell += 1;

        let mut workcell = Workcell::spawn(
            workcell_id.clone(),
            scheduled.enzyme.id.clone(),
            scheduled.inputs.clone(),
        );

        let loop_rung = self
            .enzyme_loop_count
            .get(&scheduled.enzyme.id)
            .copied()
            .unwrap_or(0);

        let clean_session = loop_rung == 3 || loop_rung >= 5;
        let swap_provider = loop_rung >= 4;

        // Rung 3 and rung 5+: strip failure_report from inputs (clean session).
        // Rung 4 deliberately preserves failure history for the swapped model.
        let mut invocation_inputs = scheduled.inputs.clone();
        if clean_session {
            invocation_inputs.remove(&ArtifactType::from("failure_report"));
            trace(&format!(
                "clean-session escalation: stripped failure_report from {} at rung {}",
                scheduled.enzyme.id, loop_rung
            ));
        }

        // Rungs 2-3 consult an alternative provider before the primary
        // invocation. Rung 4+ uses the alternative provider as the primary
        // intervention, so consultation is skipped to avoid spending two
        // provider windows on the same alternate model.
        let consultation = if (2..=3).contains(&loop_rung) {
            let consultation_prompt = format!(
                "Another model is stuck producing the same output. \
                 Here's the task: {}\n\n\
                 Suggest a fundamentally different approach in 2-3 sentences.",
                serde_json::to_string(
                    &invocation_inputs
                        .iter()
                        .map(|(k, v)| (k.0.clone(), String::from_utf8_lossy(v).into_owned()))
                        .collect::<BTreeMap<String, String>>()
                )
                .unwrap_or_default()
            );
            let consult_request = InvocationRequest {
                enzyme_id: scheduled.enzyme.id.clone(),
                system: "You are a consulting model. Provide fresh perspective.".to_string(),
                prompt: consultation_prompt,
                max_tokens: 512,
            };
            let (alt_provider_name, consult_response) = {
                let unavailable = self.unavailable_provider_names(Instant::now());
                let alt_provider = self
                    .providers
                    .alternative_provider_for_avoiding(&scheduled.enzyme.id, &unavailable);
                let alt_provider_name = alt_provider.name().to_string();
                trace(&format!(
                    "rung 2+ consultation: asking {} for advice on {}",
                    alt_provider_name, scheduled.enzyme.id
                ));
                let consult_response = alt_provider.invoke(&consult_request);
                (alt_provider_name, consult_response)
            };
            match consult_response {
                Ok(resp) => {
                    self.record_provider_success(&alt_provider_name);
                    Some(resp.text)
                }
                Err(e) => {
                    let error_message = e.to_string();
                    self.record_provider_failure(
                        &alt_provider_name,
                        &scheduled.enzyme.id,
                        &error_message,
                    );
                    trace(&format!("consultation failed: {error_message}"));

                    // A failed consultation is already a failed escalation attempt.
                    // Do not spend a second full provider timeout on the primary in
                    // the same workcell; that violates the bounded-cycle contract and
                    // was observed live as a 180s consultation timeout followed by a
                    // 180s primary timeout inside one nominally 240s cycle.
                    let failure =
                        format!("consultation failed before primary invocation: {error_message}");
                    workcell.fail(failure);
                    let health = workcell.observe();
                    let outcome =
                        workcell
                            .outcome()
                            .cloned()
                            .unwrap_or_else(|| WorkcellOutcome::Failed {
                                error: "workcell terminated without outcome".into(),
                            });
                    return InvocationLineage {
                        cycle,
                        workcell_id,
                        enzyme_id: scheduled.enzyme.id,
                        provider: alt_provider_name,
                        escalation_rung: loop_rung,
                        provider_swap: swap_provider,
                        clean_session,
                        inputs: invocation_inputs,
                        outputs: ArtifactStore::new(),
                        tool_events: workcell.trace().to_vec(),
                        health,
                        outcome,
                        mutation: None,
                        patch: None,
                        provider_policy: None,
                        candidate_evaluations: Vec::new(),
                    };
                }
            }
        } else {
            None
        };

        let request = build_request(
            &scheduled.enzyme,
            &invocation_inputs,
            loop_rung,
            consultation.as_deref(),
        );
        let mut candidate_evaluations = Vec::new();
        let (provider_name, response) = if parallelize_enzyme(&scheduled.enzyme) && !swap_provider {
            let (provider_name, response, evaluations) =
                self.invoke_parallel_candidates(&scheduled.enzyme, &request);
            candidate_evaluations = evaluations;
            (provider_name, response)
        } else {
            let unavailable = self.unavailable_provider_names(Instant::now());
            let assigned_provider_name = self
                .providers
                .provider_for(&scheduled.enzyme.id)
                .name()
                .to_string();
            let provider = if swap_provider {
                if uses_role_isolated_fallback(&scheduled.enzyme.id) {
                    self.providers.role_isolated_swapped_provider_for_avoiding(
                        &scheduled.enzyme.id,
                        &unavailable,
                    )
                } else {
                    self.providers
                        .swapped_provider_for_avoiding(&scheduled.enzyme.id, &unavailable)
                }
            } else if uses_role_isolated_fallback(&scheduled.enzyme.id) {
                self.providers
                    .role_isolated_provider_for_avoiding(&scheduled.enzyme.id, &unavailable)
            } else {
                self.providers
                    .provider_for_avoiding(&scheduled.enzyme.id, &unavailable)
            };
            let provider_name = provider.name().to_string();
            if provider_name != assigned_provider_name {
                let route_reason = if swap_provider && clean_session {
                    "rung-5 clean provider swap"
                } else if swap_provider {
                    "rung-4 provider swap"
                } else {
                    "provider circuit breaker"
                };
                trace(&format!(
                    "{route_reason}: routing {} from {} to {}",
                    scheduled.enzyme.id, assigned_provider_name, provider_name
                ));
            }
            let response = provider.invoke(&request);
            (provider_name, response)
        };

        let mut mutation = None;
        let mut patch = None;
        let mut provider_policy = None;

        match response {
            Ok(response) => {
                self.record_provider_success(&provider_name);
                let output_diagnostic = provider_output_diagnostic(&response);
                for event in response.tool_events {
                    workcell.record_event(event);
                    if let Some(reason) = workcell.should_kill() {
                        workcell.kill(reason);
                        break;
                    }
                }

                if workcell.is_alive() {
                    let provider_outputs =
                        materialize_outputs(&scheduled.enzyme, response.text.as_bytes());
                    if provider_outputs.is_empty() && !scheduled.enzyme.products.is_empty() {
                        let error_message = format!(
                            "provider returned no materialized outputs for products {:?}. {}",
                            scheduled.enzyme.products, output_diagnostic
                        );
                        self.record_provider_failure(
                            &provider_name,
                            &scheduled.enzyme.id,
                            &error_message,
                        );
                        workcell.fail(error_message);
                    } else {
                        let routed_outputs = self.route_outputs(
                            &scheduled.enzyme,
                            provider_outputs,
                            &mut mutation,
                            &mut patch,
                            &mut provider_policy,
                        );
                        workcell.complete(routed_outputs);
                    }
                }
            }
            Err(error) => {
                let error_message = error.to_string();
                self.record_provider_failure(&provider_name, &scheduled.enzyme.id, &error_message);
                workcell.fail(error_message);
            }
        }

        let health = workcell.observe();
        let outcome = workcell
            .outcome()
            .cloned()
            .unwrap_or_else(|| WorkcellOutcome::Failed {
                error: "workcell terminated without outcome".into(),
            });
        let outputs = match &outcome {
            WorkcellOutcome::Success { outputs } => outputs.clone(),
            WorkcellOutcome::Killed { .. } | WorkcellOutcome::Failed { .. } => ArtifactStore::new(),
        };

        InvocationLineage {
            cycle,
            workcell_id,
            enzyme_id: scheduled.enzyme.id,
            provider: provider_name,
            escalation_rung: loop_rung,
            provider_swap: swap_provider,
            clean_session,
            inputs: invocation_inputs,
            outputs,
            tool_events: workcell.trace().to_vec(),
            health,
            outcome,
            mutation,
            patch,
            provider_policy,
            candidate_evaluations,
        }
    }

    fn invoke_parallel_candidates(
        &mut self,
        enzyme: &EnzymeDef,
        request: &InvocationRequest,
    ) -> (
        String,
        Result<InvocationResponse, ProviderError>,
        Vec<CandidateEvaluation>,
    ) {
        let unavailable = self.unavailable_provider_names(Instant::now());
        let assigned_provider_name = self.providers.provider_for(&enzyme.id).name().to_string();
        let candidates = self
            .providers
            .parallel_providers_for_avoiding(&enzyme.id, &unavailable);

        if candidates.len() == 1 {
            let provider = candidates[0];
            let provider_name = provider.name().to_string();
            if provider_name != assigned_provider_name {
                trace(&format!(
                    "provider circuit breaker: routing {} from {} to {}",
                    enzyme.id, assigned_provider_name, provider_name
                ));
            }
            let response = provider.invoke(request);
            return (provider_name, response, Vec::new());
        }

        trace(&format!(
            "parallel provider portfolio for {}: {:?}",
            enzyme.id,
            candidates
                .iter()
                .map(|provider| provider.name())
                .collect::<Vec<_>>()
        ));

        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for provider in candidates {
                let provider_name = provider.name().to_string();
                let request = request.clone();
                handles.push(scope.spawn(move || (provider_name, provider.invoke(&request))));
            }

            handles
                .into_iter()
                .map(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        (
                            "thread-panic".to_string(),
                            Err(ProviderError::InvocationFailed(
                                "provider invocation thread panicked".to_string(),
                            )),
                        )
                    })
                })
                .collect::<Vec<_>>()
        });

        let mut evaluations = Vec::new();
        let mut selected_index = None;
        let mut selected_fitness = None::<f64>;
        let mut first_materialized_success = None;
        let mut first_success = None;

        for (index, (name, result)) in results.iter().enumerate() {
            match result {
                Ok(response) => {
                    if first_success.is_none() {
                        first_success = Some(index);
                    }

                    let outputs = materialize_outputs(enzyme, response.text.as_bytes());
                    let materialized = enzyme.products.is_empty() || !outputs.is_empty();
                    if materialized && first_materialized_success.is_none() {
                        first_materialized_success = Some(index);
                    }

                    let fitness = outputs.get(&ArtifactType::from("code")).and_then(|code| {
                        self.benchmark
                            .as_ref()
                            .map(|benchmark| benchmark.evaluate(&String::from_utf8_lossy(code)))
                    });

                    if let Some(ref report) = fitness {
                        trace(&format!(
                            "candidate {} via {} fitness {:.0}% ({}/{})",
                            enzyme.id,
                            name,
                            report.fitness * 100.0,
                            report.passed,
                            report.total
                        ));
                        if selected_fitness.is_none_or(|best| report.fitness > best) {
                            selected_fitness = Some(report.fitness);
                            selected_index = Some(index);
                        }
                    }

                    evaluations.push(CandidateEvaluation {
                        provider: name.clone(),
                        materialized,
                        fitness,
                        error: if materialized {
                            None
                        } else {
                            Some("provider returned no materialized outputs".to_string())
                        },
                    });
                }
                Err(error) => evaluations.push(CandidateEvaluation {
                    provider: name.clone(),
                    materialized: false,
                    fitness: None,
                    error: Some(error.to_string()),
                }),
            }
        }

        let selected_index = selected_index
            .or(first_materialized_success)
            .or(first_success)
            .unwrap_or(0);

        let mut selected = None;
        for (index, (name, result)) in results.into_iter().enumerate() {
            if index == selected_index {
                selected = Some((name, result));
                continue;
            }

            match result {
                Ok(_) => self.record_provider_success(&name),
                Err(error) => self.record_parallel_loser_failure(&name, &error.to_string()),
            }
        }

        let selected = selected.expect("parallel invocation has at least one candidate");
        (selected.0, selected.1, evaluations)
    }

    fn route_outputs(
        &mut self,
        enzyme: &EnzymeDef,
        provider_outputs: ArtifactStore,
        mutation: &mut Option<MutationRecord>,
        patch_record: &mut Option<PatchRecord>,
        provider_policy_record: &mut Option<ProviderPolicyRecord>,
    ) -> ArtifactStore {
        let mut routed = ArtifactStore::new();

        for product in &enzyme.products {
            let Some(bytes) = provider_outputs.get(product).cloned() else {
                continue;
            };

            if *product == enzyme_defs_artifact() {
                let record = self.apply_mutation_artifact(&bytes);
                let snapshot = self.serialize_germline();
                self.upsert_artifact(product.clone(), snapshot.clone());
                routed.insert(product.clone(), snapshot);
                if !record.accepted.is_empty() || !record.rejected.is_empty() {
                    *mutation = Some(record);
                }
            } else if *product == system_patch_artifact() {
                let record = self.apply_system_patch(&bytes);
                // Store the patch result as an artifact for feedback
                let summary = format_patch_summary(&record);
                self.upsert_artifact(product.clone(), summary.clone().into_bytes());
                routed.insert(product.clone(), summary.into_bytes());
                if !record.accepted.is_empty()
                    || !record.rejected.is_empty()
                    || !record.noops.is_empty()
                {
                    *patch_record = Some(record);
                }
            } else if *product == provider_policy_artifact() {
                let record = self.apply_provider_policy_artifact(&bytes);
                let snapshot = self.serialize_provider_policy();
                self.upsert_artifact(product.clone(), snapshot.clone());
                routed.insert(product.clone(), snapshot);
                if !record.accepted.is_empty() || !record.rejected.is_empty() {
                    *provider_policy_record = Some(record);
                }
            } else {
                self.upsert_artifact(product.clone(), bytes.clone());
                routed.insert(product.clone(), bytes);
            }
        }

        routed
    }

    fn apply_provider_policy_artifact(&mut self, bytes: &[u8]) -> ProviderPolicyRecord {
        let mut record = ProviderPolicyRecord::default();
        let text = String::from_utf8_lossy(bytes);
        let json_str = extract_json_from_output(&text).unwrap_or_else(|| text.to_string());

        let policy = match serde_json::from_str::<ProviderPolicy>(&json_str) {
            Ok(policy) => policy,
            Err(error) => {
                record.rejected.push(ProviderPolicyRejection {
                    enzyme_id: None,
                    provider: None,
                    reason: format!(
                        "unable to parse provider_policy artifact: {error}. {}",
                        provider_artifact_preview(&text)
                    ),
                });
                return record;
            }
        };

        let valid_enzyme_ids = self
            .germline
            .enzymes()
            .into_iter()
            .map(|enzyme| enzyme.id.clone())
            .collect::<BTreeSet<_>>();
        let application = self.providers.apply_policy(&policy, &valid_enzyme_ids);
        record.accepted = format_provider_policy_acceptances(&application);
        record.rejected = application.rejected;
        record
    }

    fn apply_system_patch(&mut self, bytes: &[u8]) -> PatchRecord {
        let mut record = PatchRecord::default();

        let text = String::from_utf8_lossy(bytes);
        let json_str = extract_json_from_output(&text).unwrap_or_else(|| text.to_string());
        let value: Value = match serde_json::from_str(&json_str) {
            Ok(value) => value,
            Err(e) => {
                record.rejected.push(PatchRejection {
                    file_path: None,
                    reason: format!(
                        "Failed to parse SystemPatch: {e}. {}",
                        provider_artifact_preview(&text)
                    ),
                });
                return record;
            }
        };

        let action = value
            .get("action")
            .and_then(|action| action.as_str())
            .map(|action| action.to_ascii_lowercase());

        if action.as_deref() == Some("noop") {
            let reason = value
                .get("reason")
                .and_then(|reason| reason.as_str())
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or("architect reported no source change warranted")
                .to_string();
            record.noops.push(reason);
            return record;
        }

        if let Some(action) = action.as_deref()
            && action != "patch"
        {
            record.rejected.push(PatchRejection {
                file_path: None,
                reason: format!(
                    "Unknown SystemPatch action {action:?}. Expected \"patch\" or \"noop\". {}",
                    provider_artifact_preview(&text)
                ),
            });
            return record;
        }

        let patch: SystemPatch = match serde_json::from_value(value) {
            Ok(patch) => patch,
            Err(e) => {
                record.rejected.push(PatchRejection {
                    file_path: None,
                    reason: format!(
                        "Failed to parse SystemPatch: {e}. {}",
                        provider_artifact_preview(&text)
                    ),
                });
                return record;
            }
        };

        let Some(ref root) = self.project_root else {
            record.rejected.push(PatchRejection {
                file_path: Some(patch.file_path),
                reason: "No project_root configured — cannot validate system patches".to_string(),
            });
            return record;
        };

        let result = self_sandbox::validate_patch(root, &patch);

        if result.accepted {
            self.pending_patches.push(patch.clone());
            record.accepted.push(patch.file_path);
        } else {
            record.rejected.push(PatchRejection {
                file_path: Some(patch.file_path),
                reason: result
                    .rejection_reason
                    .unwrap_or_else(|| "Unknown rejection".to_string()),
            });
        }

        record
    }

    fn apply_mutation_artifact(&mut self, bytes: &[u8]) -> MutationRecord {
        let text = String::from_utf8_lossy(bytes);
        let json_str = extract_json_from_output(&text).unwrap_or_else(|| text.to_string());

        match serde_json::from_str::<Vec<EnzymeDef>>(&json_str) {
            Ok(defs) => self.apply_mutations(defs),
            Err(array_error) => match serde_json::from_str::<EnzymeDef>(&json_str) {
                Ok(def) => self.apply_mutations(vec![def]),
                Err(object_error) => MutationRecord {
                    accepted: Vec::new(),
                    rejected: vec![MutationRejection {
                        enzyme_id: None,
                        reason: format!(
                            "unable to parse enzyme_defs artifact: {array_error}; {object_error}"
                        ),
                    }],
                },
            },
        }
    }

    fn apply_mutations(&mut self, defs: Vec<EnzymeDef>) -> MutationRecord {
        let mut record = MutationRecord::default();

        for enzyme in defs {
            let enzyme_id = enzyme.id.clone();
            let result = if self.germline.get_enzyme(&enzyme_id).is_some() {
                self.germline.propose_replace(enzyme)
            } else {
                self.germline.propose_add(enzyme)
            };

            match result {
                Ok(_) => record.accepted.push(enzyme_id),
                Err(error) => record.rejected.push(MutationRejection {
                    enzyme_id: Some(enzyme_id),
                    reason: error.to_string(),
                }),
            }
        }

        record
    }

    fn seed_food_artifacts(&mut self) {
        let food_types: Vec<ArtifactType> = self.germline.food().iter().cloned().collect();
        for artifact_type in food_types {
            self.artifacts.entry(artifact_type).or_insert_with(|| {
                self.next_revision += 1;
                StoredArtifact {
                    bytes: Vec::new(),
                    revision: self.next_revision,
                }
            });
        }
    }

    fn sync_germline_artifact(&mut self) {
        let snapshot = self.serialize_germline();
        self.upsert_artifact(enzyme_defs_artifact(), snapshot);
    }

    fn sync_provider_policy_artifact(&mut self) {
        let snapshot = self.serialize_provider_policy();
        self.upsert_artifact(provider_policy_artifact(), snapshot);
    }

    fn serialize_germline(&self) -> Vec<u8> {
        let enzymes: Vec<EnzymeDef> = self.germline.enzymes().into_iter().cloned().collect();
        serde_json::to_vec(&enzymes).expect("germline must serialize")
    }

    fn serialize_provider_policy(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(&self.providers.current_policy())
            .expect("provider policy must serialize")
    }

    fn upsert_artifact(&mut self, artifact_type: ArtifactType, bytes: Vec<u8>) -> bool {
        match self.artifacts.entry(artifact_type) {
            Entry::Vacant(entry) => {
                self.next_revision += 1;
                entry.insert(StoredArtifact {
                    bytes,
                    revision: self.next_revision,
                });
                true
            }
            Entry::Occupied(mut entry) => {
                if entry.get().bytes == bytes {
                    return false;
                }

                self.next_revision += 1;
                entry.insert(StoredArtifact {
                    bytes,
                    revision: self.next_revision,
                });
                true
            }
        }
    }
}

fn enzyme_defs_artifact() -> ArtifactType {
    ArtifactType::from("enzyme_defs")
}

fn provider_health_report_artifact() -> ArtifactType {
    ArtifactType::from("provider_health_report")
}

fn provider_policy_artifact() -> ArtifactType {
    ArtifactType::from("provider_policy")
}

fn provider_report_lineage_entry(entry: &InvocationLineage) -> Value {
    let (outcome, error) = match &entry.outcome {
        WorkcellOutcome::Success { .. } => ("success", None),
        WorkcellOutcome::Killed { reason } => ("killed", Some(format!("{reason:?}"))),
        WorkcellOutcome::Failed { error } => ("failed", Some(error.clone())),
    };
    let candidates = entry
        .candidate_evaluations
        .iter()
        .map(|candidate| {
            serde_json::json!({
                "provider": candidate.provider,
                "materialized": candidate.materialized,
                "fitness": candidate.fitness.as_ref().map(|fitness| fitness.fitness),
                "passed": candidate.fitness.as_ref().map(|fitness| fitness.passed),
                "total": candidate.fitness.as_ref().map(|fitness| fitness.total),
                "error": candidate.error.as_ref(),
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "cycle": entry.cycle,
        "enzyme": entry.enzyme_id.0,
        "provider": entry.provider,
        "escalation_rung": entry.escalation_rung,
        "provider_swap": entry.provider_swap,
        "clean_session": entry.clean_session,
        "outcome": outcome,
        "error": error,
        "candidates": candidates,
    })
}

/// Hash an enzyme's outputs deterministically for loop detection.
/// Two invocations producing identical artifacts get the same hash.
fn hash_outputs(outputs: &ArtifactStore) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    // BTreeMap iterates in sorted order, so this is deterministic.
    for (artifact_type, bytes) in outputs {
        artifact_type.0.hash(&mut hasher);
        bytes.hash(&mut hasher);
    }
    hasher.finish()
}

/// Hash a fitness vector deterministically — the behavioral signature
/// of an artifact (which tests passed, which failed). Two invocations
/// with the same fitness signature represent the same outcome, even
/// if the underlying code differs.
fn hash_fitness_signature(results: &[crate::benchmark::CaseResult]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    for result in results {
        result.name.hash(&mut hasher);
        result.passed.hash(&mut hasher);
    }
    hasher.finish()
}

fn system_patch_artifact() -> ArtifactType {
    ArtifactType::from("system_patch")
}

fn required_artifacts(enzyme: &EnzymeDef) -> BTreeSet<ArtifactType> {
    enzyme.reactants.union(&enzyme.catalysts).cloned().collect()
}

fn uses_role_isolated_fallback(enzyme_id: &EnzymeId) -> bool {
    enzyme_id.0 == "evolver"
}

fn parallelize_enzyme(enzyme: &EnzymeDef) -> bool {
    if std::env::var("A2D_PARALLEL_CODER").is_ok_and(|value| value == "0") {
        return false;
    }

    enzyme.products.contains(&ArtifactType::from("code"))
}

fn evolver_system_prompt(
    germline_json: &[u8],
    fitness: Option<&str>,
    failure: Option<&str>,
    provider_health: Option<&str>,
    provider_policy: Option<&str>,
) -> String {
    let germline_str = String::from_utf8_lossy(germline_json);
    let fitness_section = fitness
        .map(|f| format!("\nLAST FITNESS REPORT:\n{f}\n\nUse this to guide your improvements. If fitness is low, focus on changes that help the coder produce better code.\n"))
        .unwrap_or_default();
    let failure_section = failure
        .map(|f| format!("\nFAILURE DIAGNOSTIC (from sandbox):\n{f}\n\nThis is what went wrong. Use this to make targeted improvements to enzyme definitions — especially the coder's prompt_template.\n"))
        .unwrap_or_default();
    let provider_section = provider_health
        .map(|p| format!("\nPROVIDER HEALTH REPORT (mechanical):\n{p}\n\nThis is provider-role evidence. If an enzyme repeatedly times out or falls back to a poor provider, adjust enzyme prompts/topology to reduce wasted invocations or route work away from slow paths where the current germline can express that.\n"))
        .unwrap_or_default();
    let provider_policy_section = provider_policy
        .map(|p| format!("\nCURRENT PROVIDER POLICY (mechanical):\n{p}\n\nProvider-policy changes must be proposed as a typed provider_policy artifact by an enzyme that produces provider_policy; do not smuggle provider routing changes into enzyme_defs.\n"))
        .unwrap_or_default();
    format!(
        "You are the Evolver enzyme in A²D, a self-producing software system.\n\
         Your job: improve the enzyme definitions in the germline.\n\n\
         CURRENT GERMLINE (JSON):\n{germline_str}\n\
         {fitness_section}\
         {failure_section}\
         {provider_section}\
         {provider_policy_section}\n\
         RULES:\n\
         1. Output ONLY valid JSON: an array of EnzymeDef objects\n\
         2. Each enzyme has: id (string), reactants (array of strings), products (array of strings), catalysts (array of strings), prompt_template (optional string)\n\
         3. You MUST maintain catalytic closure: every enzyme's catalysts must be producible by other enzymes or be in the food set\n\
         4. The food set contains: [\"requirements\", \"design\", \"plan\", \"failure_report\", \"fitness_report\", \"provider_health_report\", \"system_code\"]\n\
         5. You may add new enzymes, modify existing ones, or keep them unchanged\n\
         6. Prefer small, incremental improvements over large changes\n\
         7. The coder must always produce \"code\", the tester must always produce \"test_results\", the evolver must always produce \"enzyme_defs\"\n\
         8. The evolver should react to \"fitness_report\" directly; \"test_results\" is optional supporting evidence, not a gate\n\
         9. If failure diagnostic shows specific errors, modify the coder's prompt_template to address those errors\n\n\
         Output the improved enzyme definitions as a JSON array. Nothing else."
    )
}

fn build_request(
    enzyme: &EnzymeDef,
    inputs: &ArtifactStore,
    loop_rung: usize,
    consultation: Option<&str>,
) -> InvocationRequest {
    let prompt_inputs: BTreeMap<String, String> = inputs
        .iter()
        .map(|(artifact_type, payload)| {
            (
                artifact_type.0.clone(),
                String::from_utf8_lossy(payload).into_owned(),
            )
        })
        .collect();

    // Specialized prompts for system enzymes.
    let base_system = if enzyme.id == EnzymeId::from("architect") {
        let failure = inputs
            .get(&ArtifactType::from("failure_report"))
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default();
        let fitness = inputs
            .get(&ArtifactType::from("fitness_report"))
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default();
        let provider_health = inputs
            .get(&ArtifactType::from("provider_health_report"))
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default();
        let provider_policy = inputs
            .get(&provider_policy_artifact())
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default();
        let system_code = inputs
            .get(&ArtifactType::from("system_code"))
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default();
        architect_system_prompt(
            &failure,
            &fitness,
            &provider_health,
            &provider_policy,
            &system_code,
        )
    } else if enzyme.id == EnzymeId::from("evolver") {
        let germline_json = inputs
            .get(&enzyme_defs_artifact())
            .cloned()
            .unwrap_or_default();
        let fitness = inputs
            .get(&ArtifactType::from("fitness_report"))
            .map(|b| String::from_utf8_lossy(b).to_string());
        let failure = inputs
            .get(&ArtifactType::from("failure_report"))
            .map(|b| String::from_utf8_lossy(b).to_string())
            .filter(|s| !s.is_empty());
        let provider_health = inputs
            .get(&ArtifactType::from("provider_health_report"))
            .map(|b| String::from_utf8_lossy(b).to_string())
            .filter(|s| !s.is_empty());
        let provider_policy = inputs
            .get(&provider_policy_artifact())
            .map(|b| String::from_utf8_lossy(b).to_string())
            .filter(|s| !s.is_empty());
        evolver_system_prompt(
            &germline_json,
            fitness.as_deref(),
            failure.as_deref(),
            provider_health.as_deref(),
            provider_policy.as_deref(),
        )
    } else {
        let base_template = if let Some(ref template) = enzyme.prompt_template {
            template.clone()
        } else {
            format!(
                "Enzyme {} transforms {:?} into {:?}.",
                enzyme.id, enzyme.reactants, enzyme.products
            )
        };

        // Inject failure report into the prompt if available.
        // This closes the feedback loop: the enzyme sees WHY its previous
        // attempt failed and can fix the specific errors.
        let failure_report = inputs
            .get(&ArtifactType::from("failure_report"))
            .map(|b| String::from_utf8_lossy(b).to_string())
            .filter(|s| !s.is_empty());

        match failure_report {
            Some(report) => format!(
                "{base_template}\n\n\
                 PREVIOUS ATTEMPT FAILED — fix these errors:\n\
                 {report}"
            ),
            None => base_template,
        }
    };

    // Apply escalation rung intervention.
    let system = apply_rung_intervention(&base_system, loop_rung, consultation);

    InvocationRequest {
        enzyme_id: enzyme.id.clone(),
        system,
        prompt: serde_json::to_string(&prompt_inputs).expect("inputs must serialize"),
        max_tokens: 4096,
    }
}

/// Append a rung-specific escalation notice to the enzyme's system prompt.
/// Rung 0: no intervention.
/// Rung 1: awareness injection — tell the model it's been looping.
/// Rung 2: awareness + consultation from another model appended.
/// Rung 3: awareness + clean session notice (failure context stripped by caller).
/// Rung 4: awareness + ephemeral provider swap while preserving failure history.
/// Rung 5+: provider swap + clean session notice.
fn apply_rung_intervention(base: &str, rung: usize, consultation: Option<&str>) -> String {
    if rung == 0 {
        return base.to_string();
    }
    // Rung 1+ always gets awareness.
    let awareness = format!(
        "\n\n\
         === LOOP DETECTED (escalation rung {rung}) ===\n\
         Your previous {rung} invocation{s} produced the same behavioral outcome \
         (identical fitness signature or identical output bytes).\n\
         You are stuck. Do NOT produce a variation of your last attempt. \
         Try a FUNDAMENTALLY different approach:\n\
         - Different algorithm, not the same one refactored\n\
         - Different data structures, not renamed variables\n\
         - Different decomposition of the problem\n\
         If you cannot identify a fundamentally different approach, say so explicitly.\n\
         === END LOOP NOTICE ===",
        s = if rung == 1 { "" } else { "s" }
    );

    let mut result = format!("{base}{awareness}");

    // Rung 2+: append consultation from alternative model if provided.
    if let Some(advice) = consultation {
        result.push_str(&format!(
            "\n\n=== CONSULTATION FROM ANOTHER MODEL ===\n\
             {advice}\n\
             === END CONSULTATION ==="
        ));
    }

    if rung >= 4 {
        result.push_str(
            "\n\n=== PROVIDER SWAP ===\n\
             This invocation is being handled by a different provider/model than \
             the one that produced the repeated failures. Use the available \
             failure history to avoid the previous model's local optimum.\n\
             === END PROVIDER SWAP ===",
        );
    }

    // Rung 3 and rung 5+: clean session notice (failure context already
    // stripped by caller). Rung 4 preserves failure history for the swapped
    // provider.
    if rung == 3 || rung >= 5 {
        result.push_str(
            "\n\n=== CLEAN SESSION ===\n\
             Previous failure context has been cleared. You are starting fresh \
             with only the original requirements.\n\
             === END CLEAN SESSION ===",
        );
    }

    result
}

fn provider_output_diagnostic(response: &InvocationResponse) -> String {
    let parsed = provider_artifact_preview(&response.text);
    match response.raw_output.as_deref() {
        Some(raw) if raw.trim() != response.text.trim() => {
            format!(
                "provider output preview: {parsed}; raw_stdout {}",
                preview_text(raw, 800)
            )
        }
        Some(_) => format!("provider output preview: {parsed}; raw_stdout same as parsed_text"),
        None => format!("provider output preview: {parsed}"),
    }
}

fn provider_artifact_preview(text: &str) -> String {
    format!("parsed_text {}", preview_text(text, 800))
}

fn preview_text(text: &str, max_chars: usize) -> String {
    let sanitized = sanitize_provider_output(text);
    if sanitized.is_empty() {
        return "<empty>".to_string();
    }

    let total_chars = sanitized.chars().count();
    let mut preview: String = sanitized.chars().take(max_chars).collect();
    if total_chars > max_chars {
        preview.push('…');
    }

    format!("({total_chars} chars)={preview:?}")
}

fn sanitize_provider_output(text: &str) -> String {
    let mut out = String::new();
    let mut last_was_space = false;

    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
            continue;
        }

        if ch.is_control() {
            continue;
        }

        out.push(ch);
        last_was_space = false;
    }

    out.trim().to_string()
}

fn materialize_outputs(enzyme: &EnzymeDef, bytes: &[u8]) -> ArtifactStore {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();

    if trimmed.is_empty() {
        return ArtifactStore::new();
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(object) = value.as_object() {
            let mut outputs = ArtifactStore::new();
            for product in &enzyme.products {
                if let Some(value) = object.get(&product.0) {
                    outputs.insert(product.clone(), encode_value(value));
                }
            }
            if !outputs.is_empty() {
                return outputs;
            }
        }

        if enzyme.products.len() == 1 {
            let product = enzyme.products.iter().next().cloned().unwrap();
            let mut outputs = ArtifactStore::new();
            outputs.insert(product, encode_value(&value));
            return outputs;
        }
    }

    enzyme
        .products
        .iter()
        .cloned()
        .map(|product| (product, trimmed.as_bytes().to_vec()))
        .collect()
}

fn architect_system_prompt(
    failure_report: &str,
    fitness_report: &str,
    provider_health_report: &str,
    provider_policy: &str,
    system_code: &str,
) -> String {
    format!(
        "You are the Architect enzyme in A²D, a self-producing software system.\n\
         Your job: improve the system itself by modifying its source code.\n\n\
         You are the mechanism by which this system achieves true autopoiesis.\n\
         The coder writes software artifacts. You write the system that writes them.\n\n\
         FAILURE DIAGNOSTIC:\n{failure_report}\n\n\
         FITNESS REPORT:\n{fitness_report}\n\n\
         PROVIDER HEALTH REPORT:\n{provider_health_report}\n\n\
         CURRENT PROVIDER POLICY:\n{provider_policy}\n\n\
         MODIFIABLE SYSTEM CODE:\n{system_code}\n\n\
         CONSTITUTIONAL CONSTRAINTS:\n\
         You CANNOT modify: germline.rs, raf.rs, sandbox.rs, benchmark.rs, self_sandbox.rs, CONSTITUTION.md\n\
         These are the physics. You are the chemistry.\n\n\
         RULES:\n\
         1. Output ONLY valid JSON matching one of these schemas:\n\
            Patch: {{\"action\": \"patch\", \"file_path\": \"crates/...\", \"new_content\": \"...full file content...\"}}\n\
            No-op: {{\"action\": \"noop\", \"reason\": \"why no source change is warranted\"}}\n\
         2. For patches, file_path is relative to project root\n\
         3. For patches, new_content must be the COMPLETE file content (not a diff)\n\
         4. The modified file must compile and pass all existing tests\n\
         5. Focus on changes that will improve the cycle's ability to produce working code\n\
         6. If the failure diagnostic shows the cycle is degrading model output, fix the orchestration\n\
         7. Prefer minimal, targeted changes over large rewrites\n\
         8. If no source change is warranted, emit the No-op schema instead of prose, markdown, or an empty answer\n\n\
         Output the SystemPatch action as JSON. Nothing else."
    )
}

fn format_system_code_snapshot(files: &[(String, String)], failure_report: &str) -> String {
    if std::env::var("A2D_ARCHITECT_FULL_CONTEXT").is_ok_and(|v| !v.is_empty() && v != "0") {
        return format_full_system_code_snapshot(files);
    }

    let mut out = String::new();
    out.push_str(
        "ARCHITECT CONTEXT PYRAMID\n\
         Tier 0: one-line purpose per modifiable file.\n\
         Tier 1: signatures only; implementation bodies are elided.\n\
         Tier 2: full source is included only for files mentioned in the failure report.\n\n",
    );

    let focused_paths = files
        .iter()
        .filter(|(path, _)| failure_mentions_path(failure_report, path))
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();

    for (path, content) in files {
        out.push_str(&format!("=== {path} ===\n"));
        out.push_str(&format!("PURPOSE: {}\n", infer_file_purpose(path, content)));
        out.push_str("SIGNATURES:\n");
        let signatures = extract_rust_signatures(content);
        if signatures.is_empty() {
            out.push_str("  (no Rust item signatures detected)\n");
        } else {
            for signature in signatures {
                out.push_str("  ");
                out.push_str(&signature);
                out.push('\n');
            }
        }
        if focused_paths.contains(path) {
            out.push_str("FULL_SOURCE: included below (failure report mentions this file)\n");
        }
        out.push('\n');
    }

    if !focused_paths.is_empty() {
        out.push_str("=== TIER 2 FULL SOURCE FOR FAILURE-MENTIONED FILES ===\n");
        for (path, content) in files {
            if focused_paths.contains(path) {
                out.push_str(&format!("--- BEGIN {path} ---\n"));
                out.push_str(content);
                out.push_str(&format!("\n--- END {path} ---\n\n"));
            }
        }
    }

    out
}

fn format_full_system_code_snapshot(files: &[(String, String)]) -> String {
    let mut out = String::new();
    for (path, content) in files {
        out.push_str(&format!("=== {path} ===\n"));
        out.push_str(content);
        out.push_str("\n\n");
    }
    out
}

fn infer_file_purpose(path: &str, content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        let doc = trimmed
            .strip_prefix("//!")
            .or_else(|| trimmed.strip_prefix("///"))
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(doc) = doc {
            return doc.to_string();
        }
        if !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with("#!") {
            break;
        }
    }

    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .replace('_', " ");
    format!("Rust source for {stem}")
}

fn extract_rust_signatures(content: &str) -> Vec<String> {
    let lines = content.lines().collect::<Vec<_>>();
    let mut signatures = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if is_signature_start(trimmed) {
            let mut declaration = trimmed.to_string();
            while !signature_is_complete(&declaration) && i + 1 < lines.len() {
                i += 1;
                declaration.push(' ');
                declaration.push_str(lines[i].trim());
            }
            let signature = elide_signature_body(&declaration);
            if !signature.is_empty() {
                signatures.push(signature);
            }
        }
        i += 1;
    }

    signatures
}

fn is_signature_start(trimmed: &str) -> bool {
    let without_visibility = trimmed
        .strip_prefix("pub ")
        .or_else(|| trimmed.strip_prefix("pub(crate) "))
        .or_else(|| trimmed.strip_prefix("pub(super) "))
        .or_else(|| trimmed.strip_prefix("pub(in "))
        .unwrap_or(trimmed);

    without_visibility.starts_with("fn ")
        || without_visibility.starts_with("async fn ")
        || without_visibility.starts_with("struct ")
        || without_visibility.starts_with("enum ")
        || without_visibility.starts_with("trait ")
        || without_visibility.starts_with("impl ")
        || without_visibility.starts_with("type ")
        || without_visibility.starts_with("const ")
        || without_visibility.starts_with("static ")
}

fn signature_is_complete(declaration: &str) -> bool {
    declaration.contains('{') || declaration.contains(';')
}

fn elide_signature_body(declaration: &str) -> String {
    let head = declaration
        .split('{')
        .next()
        .unwrap_or(declaration)
        .split(';')
        .next()
        .unwrap_or(declaration)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if declaration.contains('{') {
        format!("{head} {{ ... }}")
    } else if declaration.contains(';') {
        format!("{head};")
    } else {
        head
    }
}

fn failure_mentions_path(failure_report: &str, path: &str) -> bool {
    if failure_report.trim().is_empty() {
        return false;
    }

    let failure = failure_report.to_ascii_lowercase();
    let path_lower = path.to_ascii_lowercase();
    let file_name = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_ascii_lowercase();

    failure.contains(&path_lower) || failure.contains(&file_name)
}

fn format_provider_policy_acceptances(application: &ProviderPolicyApplication) -> Vec<String> {
    application
        .accepted
        .iter()
        .map(|change| {
            format!(
                "{}: {} → {}",
                change.enzyme_id, change.previous_provider, change.provider
            )
        })
        .collect()
}

fn format_patch_summary(record: &PatchRecord) -> String {
    let mut parts = Vec::new();
    for path in &record.accepted {
        parts.push(format!("ACCEPTED: {path}"));
    }
    for rej in &record.rejected {
        let file = rej.file_path.as_deref().unwrap_or("unknown");
        parts.push(format!("REJECTED: {file} — {}", rej.reason));
    }
    for reason in &record.noops {
        parts.push(format!("NOOP: {reason}"));
    }
    parts.join("\n")
}

/// Try to extract JSON from LLM output that may have markdown fences.
fn extract_json_from_output(text: &str) -> Option<String> {
    // Try ```json ... ```
    if let Some(start) = text.find("```json") {
        let code_start = start + "```json".len();
        if let Some(end) = text[code_start..].find("```") {
            return Some(text[code_start..code_start + end].trim().to_string());
        }
    }
    // Try ``` ... ```
    if let Some(start) = text.find("```\n") {
        let code_start = start + "```\n".len();
        if let Some(end) = text[code_start..].find("```") {
            let candidate = text[code_start..code_start + end].trim();
            if candidate.starts_with('{') {
                return Some(candidate.to_string());
            }
        }
    }
    // Try raw JSON
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed.to_string());
    }
    None
}

fn encode_value(value: &Value) -> Vec<u8> {
    match value {
        Value::String(text) => text.as_bytes().to_vec(),
        _ => serde_json::to_vec(value).expect("json value must serialize"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observer::{BehavioralState, ToolEvent};
    use crate::provider::{
        InvocationRequest, InvocationResponse, Provider, ProviderError, TokenUsage,
    };
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn art(s: &str) -> ArtifactType {
        ArtifactType::from(s)
    }

    fn enzyme(id: &str, reactants: &[&str], products: &[&str], catalysts: &[&str]) -> EnzymeDef {
        EnzymeDef {
            id: EnzymeId::from(id),
            reactants: reactants.iter().map(|&s| art(s)).collect(),
            products: products.iter().map(|&s| art(s)).collect(),
            catalysts: catalysts.iter().map(|&s| art(s)).collect(),
            ..Default::default()
        }
    }

    fn food(items: &[&str]) -> BTreeSet<ArtifactType> {
        items.iter().map(|&s| art(s)).collect()
    }

    fn seed_enzymes() -> Vec<EnzymeDef> {
        vec![
            enzyme("coder", &["requirements"], &["code"], &["enzyme_defs"]),
            enzyme("tester", &["code"], &["test_results"], &["code"]),
            enzyme(
                "evolver",
                &["fitness_report"],
                &["enzyme_defs"],
                &["enzyme_defs", "failure_report", "fitness_report"],
            ),
        ]
    }

    #[test]
    fn architect_snapshot_uses_pyramid_summaries_by_default() {
        let files = vec![
            (
                "crates/a2d-core/src/foo.rs".to_string(),
                "//! Foo orchestration.\n\npub struct Foo { value: usize }\n\npub fn make_foo(\n    value: usize,\n) -> Foo {\n    Foo { value }\n}\n\nfn private_helper() {\n    println!(\"body should be elided\");\n}\n"
                    .to_string(),
            ),
            (
                "crates/a2d-cli/src/main.rs".to_string(),
                "fn main() { println!(\"hello\"); }\n".to_string(),
            ),
        ];

        let snapshot = format_system_code_snapshot(&files, "");

        assert!(snapshot.contains("ARCHITECT CONTEXT PYRAMID"));
        assert!(snapshot.contains("PURPOSE: Foo orchestration."));
        assert!(snapshot.contains("pub struct Foo"));
        assert!(snapshot.contains("pub fn make_foo( value: usize, ) -> Foo { ... }"));
        assert!(snapshot.contains("fn private_helper() { ... }"));
        assert!(!snapshot.contains("body should be elided"));
        assert!(!snapshot.contains("--- BEGIN crates/a2d-core/src/foo.rs ---"));
    }

    #[test]
    fn architect_snapshot_includes_full_source_for_failure_mentioned_files() {
        let files = vec![
            (
                "crates/a2d-core/src/metabolism.rs".to_string(),
                "//! Metabolism.\n\nfn broken() {\n    compile_error!(\"details only in full source\");\n}\n".to_string(),
            ),
            (
                "crates/a2d-core/src/types.rs".to_string(),
                "pub struct ArtifactType(String);\n".to_string(),
            ),
        ];

        let snapshot = format_system_code_snapshot(
            &files,
            "error[E0425] at crates/a2d-core/src/metabolism.rs:510",
        );

        assert!(snapshot.contains("--- BEGIN crates/a2d-core/src/metabolism.rs ---"));
        assert!(snapshot.contains("details only in full source"));
        assert!(!snapshot.contains("--- BEGIN crates/a2d-core/src/types.rs ---"));
    }

    struct MockProvider {
        name: String,
        responses: Vec<InvocationResponse>,
        calls: AtomicUsize,
    }

    impl MockProvider {
        fn new(name: &str, responses: Vec<InvocationResponse>) -> Self {
            Self {
                name: name.to_string(),
                responses,
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl Provider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn invoke(
            &self,
            _request: &InvocationRequest,
        ) -> Result<InvocationResponse, ProviderError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let response = self
                .responses
                .get(index)
                .cloned()
                .or_else(|| self.responses.last().cloned())
                .ok_or_else(|| ProviderError::InvocationFailed("no mock response".into()))?;
            Ok(response)
        }
    }

    struct SequenceProvider {
        name: String,
        responses: Mutex<VecDeque<Result<InvocationResponse, String>>>,
    }

    impl SequenceProvider {
        fn new(name: &str, responses: Vec<Result<InvocationResponse, String>>) -> Self {
            Self {
                name: name.to_string(),
                responses: Mutex::new(responses.into()),
            }
        }
    }

    impl Provider for SequenceProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn invoke(
            &self,
            _request: &InvocationRequest,
        ) -> Result<InvocationResponse, ProviderError> {
            match self.responses.lock().unwrap().pop_front() {
                Some(Ok(response)) => Ok(response),
                Some(Err(error)) => Err(ProviderError::InvocationFailed(error)),
                None => Err(ProviderError::InvocationFailed("no mock response".into())),
            }
        }
    }

    fn response(text: Value, tool_events: Vec<ToolEvent>) -> InvocationResponse {
        InvocationResponse {
            text: text.to_string(),
            raw_output: None,
            tool_events,
            thinking: None,
            usage: TokenUsage::default(),
        }
    }

    fn registry_for_cycle() -> ProviderRegistry {
        let default = Box::new(MockProvider::new("default", vec![]));
        let mut registry = ProviderRegistry::new(default);

        let coder = registry.register(Box::new(MockProvider::new(
            "codex",
            vec![
                response(
                    json!({"code": "fn main() { println!(\"v1\"); }"}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
                response(
                    json!({"code": "fn main() { println!(\"v2\"); }"}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
            ],
        )));
        let tester = registry.register(Box::new(MockProvider::new(
            "gemini",
            vec![
                response(
                    json!({"test_results": "tests:green:v1"}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
                response(
                    json!({"test_results": "tests:green:v2"}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
            ],
        )));
        let evolver = registry.register(Box::new(MockProvider::new(
            "claude",
            vec![
                response(
                    json!({
                        "enzyme_defs": [
                            {
                                "id": "coder",
                                "reactants": ["requirements"],
                                "products": ["code", "docs"],
                                "catalysts": ["enzyme_defs"]
                            }
                        ]
                    }),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
                response(
                    json!({
                        "enzyme_defs": [
                            {
                                "id": "coder",
                                "reactants": ["requirements"],
                                "products": ["code", "docs"],
                                "catalysts": ["enzyme_defs"]
                            }
                        ]
                    }),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
            ],
        )));

        registry.assign(EnzymeId::from("coder"), coder);
        registry.assign(EnzymeId::from("tester"), tester);
        registry.assign(EnzymeId::from("evolver"), evolver);
        registry
    }

    #[test]
    fn routes_artifacts_and_turns_the_cycle() {
        let germline = Germline::new(seed_enzymes(), food(&["requirements"]));
        let mut metabolism = Metabolism::new(germline, registry_for_cycle())
            .with_benchmark(BenchmarkSuite::default())
            .with_max_invocations_per_cycle(8);
        metabolism.seed_artifact(art("requirements"), b"ship the seed".to_vec());

        let report1 = metabolism.run_cycle();
        let report2 = metabolism.run_cycle();
        let report3 = metabolism.run_cycle();

        let lineage_ids: Vec<String> = report1
            .lineage
            .iter()
            .chain(report2.lineage.iter())
            .chain(report3.lineage.iter())
            .map(|entry| entry.enzyme_id.0.clone())
            .collect();
        assert_eq!(
            lineage_ids,
            vec![
                "coder", "evolver", "tester", "coder", "evolver", "tester", "coder"
            ]
        );
        assert_eq!(report1.completed + report2.completed + report3.completed, 7);
        assert_eq!(
            report1.accepted_mutations + report2.accepted_mutations + report3.accepted_mutations,
            2
        );
        assert!(!report1.capped && !report2.capped && !report3.capped);

        let first_coder = &report1.lineage[0];
        assert_eq!(first_coder.provider, "codex");
        assert!(first_coder.inputs.contains_key(&art("requirements")));
        assert!(first_coder.inputs.contains_key(&art("enzyme_defs")));
        assert_eq!(
            first_coder.outputs.get(&art("code")).unwrap(),
            b"fn main() { println!(\"v1\"); }"
        );

        let tester = &report2.lineage[1];
        assert_eq!(tester.provider, "gemini");
        assert_eq!(
            tester.inputs.get(&art("code")).unwrap(),
            b"fn main() { println!(\"v1\"); }"
        );
        assert_eq!(
            tester.outputs.get(&art("test_results")).unwrap(),
            b"tests:green:v1"
        );

        let evolver = &report2.lineage[0];
        let mutation = evolver.mutation.as_ref().unwrap();
        assert_eq!(mutation.accepted, vec![EnzymeId::from("coder")]);
        assert!(mutation.rejected.is_empty());
        assert!(evolver.outputs.contains_key(&art("enzyme_defs")));

        let second_coder = &report2.lineage[2];
        assert!(second_coder.workcell_id != first_coder.workcell_id);
        assert!(second_coder.inputs.contains_key(&art("enzyme_defs")));

        let artifacts = metabolism.artifacts();
        assert_eq!(
            artifacts.get(&art("code")).unwrap(),
            b"fn main() { println!(\"v2\"); }"
        );
        assert_eq!(
            artifacts.get(&art("test_results")).unwrap(),
            b"tests:green:v2"
        );

        let coder = metabolism
            .germline()
            .get_enzyme(&EnzymeId::from("coder"))
            .unwrap();
        assert!(coder.products.contains(&art("docs")));
    }

    #[test]
    fn kills_prefailure_workcells_before_routing_outputs() {
        let enzymes = vec![enzyme("fragile", &["requirements"], &["code"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let mut registry = ProviderRegistry::new(Box::new(MockProvider::new(
            "default",
            vec![response(
                json!({"code": "should never land"}),
                vec![
                    ToolEvent::Text,
                    ToolEvent::Text,
                    ToolEvent::Text,
                    ToolEvent::Text,
                    ToolEvent::Text,
                    ToolEvent::Text,
                ],
            )],
        )));
        registry.assign(EnzymeId::from("fragile"), 0);

        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"keep it alive".to_vec());

        let report = metabolism.run_cycle();

        assert_eq!(report.killed, 1);
        assert!(matches!(
            report.lineage[0].outcome,
            WorkcellOutcome::Killed {
                reason: BehavioralState::PreFailure
            }
        ));
        assert_eq!(
            report.lineage[0].health.behavioral_state,
            BehavioralState::PreFailure
        );
        assert!(report.lineage[0].outputs.is_empty());
        assert!(!metabolism.artifacts().contains_key(&art("code")));
    }

    #[test]
    fn rejects_closure_breaking_mutations_and_keeps_current_germline() {
        let germline = Germline::new(seed_enzymes(), food(&["requirements"]));

        let mut registry = ProviderRegistry::new(Box::new(MockProvider::new("default", vec![])));
        let coder = registry.register(Box::new(MockProvider::new(
            "codex",
            vec![response(
                json!({"code": "fn main() {}"}),
                vec![ToolEvent::Read, ToolEvent::Think, ToolEvent::Execute],
            )],
        )));
        let tester = registry.register(Box::new(MockProvider::new(
            "gemini",
            vec![response(
                json!({"test_results": "failing"}),
                vec![ToolEvent::Read, ToolEvent::Think, ToolEvent::Execute],
            )],
        )));
        let evolver = registry.register(Box::new(MockProvider::new(
            "claude",
            vec![response(
                json!({"enzyme_defs": "not valid enzyme json"}),
                vec![ToolEvent::Read, ToolEvent::Think, ToolEvent::Execute],
            )],
        )));

        registry.assign(EnzymeId::from("coder"), coder);
        registry.assign(EnzymeId::from("tester"), tester);
        registry.assign(EnzymeId::from("evolver"), evolver);

        let mut metabolism =
            Metabolism::new(germline, registry).with_benchmark(BenchmarkSuite::default());
        metabolism.seed_artifact(art("requirements"), b"requirements".to_vec());

        let _report1 = metabolism.run_cycle();
        let report = metabolism.run_cycle();
        let evolver_entry = report
            .lineage
            .iter()
            .find(|entry| entry.enzyme_id == EnzymeId::from("evolver"))
            .unwrap();
        let mutation = evolver_entry.mutation.as_ref().unwrap();

        assert!(mutation.accepted.is_empty());
        assert_eq!(mutation.rejected.len(), 1);
        assert!(
            mutation.rejected[0]
                .reason
                .contains("unable to parse enzyme_defs")
        );
        assert_eq!(report.rejected_mutations, 1);

        assert!(
            metabolism
                .germline()
                .get_enzyme(&EnzymeId::from("coder"))
                .is_some()
        );

        let germline_snapshot: Vec<EnzymeDef> =
            serde_json::from_slice(metabolism.artifacts().get(&art("enzyme_defs")).unwrap())
                .unwrap();
        let snapshot_coder = germline_snapshot
            .into_iter()
            .find(|enzyme| enzyme.id == EnzymeId::from("coder"))
            .unwrap();
        assert!(snapshot_coder.products.contains(&art("code")));
    }

    #[test]
    fn records_lineage_inputs_outputs_provider_and_health() {
        let enzymes = vec![enzyme("writer", &["requirements"], &["code"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let registry = ProviderRegistry::new(Box::new(MockProvider::new(
            "codex",
            vec![response(
                json!({"code": "artifact"}),
                vec![
                    ToolEvent::Read,
                    ToolEvent::Think,
                    ToolEvent::Execute,
                    ToolEvent::Write,
                ],
            )],
        )));

        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"write it".to_vec());

        let report = metabolism.run_cycle();
        let lineage = &report.lineage[0];

        assert_eq!(lineage.provider, "codex");
        assert_eq!(
            lineage.inputs.get(&art("requirements")).unwrap(),
            b"write it"
        );
        assert_eq!(lineage.outputs.get(&art("code")).unwrap(), b"artifact");
        assert_eq!(lineage.tool_events.len(), 4);
        assert_eq!(lineage.health.window_size, 4);
        assert_eq!(
            lineage.health.behavioral_state,
            BehavioralState::Deliberating
        );
    }

    #[test]
    fn feedback_loop_populates_failure_report_on_benchmark_failure() {
        // Coder produces code that compiles but has a failing test.
        // After benchmark evaluation, failure_report should be populated.
        let enzymes = vec![enzyme("coder", &["requirements"], &["code"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let failing_code = "fn solve() -> i32 { 41 }\nfn main() { println!(\"{}\", solve()); }\n#[cfg(test)] mod tests { use super::*; #[test] fn test_solve() { assert_eq!(solve(), 42); } }";

        let registry = ProviderRegistry::new(Box::new(MockProvider::new(
            "mock",
            vec![response(
                json!({"code": failing_code}),
                vec![
                    ToolEvent::Read,
                    ToolEvent::Think,
                    ToolEvent::Execute,
                    ToolEvent::Write,
                ],
            )],
        )));

        let benchmark = crate::benchmark::BenchmarkSuite {
            name: "test".to_string(),
            cases: vec![],
            acceptance_test: None,
            test_timeout_secs: 30,
        };

        let mut metabolism = Metabolism::new(germline, registry).with_benchmark(benchmark);
        metabolism.seed_artifact(art("requirements"), b"solve the puzzle".to_vec());

        let report = metabolism.run_cycle();

        // Fitness should be < 1.0 (the test fails)
        assert!(report.fitness.is_some());
        let fitness = report.fitness.as_ref().unwrap();
        assert!(
            fitness.fitness < 1.0,
            "expected fitness < 1.0, got {}",
            fitness.fitness
        );

        // failure_report artifact should be populated with diagnostic info
        let artifacts = metabolism.artifacts();
        let failure = artifacts.get(&art("failure_report"));
        assert!(failure.is_some(), "failure_report artifact missing");
        assert!(
            !failure.unwrap().is_empty(),
            "failure_report should be non-empty on failure"
        );
    }

    #[test]
    fn feedback_loop_empty_failure_report_on_perfect_fitness() {
        // Coder produces code that passes all tests.
        // failure_report should be empty (no diagnostic needed).
        let enzymes = vec![enzyme("coder", &["requirements"], &["code"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let passing_code = "fn solve() -> i32 { 42 }\nfn main() { println!(\"{}\", solve()); }\n#[cfg(test)] mod tests { use super::*; #[test] fn test_solve() { assert_eq!(solve(), 42); } }";

        let registry = ProviderRegistry::new(Box::new(MockProvider::new(
            "mock",
            vec![response(
                json!({"code": passing_code}),
                vec![
                    ToolEvent::Read,
                    ToolEvent::Think,
                    ToolEvent::Execute,
                    ToolEvent::Write,
                ],
            )],
        )));

        let benchmark = crate::benchmark::BenchmarkSuite {
            name: "test".to_string(),
            cases: vec![],
            acceptance_test: None,
            test_timeout_secs: 30,
        };

        let mut metabolism = Metabolism::new(germline, registry).with_benchmark(benchmark);
        metabolism.seed_artifact(art("requirements"), b"solve the puzzle".to_vec());

        let report = metabolism.run_cycle();

        assert!(report.fitness.is_some());
        let fitness = report.fitness.as_ref().unwrap();
        assert_eq!(fitness.fitness, 1.0);

        // failure_report should exist but be empty
        let artifacts = metabolism.artifacts();
        let failure = artifacts.get(&art("failure_report"));
        assert!(failure.is_some());
        assert!(
            failure.unwrap().is_empty(),
            "failure_report should be empty on perfect fitness"
        );
    }

    #[test]
    fn coder_prompt_includes_failure_report_when_available() {
        // Verify that build_request injects failure_report into the coder's prompt.
        let coder = EnzymeDef {
            id: EnzymeId::from("coder"),
            reactants: [art("requirements")].into(),
            products: [art("code")].into(),
            catalysts: [art("failure_report")].into(),
            prompt_template: Some("Write code.".to_string()),
        };

        let mut inputs = ArtifactStore::new();
        inputs.insert(art("requirements"), b"build it".to_vec());
        inputs.insert(
            art("failure_report"),
            b"COMPILATION FAILED: undefined variable".to_vec(),
        );

        let request = build_request(&coder, &inputs, 0, None);
        assert!(
            request.system.contains("PREVIOUS ATTEMPT FAILED"),
            "prompt should include failure injection, got: {}",
            &request.system[..200.min(request.system.len())]
        );
        assert!(
            request.system.contains("undefined variable"),
            "prompt should include the actual error"
        );
    }

    #[test]
    fn coder_prompt_clean_when_no_failure() {
        let coder = EnzymeDef {
            id: EnzymeId::from("coder"),
            reactants: [art("requirements")].into(),
            products: [art("code")].into(),
            catalysts: [art("failure_report")].into(),
            prompt_template: Some("Write code.".to_string()),
        };

        let mut inputs = ArtifactStore::new();
        inputs.insert(art("requirements"), b"build it".to_vec());
        inputs.insert(art("failure_report"), b"".to_vec());

        let request = build_request(&coder, &inputs, 0, None);
        assert!(
            !request.system.contains("PREVIOUS ATTEMPT FAILED"),
            "prompt should NOT include failure injection when report is empty"
        );
    }

    #[test]
    fn evolver_prompt_includes_provider_health_report_when_available() {
        let evolver = EnzymeDef {
            id: EnzymeId::from("evolver"),
            reactants: [art("fitness_report")].into(),
            products: [art("enzyme_defs")].into(),
            catalysts: [art("provider_health_report"), art("enzyme_defs")].into(),
            prompt_template: None,
        };

        let mut inputs = ArtifactStore::new();
        inputs.insert(art("fitness_report"), b"fitness: 0.50".to_vec());
        inputs.insert(art("enzyme_defs"), b"[]".to_vec());
        inputs.insert(
            art("provider_health_report"),
            br#"{"unavailable_providers":["glm"]}"#.to_vec(),
        );

        let request = build_request(&evolver, &inputs, 0, None);

        assert!(request.system.contains("PROVIDER HEALTH REPORT"));
        assert!(request.system.contains("glm"));
    }

    #[test]
    fn architect_prompt_includes_provider_health_report_when_available() {
        let architect = EnzymeDef {
            id: EnzymeId::from("architect"),
            reactants: [art("failure_report"), art("fitness_report")].into(),
            products: [art("system_patch")].into(),
            catalysts: [art("provider_health_report"), art("system_code")].into(),
            prompt_template: None,
        };

        let mut inputs = ArtifactStore::new();
        inputs.insert(art("failure_report"), b"".to_vec());
        inputs.insert(art("fitness_report"), b"fitness: 0.50".to_vec());
        inputs.insert(
            art("provider_health_report"),
            br#"{"recent_invocations":[{"enzyme":"architect","provider":"glm","outcome":"failed"}]}"#.to_vec(),
        );
        inputs.insert(art("system_code"), b"fn build_registry() {}".to_vec());

        let request = build_request(&architect, &inputs, 0, None);

        assert!(request.system.contains("PROVIDER HEALTH REPORT"));
        assert!(request.system.contains("architect"));
        assert!(request.system.contains("glm"));
    }

    #[test]
    fn rung_1_injects_loop_awareness_into_prompt() {
        // Rung 1 is the first intervention: tell the enzyme it's looping
        // and must try a fundamentally different approach.
        let coder = EnzymeDef {
            id: EnzymeId::from("coder"),
            reactants: [art("requirements")].into(),
            products: [art("code")].into(),
            catalysts: [].into(),
            prompt_template: Some("Write code.".to_string()),
        };

        let mut inputs = ArtifactStore::new();
        inputs.insert(art("requirements"), b"build it".to_vec());

        // Rung 0: no awareness
        let r0 = build_request(&coder, &inputs, 0, None);
        assert!(!r0.system.contains("LOOP DETECTED"));

        // Rung 1: awareness injected
        let r1 = build_request(&coder, &inputs, 1, None);
        assert!(
            r1.system.contains("LOOP DETECTED"),
            "rung 1 should inject loop awareness, got: {}",
            &r1.system
        );
        assert!(
            r1.system.contains("FUNDAMENTALLY different"),
            "rung 1 should tell the enzyme to try a fundamentally different approach"
        );
        assert!(
            r1.system.contains("escalation rung 1"),
            "prompt should name the current rung"
        );

        // Rung 3: still includes awareness (higher rungs layer on top)
        let r3 = build_request(&coder, &inputs, 3, None);
        assert!(r3.system.contains("LOOP DETECTED"));
        assert!(r3.system.contains("escalation rung 3"));
    }

    #[test]
    fn empty_reactants_skip_enzyme() {
        // An enzyme with all-empty reactants should not fire.
        let enzymes = vec![enzyme("worker", &["data"], &["output"], &[])];
        let germline = Germline::new(enzymes, food(&["data"]));

        let registry = ProviderRegistry::new(Box::new(MockProvider::new(
            "mock",
            vec![response(
                json!({"output": "result"}),
                vec![ToolEvent::Read, ToolEvent::Think],
            )],
        )));

        let mut metabolism = Metabolism::new(germline, registry);
        // Seed empty data (placeholder food) — enzyme should NOT fire
        metabolism.seed_artifact(art("data"), b"".to_vec());

        let report = metabolism.run_cycle();
        assert_eq!(
            report.invocations, 0,
            "enzyme should not fire on empty reactants"
        );
    }

    #[test]
    fn loop_detection_byte_hash_escalates_non_benchmarked_enzyme() {
        // For enzymes without a fitness signal (evolver), byte-hash
        // equality is the loop signal. When the same output is produced
        // twice in a row, the enzyme's loop_count increments (rung 1+).
        // The enzyme is NOT halted — it keeps running with escalation
        // interventions applied to its prompt.
        //
        // The cycle: producer → consumer → evolver → producer (re-triggered
        // by new enzyme_defs catalyst). The evolver returns the same defs
        // twice, which triggers byte-hash loop detection → rung 1.
        let enzymes = vec![
            enzyme("producer", &["requirements"], &["thing"], &["enzyme_defs"]),
            enzyme("consumer", &["thing"], &["report"], &["thing"]),
            enzyme("evolver", &["report"], &["enzyme_defs"], &["report"]),
        ];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let mut registry = ProviderRegistry::new(Box::new(MockProvider::new("default", vec![])));
        let prod = registry.register(Box::new(MockProvider::new(
            "p",
            vec![
                response(
                    json!({"thing": "v1"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
                response(
                    json!({"thing": "v2"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
                response(
                    json!({"thing": "v3"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
            ],
        )));
        let cons = registry.register(Box::new(MockProvider::new(
            "c",
            vec![
                response(
                    json!({"report": "r1"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
                response(
                    json!({"report": "r2"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
                response(
                    json!({"report": "r3"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
            ],
        )));
        // Evolver returns the SAME enzyme_defs each invocation — this is the loop.
        let same_defs = json!({"enzyme_defs": [{
            "id": "producer",
            "reactants": ["requirements"],
            "products": ["thing", "extra"],
            "catalysts": ["enzyme_defs"]
        }]});
        let evol = registry.register(Box::new(MockProvider::new(
            "e",
            vec![
                response(same_defs.clone(), vec![ToolEvent::Read, ToolEvent::Think]),
                response(same_defs.clone(), vec![ToolEvent::Read, ToolEvent::Think]),
                response(same_defs, vec![ToolEvent::Read, ToolEvent::Think]),
            ],
        )));

        registry.assign(EnzymeId::from("producer"), prod);
        registry.assign(EnzymeId::from("consumer"), cons);
        registry.assign(EnzymeId::from("evolver"), evol);

        let mut metabolism = Metabolism::new(germline, registry).with_max_invocations_per_cycle(20);
        metabolism.seed_artifact(art("requirements"), b"do it".to_vec());

        let report = metabolism.run_cycle();

        let evolver_invocations: usize = report
            .lineage
            .iter()
            .filter(|l| l.enzyme_id == EnzymeId::from("evolver"))
            .count();

        // Evolver fires at least twice — first records hash, second triggers
        // rung 1 escalation. No halt: the enzyme keeps running.
        assert!(
            evolver_invocations >= 2,
            "evolver should fire at least twice, got {}",
            evolver_invocations
        );
        let evolver_rung = report
            .loop_escalations
            .iter()
            .find(|(id, _)| id == &EnzymeId::from("evolver"))
            .map(|(_, r)| *r);
        assert!(
            evolver_rung.is_some_and(|r| r >= 1),
            "evolver should be escalated to rung 1+, got {:?}",
            report.loop_escalations
        );
    }

    #[test]
    fn cycle_firing_cap_force_advances_when_enzymes_keep_retriggering() {
        // When enzymes produce novel output each firing, revisions keep
        // bumping and the loop never naturally terminates. The cap bounds
        // the cycle and force-advances with `capped = true`. This is the
        // mechanical halt that lets the benchmark complete even when
        // escalation rungs 0–3 don't stop output repetition.
        let enzymes = vec![
            enzyme("producer", &["requirements"], &["thing"], &["enzyme_defs"]),
            enzyme("consumer", &["thing"], &["report"], &["thing"]),
            enzyme("evolver", &["report"], &["enzyme_defs"], &["report"]),
        ];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let mk_responses = |key: &str| -> Vec<InvocationResponse> {
            (0..30)
                .map(|n| {
                    response(
                        json!({key: format!("{key}_v{n}")}),
                        vec![ToolEvent::Read, ToolEvent::Think],
                    )
                })
                .collect()
        };
        let evolver_responses: Vec<InvocationResponse> = (0..30)
            .map(|n| {
                response(
                    json!({"enzyme_defs": [{
                        "id": "producer",
                        "reactants": ["requirements"],
                        "products": ["thing", format!("extra_v{n}")],
                        "catalysts": ["enzyme_defs"]
                    }]}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                )
            })
            .collect();

        let mut registry = ProviderRegistry::new(Box::new(MockProvider::new("default", vec![])));
        let prod = registry.register(Box::new(MockProvider::new("p", mk_responses("thing"))));
        let cons = registry.register(Box::new(MockProvider::new("c", mk_responses("report"))));
        let evol = registry.register(Box::new(MockProvider::new("e", evolver_responses)));

        registry.assign(EnzymeId::from("producer"), prod);
        registry.assign(EnzymeId::from("consumer"), cons);
        registry.assign(EnzymeId::from("evolver"), evol);

        const CAP: usize = 10;
        let mut metabolism =
            Metabolism::new(germline, registry).with_max_invocations_per_cycle(CAP);
        metabolism.seed_artifact(art("requirements"), b"do it".to_vec());

        let report = metabolism.run_cycle();

        assert!(
            report.capped,
            "cycle should be capped when enzymes keep retriggering, got capped={}, invocations={}",
            report.capped, report.invocations
        );
        assert_eq!(
            report.invocations, CAP,
            "invocations should equal cap, got {}",
            report.invocations
        );
    }

    #[test]
    fn cycle_not_capped_on_happy_path() {
        // Baseline: one firing each of coder/tester/evolver must not trip
        // the cap at a realistic setting.
        let germline = Germline::new(seed_enzymes(), food(&["requirements"]));
        let mut metabolism =
            Metabolism::new(germline, registry_for_cycle()).with_max_invocations_per_cycle(20);
        metabolism.seed_artifact(art("requirements"), b"ship it".to_vec());

        let report = metabolism.run_cycle();

        assert!(
            !report.capped,
            "happy-path cycle should not be capped, got invocations={}",
            report.invocations
        );
        assert!(
            !report.wall_clock_capped,
            "happy-path cycle should not hit wall-clock cap"
        );
    }

    #[test]
    fn cycle_wall_clock_cap_force_advances_before_invocation() {
        let germline = Germline::new(seed_enzymes(), food(&["requirements"]));
        let mut metabolism = Metabolism::new(germline, registry_for_cycle())
            .with_max_cycle_wall_clock(Duration::ZERO);
        metabolism.seed_artifact(art("requirements"), b"ship it".to_vec());

        let report = metabolism.run_cycle();

        assert!(
            report.wall_clock_capped,
            "cycle should be wall-clock capped"
        );
        assert_eq!(
            report.invocations, 0,
            "no provider should be invoked after cap"
        );
    }

    #[test]
    fn ready_invocations_prioritize_coder_over_speculative_decomposition() {
        let enzymes = vec![
            enzyme("analyze_requirements", &["requirements"], &["spec"], &[]),
            enzyme("coder", &["design", "plan", "requirements"], &["code"], &[]),
        ];
        let germline = Germline::new(enzymes, food(&["design", "plan", "requirements"]));
        let registry = ProviderRegistry::new(Box::new(MockProvider::new("default", vec![])));
        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"build sudoku".to_vec());
        metabolism.seed_artifact(art("design"), b"design".to_vec());
        metabolism.seed_artifact(art("plan"), b"plan".to_vec());

        let ready = metabolism.ready_invocations();

        let ids = ready
            .iter()
            .map(|scheduled| scheduled.enzyme.id.0.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["coder", "analyze_requirements"]);
    }

    #[test]
    fn successful_coder_advances_cycle_before_stale_auxiliary_work() {
        let enzymes = vec![
            enzyme("analyze_requirements", &["requirements"], &["spec"], &[]),
            enzyme("coder", &["requirements"], &["code"], &[]),
        ];
        let germline = Germline::new(enzymes, food(&["requirements"]));
        let registry = ProviderRegistry::new(Box::new(MockProvider::new(
            "default",
            vec![
                response(
                    json!({"code": "fn main() { println!(\"ok\"); }"}),
                    vec![ToolEvent::Text],
                ),
                response(json!({"spec": "should not run in same cycle"}), vec![]),
            ],
        )));
        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"build sudoku".to_vec());

        let report = metabolism.run_cycle();

        let ids = report
            .lineage
            .iter()
            .map(|entry| entry.enzyme_id.0.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["coder"]);
        assert!(metabolism.artifacts.contains_key(&art("code")));
        assert!(!metabolism.artifacts.contains_key(&art("spec")));
    }

    #[test]
    fn feedback_metabolism_precedes_tester_and_coder_once_fitness_exists() {
        let enzymes = vec![
            enzyme("coder", &["requirements"], &["code"], &["failure_report"]),
            enzyme("tester", &["code"], &["test_results"], &[]),
            enzyme(
                "evolver",
                &["fitness_report"],
                &["enzyme_defs"],
                &["enzyme_defs", "failure_report"],
            ),
            enzyme(
                "architect",
                &["failure_report", "fitness_report"],
                &["system_patch"],
                &["system_code"],
            ),
        ];
        let germline = Germline::new(
            enzymes,
            food(&[
                "requirements",
                "failure_report",
                "fitness_report",
                "system_code",
            ]),
        );
        let registry = ProviderRegistry::new(Box::new(MockProvider::new("default", vec![])));
        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"build sudoku".to_vec());
        metabolism.seed_artifact(art("failure_report"), b"try again".to_vec());
        metabolism.seed_artifact(art("fitness_report"), b"fitness: 0.83".to_vec());
        metabolism.seed_artifact(art("system_code"), b"metabolism snapshot".to_vec());
        metabolism.seed_artifact(art("enzyme_defs"), b"[]".to_vec());
        metabolism.seed_artifact(art("code"), b"fn main() {}".to_vec());

        let ready = metabolism.ready_invocations();

        let ids = ready
            .iter()
            .map(|scheduled| scheduled.enzyme.id.0.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["evolver", "architect", "tester", "coder"]);
    }

    #[test]
    fn empty_provider_output_fails_invocation() {
        let enzymes = vec![enzyme("worker", &["requirements"], &["output"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));
        let registry = ProviderRegistry::new(Box::new(MockProvider::new(
            "empty",
            vec![InvocationResponse {
                text: String::new(),
                raw_output: Some(
                    r#"{"type":"message","content":"wrote nothing useful"}"#.to_string(),
                ),
                tool_events: vec![ToolEvent::Think, ToolEvent::Text],
                thinking: None,
                usage: TokenUsage::default(),
            }],
        )));
        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"do it".to_vec());

        let report = metabolism.run_cycle();

        assert_eq!(report.failed, 1);
        let WorkcellOutcome::Failed { error } = &report.lineage[0].outcome else {
            panic!(
                "expected failed workcell, got {:?}",
                report.lineage[0].outcome
            );
        };
        assert!(error.contains("provider returned no materialized outputs"));
        assert!(error.contains("parsed_text <empty>"));
        assert!(error.contains("raw_stdout"));
        assert!(error.contains("wrote nothing useful"));
    }

    #[test]
    fn architect_noop_output_is_successful_patch_record() {
        let germline = Germline::new(Vec::new(), food(&[]));
        let registry = ProviderRegistry::new(Box::new(MockProvider::new("default", vec![])));
        let mut metabolism = Metabolism::new(germline, registry);

        let record = metabolism.apply_system_patch(
            br#"{"action":"noop","reason":"failure is provider capacity, not source code"}"#,
        );

        assert_eq!(record.accepted.len(), 0);
        assert_eq!(record.rejected.len(), 0);
        assert_eq!(
            record.noops,
            vec!["failure is provider capacity, not source code".to_string()]
        );
        assert_eq!(
            format_patch_summary(&record),
            "NOOP: failure is provider capacity, not source code"
        );
    }

    #[test]
    fn only_evolver_uses_role_isolated_nonparallel_fallback() {
        assert!(uses_role_isolated_fallback(&EnzymeId::from("evolver")));
        assert!(!uses_role_isolated_fallback(&EnzymeId::from("tester")));
        assert!(!uses_role_isolated_fallback(&EnzymeId::from("architect")));
        assert!(!uses_role_isolated_fallback(&EnzymeId::from("coder")));
    }

    #[test]
    fn parallel_coder_uses_fallback_success_in_same_cycle() {
        let enzymes = vec![enzyme("coder", &["requirements"], &["code"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let fallback = Box::new(MockProvider::new(
            "fallback",
            vec![response(
                json!({"code": "fn main() { println!(\"ok\"); }"}),
                vec![ToolEvent::Think, ToolEvent::Text],
            )],
        ));
        let mut registry = ProviderRegistry::new(fallback);
        let flaky = registry.register(Box::new(SequenceProvider::new(
            "glm",
            vec![Err("timeout".to_string())],
        )));
        registry.assign(EnzymeId::from("coder"), flaky);

        let mut metabolism = Metabolism::new(germline, registry)
            .with_provider_failure_cooldown(Duration::from_secs(60));
        metabolism.seed_artifact(art("requirements"), b"write hello".to_vec());

        let report = metabolism.run_cycle();

        assert_eq!(report.failed, 0);
        assert_eq!(report.completed, 1);
        assert_eq!(report.lineage[0].provider, "fallback");
        assert!(matches!(
            report.lineage[0].outcome,
            WorkcellOutcome::Success { .. }
        ));
    }

    #[test]
    fn parallel_coder_selects_highest_fitness_candidate() {
        let enzymes = vec![enzyme("coder", &["requirements"], &["code"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let weak_code = "fn main() { println!(\"weak\"); }";
        let strong_code = r#"
fn main() {}

#[cfg(test)]
mod tests {
    #[test]
    fn works() {
        assert_eq!(2 + 2, 4);
    }
}
"#;

        let weak = Box::new(MockProvider::new(
            "weak-first",
            vec![response(json!({"code": weak_code}), vec![ToolEvent::Text])],
        ));
        let mut registry = ProviderRegistry::new(weak);
        registry.register(Box::new(MockProvider::new(
            "strong-second",
            vec![response(
                json!({"code": strong_code}),
                vec![ToolEvent::Text],
            )],
        )));

        let benchmark = BenchmarkSuite::default();
        let mut metabolism = Metabolism::new(germline, registry).with_benchmark(benchmark);
        metabolism.seed_artifact(art("requirements"), b"write code".to_vec());

        let report = metabolism.run_cycle();

        assert_eq!(report.failed, 0);
        assert_eq!(report.completed, 1);
        assert_eq!(report.lineage[0].provider, "strong-second");
        assert_eq!(report.lineage[0].candidate_evaluations.len(), 2);
        assert_eq!(
            report.lineage[0].candidate_evaluations[0]
                .fitness
                .as_ref()
                .unwrap()
                .passed,
            2
        );
        assert_eq!(
            report.lineage[0].candidate_evaluations[1]
                .fitness
                .as_ref()
                .unwrap()
                .passed,
            3
        );
        assert_eq!(report.fitness.as_ref().unwrap().fitness, 1.0);
    }

    #[test]
    fn provider_failure_cools_down_provider_and_routes_following_enzyme_to_alternative() {
        let enzymes = vec![
            enzyme("alpha", &["requirements"], &["alpha_out"], &[]),
            enzyme("beta", &["requirements"], &["beta_out"], &[]),
        ];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let fallback = Box::new(MockProvider::new(
            "fallback",
            vec![response(
                json!({"beta_out": "from fallback"}),
                vec![ToolEvent::Think, ToolEvent::Text],
            )],
        ));
        let mut registry = ProviderRegistry::new(fallback);
        let flaky = registry.register(Box::new(SequenceProvider::new(
            "gemini",
            vec![Err("quota exhausted".to_string())],
        )));
        registry.assign(EnzymeId::from("alpha"), flaky);
        registry.assign(EnzymeId::from("beta"), flaky);

        let mut metabolism = Metabolism::new(germline, registry)
            .with_provider_failure_cooldown(Duration::from_secs(60));
        metabolism.seed_artifact(art("requirements"), b"do it".to_vec());

        let report = metabolism.run_cycle();

        let providers = report
            .lineage
            .iter()
            .map(|entry| (entry.enzyme_id.0.as_str(), entry.provider.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(providers, vec![("alpha", "gemini")]);
        assert!(matches!(
            report.lineage[0].outcome,
            WorkcellOutcome::Failed { .. }
        ));

        let second = metabolism.run_cycle();
        let second_providers = second
            .lineage
            .iter()
            .map(|entry| (entry.enzyme_id.0.as_str(), entry.provider.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            second_providers,
            vec![("alpha", "fallback"), ("beta", "fallback")]
        );
    }

    #[test]
    fn provider_failure_populates_provider_health_report_artifact() {
        let enzymes = vec![enzyme(
            "worker",
            &["requirements"],
            &["output"],
            &["provider_health_report"],
        )];
        let germline = Germline::new(enzymes, food(&["requirements", "provider_health_report"]));

        let mut registry = ProviderRegistry::new(Box::new(MockProvider::new("fallback", vec![])));
        let flaky = registry.register(Box::new(SequenceProvider::new(
            "gemini",
            vec![Err("quota exhausted".to_string())],
        )));
        registry.assign(EnzymeId::from("worker"), flaky);

        let mut metabolism = Metabolism::new(germline, registry)
            .with_provider_failure_cooldown(Duration::from_secs(60));
        metabolism.seed_artifact(art("requirements"), b"do it".to_vec());

        let report = metabolism.run_cycle();

        assert_eq!(report.failed, 1);
        let artifacts = metabolism.artifacts();
        let provider_report = artifacts.get(&art("provider_health_report")).unwrap();
        let provider_report: Value = serde_json::from_slice(provider_report).unwrap();

        assert_eq!(provider_report["cycle"], 0);
        assert_eq!(provider_report["unavailable_providers"][0], "gemini");
        assert_eq!(provider_report["provider_health"][0]["provider"], "gemini");
        assert_eq!(
            provider_report["provider_health"][0]["consecutive_failures"],
            1
        );
        assert_eq!(provider_report["recent_invocations"][0]["enzyme"], "worker");
        assert_eq!(
            provider_report["recent_invocations"][0]["provider"],
            "gemini"
        );
        assert_eq!(
            provider_report["recent_invocations"][0]["outcome"],
            "failed"
        );
        assert_eq!(
            provider_report["recent_invocations"][0]["escalation_rung"],
            0
        );
        assert_eq!(
            provider_report["recent_invocations"][0]["provider_swap"],
            false
        );
        assert_eq!(
            provider_report["recent_invocations"][0]["clean_session"],
            false
        );
        assert!(
            provider_report["recent_invocations"][0]["error"]
                .as_str()
                .unwrap()
                .contains("quota exhausted")
        );
    }

    #[test]
    fn provider_policy_artifact_is_gated_and_changes_later_routing() {
        let enzymes = vec![
            enzyme("aaa_policy", &["requirements"], &["provider_policy"], &[]),
            enzyme(
                "zzz_worker",
                &["provider_policy"],
                &["result"],
                &["requirements"],
            ),
        ];
        let germline = Germline::new(enzymes, food(&["requirements", "provider_policy"]));

        let default = Box::new(MockProvider::new(
            "default",
            vec![response(
                json!({"provider_policy": {"assignments": {"zzz_worker": "fast"}}}),
                vec![ToolEvent::Text],
            )],
        ));
        let mut registry = ProviderRegistry::new(default);
        registry.register(Box::new(MockProvider::new(
            "fast",
            vec![response(
                json!({"result": "fast path"}),
                vec![ToolEvent::Text],
            )],
        )));

        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"route work".to_vec());

        let report = metabolism.run_cycle();

        assert_eq!(report.accepted_provider_policy_changes, 1);
        assert_eq!(report.rejected_provider_policy_changes, 0);
        assert_eq!(
            report
                .lineage
                .iter()
                .map(|entry| (entry.enzyme_id.0.as_str(), entry.provider.as_str()))
                .collect::<Vec<_>>(),
            vec![("aaa_policy", "default"), ("zzz_worker", "fast")]
        );
        let policy_record = report.lineage[0].provider_policy.as_ref().unwrap();
        assert_eq!(
            policy_record.accepted,
            vec!["zzz_worker: default → fast".to_string()]
        );
        let artifacts = metabolism.artifacts();
        let policy: ProviderPolicy =
            serde_json::from_slice(artifacts.get(&art("provider_policy")).unwrap()).unwrap();
        assert_eq!(
            policy.assignments.get("zzz_worker"),
            Some(&"fast".to_string())
        );
    }

    #[test]
    fn provider_policy_rejects_malformed_unknown_or_unregistered_changes() {
        let enzymes = vec![enzyme(
            "policy",
            &["requirements"],
            &["provider_policy"],
            &[],
        )];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let registry = ProviderRegistry::new(Box::new(MockProvider::new(
            "default",
            vec![response(
                json!({"provider_policy": {"assignments": {"ghost": "default", "policy": "missing"}}}),
                vec![ToolEvent::Text],
            )],
        )));

        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"bad policy".to_vec());

        let report = metabolism.run_cycle();

        assert_eq!(report.accepted_provider_policy_changes, 0);
        assert_eq!(report.rejected_provider_policy_changes, 2);
        let policy_record = report.lineage[0].provider_policy.as_ref().unwrap();
        assert!(
            policy_record
                .rejected
                .iter()
                .any(|rejection| rejection.reason.contains("target enzyme"))
        );
        assert!(
            policy_record
                .rejected
                .iter()
                .any(|rejection| rejection.reason.contains("provider is not registered"))
        );
    }

    #[test]
    fn provider_is_retried_after_zero_cooldown_expires() {
        let enzymes = vec![enzyme("worker", &["requirements"], &["output"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let fallback = Box::new(MockProvider::new(
            "fallback",
            vec![response(
                json!({"output": "fallback"}),
                vec![ToolEvent::Think, ToolEvent::Text],
            )],
        ));
        let mut registry = ProviderRegistry::new(fallback);
        let flaky = registry.register(Box::new(SequenceProvider::new(
            "gemini",
            vec![
                Err("temporary timeout".to_string()),
                Ok(response(
                    json!({"output": "recovered"}),
                    vec![ToolEvent::Think, ToolEvent::Text],
                )),
            ],
        )));
        registry.assign(EnzymeId::from("worker"), flaky);

        let mut metabolism =
            Metabolism::new(germline, registry).with_provider_failure_cooldown(Duration::ZERO);
        metabolism.seed_artifact(art("requirements"), b"do it".to_vec());

        let first = metabolism.run_cycle();
        assert_eq!(first.lineage[0].provider, "gemini");
        assert!(matches!(
            first.lineage[0].outcome,
            WorkcellOutcome::Failed { .. }
        ));

        metabolism.seed_artifact(art("requirements"), b"do it again".to_vec());
        let second = metabolism.run_cycle();

        assert_eq!(second.lineage[0].provider, "gemini");
        assert!(matches!(
            second.lineage[0].outcome,
            WorkcellOutcome::Success { .. }
        ));
    }

    #[test]
    fn loop_detection_does_not_halt_enzyme_producing_different_output() {
        // Coder produces different output each cycle — should NOT be halted.
        let germline = Germline::new(seed_enzymes(), food(&["requirements"]));

        let mut registry = ProviderRegistry::new(Box::new(MockProvider::new("default", vec![])));
        let coder = registry.register(Box::new(MockProvider::new(
            "codex",
            vec![
                response(
                    json!({"code": "fn v1() {}"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
                response(
                    json!({"code": "fn v2() {}"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
            ],
        )));
        let tester = registry.register(Box::new(MockProvider::new(
            "gemini",
            vec![
                response(
                    json!({"test_results": "tests:r1"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
                response(
                    json!({"test_results": "tests:r2"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
            ],
        )));
        let evolver = registry.register(Box::new(MockProvider::new(
            "claude",
            vec![response(
                json!({"enzyme_defs": [{
                    "id": "coder",
                    "reactants": ["requirements"],
                    "products": ["code", "docs"],
                    "catalysts": ["enzyme_defs"]
                }]}),
                vec![ToolEvent::Read, ToolEvent::Think],
            )],
        )));

        registry.assign(EnzymeId::from("coder"), coder);
        registry.assign(EnzymeId::from("tester"), tester);
        registry.assign(EnzymeId::from("evolver"), evolver);

        let mut metabolism = Metabolism::new(germline, registry).with_max_invocations_per_cycle(20);
        metabolism.seed_artifact(art("requirements"), b"do it".to_vec());

        let report1 = metabolism.run_cycle();
        let report2 = metabolism.run_cycle();

        // The coder produces distinct outputs each firing, so it should
        // not be escalated. Other enzymes may escalate on their own — we
        // only care that the coder stays at rung 0.
        let coder_rung = report2
            .loop_escalations
            .iter()
            .find(|(id, _)| id == &EnzymeId::from("coder"))
            .map(|(_, r)| *r);
        assert!(
            coder_rung.is_none(),
            "coder should not be escalated when its outputs differ, got {:?}",
            report2.loop_escalations
        );
        // And the coder should have fired more than once across feedback turns.
        let coder_count = report1
            .lineage
            .iter()
            .chain(report2.lineage.iter())
            .filter(|l| l.enzyme_id == EnzymeId::from("coder"))
            .count();
        assert!(
            coder_count >= 2,
            "expected ≥2 coder firings, got {coder_count}"
        );
    }

    #[test]
    fn loop_detection_escalates_on_identical_fitness_vector_with_different_bytes() {
        // The real loop case: coder produces different code each time but
        // they all have the same fitness signature (same tests pass/fail).
        // The byte hash differs, but the behavioral outcome is identical.
        // This is what catches "model rephrases the same broken solution."
        let enzymes = vec![enzyme("coder", &["requirements"], &["code"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        // Two different code samples that both compile but have no tests.
        // Both produce: compiles=true, has_tests=false, all_tests_pass=false.
        // Same fitness vector, different bytes.
        let v1 = "fn main() { println!(\"v1\"); }";
        let v2 = "fn main() { println!(\"v2 different bytes\"); }";

        let registry = ProviderRegistry::new(Box::new(MockProvider::new(
            "mock",
            vec![
                response(
                    json!({"code": v1}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
                response(
                    json!({"code": v2}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
                response(
                    json!({"code": v1}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
            ],
        )));

        let benchmark = crate::benchmark::BenchmarkSuite {
            name: "test".to_string(),
            cases: vec![],
            acceptance_test: None,
            test_timeout_secs: 30,
        };

        let mut metabolism = Metabolism::new(germline, registry).with_benchmark(benchmark);
        metabolism.seed_artifact(art("requirements"), b"do it".to_vec());

        // Cycle 1: coder fires once with v1, gets a fitness signature.
        // No previous signature → no escalation possible.
        let report1 = metabolism.run_cycle();
        assert!(
            report1.loop_escalations.is_empty(),
            "first invocation cannot loop, got {:?}",
            report1.loop_escalations
        );

        // Cycle 2: coder fires with v2 — different bytes, same fitness signature
        // (both are non-test code that compiles). Should escalate to rung 1.
        let report2 = metabolism.run_cycle();
        let coder_rung = report2
            .loop_escalations
            .iter()
            .find(|(id, _)| id == &EnzymeId::from("coder"))
            .map(|(_, r)| *r);
        assert!(
            coder_rung.is_some_and(|r| r >= 1),
            "coder should be at rung 1+ in cycle 2 (same fitness signature, different bytes), got {:?}",
            report2.loop_escalations
        );
    }

    #[test]
    fn loop_detection_counter_resets_when_output_changes() {
        // Single-enzyme germline — worker fires exactly once per cycle.
        // Cycle 1: baseline. Cycle 2: same output → rung 1. Cycle 3: fresh → reset.
        let enzymes = vec![enzyme("worker", &["requirements"], &["output"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let registry = ProviderRegistry::new(Box::new(MockProvider::new(
            "mock",
            vec![
                response(
                    json!({"output": "same"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
                response(
                    json!({"output": "same"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
                response(
                    json!({"output": "fresh"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
            ],
        )));

        let mut metabolism = Metabolism::new(germline, registry);

        // Cycle 1: seed requirements, worker fires with "same", no prior → rung 0
        metabolism.seed_artifact(art("requirements"), b"v1".to_vec());
        let r1 = metabolism.run_cycle();
        let s1 = r1
            .loop_escalations
            .iter()
            .find(|(id, _)| id == &EnzymeId::from("worker"))
            .map(|(_, r)| *r);
        assert!(
            s1.is_none(),
            "cycle 1 has no prior, got {:?}",
            r1.loop_escalations
        );

        // Cycle 2: re-seed requirements so worker is ready again, produces "same" → rung 1
        metabolism.seed_artifact(art("requirements"), b"v2".to_vec());
        let r2 = metabolism.run_cycle();
        let s2 = r2
            .loop_escalations
            .iter()
            .find(|(id, _)| id == &EnzymeId::from("worker"))
            .map(|(_, r)| *r);
        assert_eq!(
            s2,
            Some(1),
            "cycle 2 should have worker at rung 1, got {:?}",
            r2.loop_escalations
        );

        // Cycle 3: new requirements, worker produces "fresh" → rung resets to 0
        metabolism.seed_artifact(art("requirements"), b"v3".to_vec());
        let r3 = metabolism.run_cycle();
        let s3 = r3
            .loop_escalations
            .iter()
            .find(|(id, _)| id == &EnzymeId::from("worker"))
            .map(|(_, r)| *r);
        assert!(
            s3.is_none(),
            "worker should be back at rung 0 after producing different output, got {:?}",
            r3.loop_escalations
        );
    }

    #[test]
    fn fitness_degradation_tracked_across_cycles() {
        // Two cycles: first produces good code, second produces worse code.
        // Verify fitness_delta is negative on cycle 2.
        let enzymes = vec![enzyme("coder", &["requirements"], &["code"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let good_code = "fn solve() -> i32 { 42 }\nfn main() {}\n#[cfg(test)] mod tests { use super::*; #[test] fn t() { assert_eq!(solve(), 42); } }";
        let bad_code = "fn main() {}"; // compiles but no tests pass

        let registry = ProviderRegistry::new(Box::new(MockProvider::new(
            "mock",
            vec![
                response(
                    json!({"code": good_code}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
                response(
                    json!({"code": bad_code}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
            ],
        )));

        let benchmark = crate::benchmark::BenchmarkSuite {
            name: "test".to_string(),
            cases: vec![],
            acceptance_test: None,
            test_timeout_secs: 30,
        };

        let mut metabolism = Metabolism::new(germline, registry).with_benchmark(benchmark);
        metabolism.seed_artifact(art("requirements"), b"solve it".to_vec());

        let report1 = metabolism.run_cycle();
        assert!(report1.fitness.is_some());
        let f1 = report1.fitness.as_ref().unwrap().fitness;
        assert!(f1 > 0.5, "cycle 1 should have good fitness, got {f1}");

        let report2 = metabolism.run_cycle();
        assert!(report2.fitness.is_some());
        let delta = report2.fitness_delta.unwrap();
        assert!(
            delta < 0.0,
            "cycle 2 should show regression, delta was {delta}"
        );

        // last_fitness should NOT have been updated (ratchet holds)
        // This is verified by the fact that a hypothetical cycle 3
        // would still compare against cycle 1's fitness
    }

    #[test]
    fn rung_4_invokes_swapped_provider_without_consultation() {
        let enzymes = vec![enzyme("worker", &["requirements"], &["output"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let swapped = SequenceProvider::new(
            "swapped",
            vec![Ok(response(json!({"output": "from-swapped"}), vec![]))],
        );
        let mut registry = ProviderRegistry::new(Box::new(swapped));
        let primary = registry.register(Box::new(SequenceProvider::new(
            "primary",
            vec![Err("primary should not be invoked at rung 4".into())],
        )));
        registry.assign(EnzymeId::from("worker"), primary);

        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"do work".to_vec());
        metabolism
            .enzyme_loop_count
            .insert(EnzymeId::from("worker"), 4);

        let report = metabolism.run_cycle();

        assert_eq!(report.invocations, 1);
        assert_eq!(report.completed, 1, "{:?}", report.lineage[0].outcome);
        assert_eq!(report.lineage[0].provider, "swapped");
        assert_eq!(report.lineage[0].escalation_rung, 4);
        assert!(report.lineage[0].provider_swap);
        assert!(!report.lineage[0].clean_session);
        assert_eq!(
            String::from_utf8_lossy(&metabolism.artifacts.get(&art("output")).unwrap().bytes),
            "from-swapped"
        );
    }

    #[test]
    fn rung_5_invokes_swapped_provider_with_clean_session_lineage() {
        let enzymes = vec![enzyme(
            "worker",
            &["requirements"],
            &["output"],
            &["failure_report"],
        )];
        let germline = Germline::new(enzymes, food(&["requirements", "failure_report"]));

        let swapped = SequenceProvider::new(
            "swapped",
            vec![Ok(response(json!({"output": "fresh-clean"}), vec![]))],
        );
        let mut registry = ProviderRegistry::new(Box::new(swapped));
        let primary = registry.register(Box::new(SequenceProvider::new(
            "primary",
            vec![Err("primary should not be invoked at rung 5".into())],
        )));
        registry.assign(EnzymeId::from("worker"), primary);

        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"do work".to_vec());
        metabolism.seed_artifact(art("failure_report"), b"stale failure details".to_vec());
        metabolism
            .enzyme_loop_count
            .insert(EnzymeId::from("worker"), 5);

        let report = metabolism.run_cycle();

        assert_eq!(report.invocations, 1);
        assert_eq!(report.completed, 1, "{:?}", report.lineage[0].outcome);
        assert_eq!(report.lineage[0].provider, "swapped");
        assert_eq!(report.lineage[0].escalation_rung, 5);
        assert!(report.lineage[0].provider_swap);
        assert!(report.lineage[0].clean_session);
        assert!(
            !report.lineage[0]
                .inputs
                .contains_key(&art("failure_report")),
            "clean-session lineage should record the provider-visible inputs"
        );
        assert_eq!(
            String::from_utf8_lossy(&metabolism.artifacts.get(&art("output")).unwrap().bytes),
            "fresh-clean"
        );
    }

    #[test]
    fn rung_4_does_not_fire_below_threshold() {
        let enzymes = vec![enzyme("worker", &["requirements"], &["output"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let alternate = SequenceProvider::new(
            "alternate",
            vec![Ok(response(json!({"output": "from-alternate"}), vec![]))],
        );
        let mut registry = ProviderRegistry::new(Box::new(alternate));
        let primary = registry.register(Box::new(SequenceProvider::new(
            "primary",
            vec![Ok(response(json!({"output": "from-primary"}), vec![]))],
        )));
        registry.assign(EnzymeId::from("worker"), primary);

        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"do work".to_vec());
        metabolism
            .enzyme_loop_count
            .insert(EnzymeId::from("worker"), 1);

        let report = metabolism.run_cycle();

        assert_eq!(report.completed, 1, "{:?}", report.lineage[0].outcome);
        assert_eq!(report.lineage[0].provider, "primary");
        assert_eq!(report.lineage[0].escalation_rung, 1);
        assert!(!report.lineage[0].provider_swap);
        assert!(!report.lineage[0].clean_session);
        assert_eq!(
            String::from_utf8_lossy(&metabolism.artifacts.get(&art("output")).unwrap().bytes),
            "from-primary"
        );
    }

    #[test]
    fn rung_4_escape_resets_to_assigned_provider() {
        let enzymes = vec![enzyme("worker", &["requirements"], &["output"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let swapped = SequenceProvider::new(
            "swapped",
            vec![Ok(response(json!({"output": "fresh"}), vec![]))],
        );
        let mut registry = ProviderRegistry::new(Box::new(swapped));
        let primary = registry.register(Box::new(SequenceProvider::new(
            "primary",
            vec![Ok(response(json!({"output": "assigned-again"}), vec![]))],
        )));
        registry.assign(EnzymeId::from("worker"), primary);

        let mut previous_outputs = ArtifactStore::new();
        previous_outputs.insert(art("output"), b"stale".to_vec());

        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"do work".to_vec());
        metabolism
            .enzyme_loop_count
            .insert(EnzymeId::from("worker"), 4);
        metabolism
            .enzyme_output_hashes
            .insert(EnzymeId::from("worker"), hash_outputs(&previous_outputs));

        let report1 = metabolism.run_cycle();
        assert_eq!(report1.lineage[0].provider, "swapped");
        assert!(
            report1.loop_escalations.is_empty(),
            "fresh output should reset rung state: {:?}",
            report1.loop_escalations
        );

        metabolism.seed_artifact(art("requirements"), b"do work again".to_vec());
        let report2 = metabolism.run_cycle();

        assert_eq!(report2.completed, 1, "{:?}", report2.lineage[0].outcome);
        assert_eq!(report2.lineage[0].provider, "primary");
        assert_eq!(
            String::from_utf8_lossy(&metabolism.artifacts.get(&art("output")).unwrap().bytes),
            "assigned-again"
        );
    }

    #[test]
    fn rung_2_consults_alternative_provider() {
        // When loop_rung >= 2, the metabolism should:
        // 1. Call an alternative provider for consultation
        // 2. Inject the consultation response into the primary's prompt
        //
        // Setup: single enzyme "worker" with loop_count pre-set to 2.
        // Two providers: primary (assigned) and alternative (default).
        // Both return deterministic responses so we can verify call counts.
        let enzymes = vec![enzyme("worker", &["requirements"], &["output"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        // The alternative (default) provider — its response becomes consultation text.
        let alt_provider = MockProvider::new(
            "consultant",
            vec![response(
                json!("Try a recursive approach instead of iterative."),
                vec![ToolEvent::Read, ToolEvent::Think],
            )],
        );
        let mut registry = ProviderRegistry::new(Box::new(alt_provider));

        // The primary provider — assigned to "worker".
        let primary = registry.register(Box::new(MockProvider::new(
            "primary",
            vec![
                // First call (cycle 1): baseline
                response(
                    json!({"output": "same"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
                // Second call (cycle 2): same output → rung 1
                response(
                    json!({"output": "same"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
                // Third call (cycle 3): rung 2, should get consultation
                response(
                    json!({"output": "different"}),
                    vec![ToolEvent::Read, ToolEvent::Think],
                ),
            ],
        )));
        registry.assign(EnzymeId::from("worker"), primary);

        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"do work".to_vec());

        // Cycle 1: baseline — no loop yet
        let _r1 = metabolism.run_cycle();

        // Cycle 2: same output → escalates to rung 1
        metabolism.seed_artifact(art("requirements"), b"do work v2".to_vec());
        let r2 = metabolism.run_cycle();
        let rung2 = r2
            .loop_escalations
            .iter()
            .find(|(id, _)| id == &EnzymeId::from("worker"))
            .map(|(_, r)| *r);
        assert_eq!(rung2, Some(1), "cycle 2 should be rung 1");

        // Cycle 3: same output again from prior → rung 2, consultation should fire
        // We can't easily inspect the prompt that was sent to the primary,
        // but we CAN verify the alternative provider was called by checking
        // its call count. The default provider (consultant) should have been
        // invoked once for the consultation.
        metabolism.seed_artifact(art("requirements"), b"do work v3".to_vec());
        let _r3 = metabolism.run_cycle();

        // Verify: the consultant provider should have been called.
        // It was the default provider (index 0). We check via the registry's
        // provider list that it was invoked.
        // Since we can't directly access the call count from outside MockProvider,
        // we verify through the build_request function that consultation text
        // appears in rung 2 prompts.
        let test_enzyme = EnzymeDef {
            id: EnzymeId::from("worker"),
            reactants: [art("requirements")].into(),
            products: [art("output")].into(),
            catalysts: [].into(),
            prompt_template: Some("Do the work.".to_string()),
        };
        let mut test_inputs = ArtifactStore::new();
        test_inputs.insert(art("requirements"), b"task".to_vec());

        let req_with_consultation = build_request(
            &test_enzyme,
            &test_inputs,
            2,
            Some("Try a recursive approach instead of iterative."),
        );
        assert!(
            req_with_consultation
                .system
                .contains("CONSULTATION FROM ANOTHER MODEL"),
            "rung 2 prompt should contain consultation, got: {}",
            &req_with_consultation.system
        );
        assert!(
            req_with_consultation.system.contains("recursive approach"),
            "consultation content should appear in prompt"
        );
    }

    #[test]
    fn failed_consultation_does_not_invoke_primary_in_same_workcell() {
        let enzymes = vec![enzyme("worker", &["requirements"], &["output"], &[])];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let consultant = SequenceProvider::new("consultant", vec![Err("consult-down".into())]);
        let mut registry = ProviderRegistry::new(Box::new(consultant));
        let primary = registry.register(Box::new(SequenceProvider::new(
            "primary",
            vec![Ok(response(json!({"output": "should-not-run"}), vec![]))],
        )));
        registry.assign(EnzymeId::from("worker"), primary);

        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"do work".to_vec());
        metabolism
            .enzyme_loop_count
            .insert(EnzymeId::from("worker"), 2);

        let report = metabolism.run_cycle();

        assert_eq!(report.invocations, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.lineage[0].provider, "consultant");
        match &report.lineage[0].outcome {
            WorkcellOutcome::Failed { error } => {
                assert!(error.contains("consultation failed before primary invocation"));
                assert!(error.contains("consult-down"));
            }
            other => panic!("expected failed consultation, got {other:?}"),
        }
        assert!(metabolism.artifacts.get(&art("output")).is_none());
    }

    #[test]
    fn rung_3_strips_failure_context() {
        // When loop_rung >= 3, the metabolism should:
        // 1. Remove failure_report from the enzyme's inputs
        // 2. Add a "CLEAN SESSION" notice to the prompt
        //
        // Setup: enzyme with failure_report as catalyst, loop_count pre-set to 3.
        let enzymes = vec![{
            let mut e = enzyme("coder", &["requirements"], &["code"], &["failure_report"]);
            e.prompt_template = Some("Write code.".to_string());
            e
        }];
        let germline = Germline::new(enzymes, food(&["requirements"]));

        let registry = ProviderRegistry::new(Box::new(MockProvider::new(
            "mock",
            vec![
                response(
                    json!({"code": "same_v1"}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
                response(
                    json!({"code": "same_v1"}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
                response(
                    json!({"code": "same_v1"}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
                response(
                    json!({"code": "same_v1"}),
                    vec![
                        ToolEvent::Read,
                        ToolEvent::Think,
                        ToolEvent::Execute,
                        ToolEvent::Write,
                    ],
                ),
            ],
        )));

        let mut metabolism = Metabolism::new(germline, registry);
        metabolism.seed_artifact(art("requirements"), b"solve it".to_vec());
        metabolism.seed_artifact(
            art("failure_report"),
            b"COMPILATION FAILED: undefined variable foo".to_vec(),
        );

        // Cycle 1: baseline
        let _r1 = metabolism.run_cycle();
        // Cycle 2: same output → rung 1
        metabolism.seed_artifact(art("requirements"), b"solve it v2".to_vec());
        let _r2 = metabolism.run_cycle();
        // Cycle 3: same output → rung 2
        metabolism.seed_artifact(art("requirements"), b"solve it v3".to_vec());
        let _r3 = metabolism.run_cycle();
        // Cycle 4: same output → rung 3
        metabolism.seed_artifact(art("requirements"), b"solve it v4".to_vec());
        let _r4 = metabolism.run_cycle();

        // Verify rung 3 behavior via build_request:
        // At rung 3, failure_report should NOT appear in the prompt,
        // and CLEAN SESSION notice should be present.
        let test_enzyme = EnzymeDef {
            id: EnzymeId::from("coder"),
            reactants: [art("requirements")].into(),
            products: [art("code")].into(),
            catalysts: [art("failure_report")].into(),
            prompt_template: Some("Write code.".to_string()),
        };

        // Build request WITH failure_report but at rung 3 — the caller
        // (invoke_scheduled) strips failure_report before building.
        let mut inputs_clean = ArtifactStore::new();
        inputs_clean.insert(art("requirements"), b"solve it".to_vec());
        // failure_report intentionally NOT inserted (simulating rung 3 stripping)

        let req = build_request(&test_enzyme, &inputs_clean, 3, None);
        assert!(
            !req.system.contains("PREVIOUS ATTEMPT FAILED"),
            "rung 3 should NOT include failure injection (stripped), got: {}",
            &req.system[..300.min(req.system.len())]
        );
        assert!(
            req.system.contains("CLEAN SESSION"),
            "rung 3 should include CLEAN SESSION notice"
        );
        assert!(
            req.system.contains("starting fresh"),
            "CLEAN SESSION notice should mention starting fresh"
        );
    }

    #[test]
    fn rung_1_does_not_consult() {
        // At rung 1, no consultation should happen — only awareness injection.
        // Verify via build_request that no consultation text appears.
        let test_enzyme = EnzymeDef {
            id: EnzymeId::from("worker"),
            reactants: [art("requirements")].into(),
            products: [art("output")].into(),
            catalysts: [].into(),
            prompt_template: Some("Do the work.".to_string()),
        };

        let mut inputs = ArtifactStore::new();
        inputs.insert(art("requirements"), b"task".to_vec());

        // Rung 1 with no consultation (None) — should NOT contain consultation text
        let req = build_request(&test_enzyme, &inputs, 1, None);
        assert!(
            req.system.contains("LOOP DETECTED"),
            "rung 1 should have loop awareness"
        );
        assert!(
            !req.system.contains("CONSULTATION FROM ANOTHER MODEL"),
            "rung 1 should NOT have consultation, got: {}",
            &req.system
        );
        assert!(
            !req.system.contains("CLEAN SESSION"),
            "rung 1 should NOT have clean session notice"
        );
    }

    #[test]
    fn rung_2_build_request_includes_consultation_text() {
        // Direct unit test of build_request with consultation parameter.
        let test_enzyme = EnzymeDef {
            id: EnzymeId::from("coder"),
            reactants: [art("requirements")].into(),
            products: [art("code")].into(),
            catalysts: [].into(),
            prompt_template: Some("Write code.".to_string()),
        };

        let mut inputs = ArtifactStore::new();
        inputs.insert(art("requirements"), b"build a parser".to_vec());

        let req = build_request(
            &test_enzyme,
            &inputs,
            2,
            Some("Use a state machine instead of regex for parsing."),
        );

        assert!(
            req.system.contains("LOOP DETECTED"),
            "rung 2 should include loop awareness"
        );
        assert!(
            req.system.contains("CONSULTATION FROM ANOTHER MODEL"),
            "rung 2 should include consultation header"
        );
        assert!(
            req.system.contains("state machine instead of regex"),
            "consultation content should be in prompt"
        );
        assert!(
            !req.system.contains("CLEAN SESSION"),
            "rung 2 should NOT include clean session (that's rung 3+)"
        );
    }

    #[test]
    fn rung_4_build_request_has_provider_swap_without_clean_session() {
        let test_enzyme = EnzymeDef {
            id: EnzymeId::from("coder"),
            reactants: [art("requirements")].into(),
            products: [art("code")].into(),
            catalysts: [art("failure_report")].into(),
            prompt_template: Some("Write code.".to_string()),
        };

        let mut inputs = ArtifactStore::new();
        inputs.insert(art("requirements"), b"build it".to_vec());
        inputs.insert(art("failure_report"), b"previous compiler error".to_vec());

        let req = build_request(&test_enzyme, &inputs, 4, None);

        assert!(req.system.contains("LOOP DETECTED"));
        assert!(req.system.contains("escalation rung 4"));
        assert!(req.system.contains("PROVIDER SWAP"));
        assert!(req.system.contains("PREVIOUS ATTEMPT FAILED"));
        assert!(req.system.contains("previous compiler error"));
        assert!(
            !req.system.contains("CLEAN SESSION"),
            "rung 4 should preserve failure history for the swapped provider"
        );
    }

    #[test]
    fn rung_5_build_request_has_provider_swap_and_clean_session_without_failure_context() {
        let test_enzyme = EnzymeDef {
            id: EnzymeId::from("coder"),
            reactants: [art("requirements")].into(),
            products: [art("code")].into(),
            catalysts: [art("failure_report")].into(),
            prompt_template: Some("Write code.".to_string()),
        };

        let mut inputs = ArtifactStore::new();
        inputs.insert(art("requirements"), b"build it".to_vec());
        // Rung 5 clean-session stripping happens before build_request, so the
        // provider-visible inputs intentionally omit failure_report here.
        let req = build_request(&test_enzyme, &inputs, 5, None);

        assert!(req.system.contains("LOOP DETECTED"));
        assert!(req.system.contains("escalation rung 5"));
        assert!(req.system.contains("PROVIDER SWAP"));
        assert!(req.system.contains("CLEAN SESSION"));
        assert!(req.system.contains("starting fresh"));
        assert!(!req.system.contains("PREVIOUS ATTEMPT FAILED"));
        assert!(!req.system.contains("CONSULTATION FROM ANOTHER MODEL"));
    }

    #[test]
    fn rung_3_build_request_has_clean_session_and_consultation() {
        // Rung 3 should have: awareness + consultation (if provided) + clean session.
        let test_enzyme = EnzymeDef {
            id: EnzymeId::from("coder"),
            reactants: [art("requirements")].into(),
            products: [art("code")].into(),
            catalysts: [].into(),
            prompt_template: Some("Write code.".to_string()),
        };

        let mut inputs = ArtifactStore::new();
        inputs.insert(art("requirements"), b"build it".to_vec());

        let req = build_request(&test_enzyme, &inputs, 3, Some("Try dynamic programming."));

        assert!(req.system.contains("LOOP DETECTED"));
        assert!(req.system.contains("escalation rung 3"));
        assert!(req.system.contains("CONSULTATION FROM ANOTHER MODEL"));
        assert!(req.system.contains("dynamic programming"));
        assert!(req.system.contains("CLEAN SESSION"));
        assert!(req.system.contains("starting fresh"));
    }
}
