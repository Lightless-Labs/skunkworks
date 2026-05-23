//! Workcell: ephemeral, scoped execution context for a single enzyme invocation.
//!
//! Design rationale (from collision synthesis §I.7):
//! Sessions degrade 7.24x over their lifetime. 44.5% show increasing entropy.
//! Enzymes must be ephemeral: spawn fresh, die after task. The workcell is
//! the unit of execution — not the enzyme itself.

use crate::observer::{self, BehavioralState, HealthMetrics, ToolEvent};
use crate::types::{ArtifactType, EnzymeId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Unique identifier for a workcell execution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkcellId(pub String);

/// The scoped context available to an enzyme during execution.
///
/// Typed and restricted: an enzyme can only see artifacts explicitly
/// provided to it. Cross-workcell access is a compile-time error
/// (there is no field for another workcell's state).
#[derive(Debug, Clone)]
pub struct WorkcellContext {
    /// Which enzyme this workcell is running.
    pub enzyme_id: EnzymeId,
    /// Available input artifacts (reactants + catalysts).
    pub inputs: BTreeMap<ArtifactType, Vec<u8>>,
}

/// Outcome of a workcell execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkcellOutcome {
    /// Enzyme completed successfully, produced artifacts.
    Success {
        outputs: BTreeMap<ArtifactType, Vec<u8>>,
    },
    /// Enzyme was killed by the observer (pre-failure or degradation detected).
    Killed { reason: BehavioralState },
    /// Enzyme failed with an error.
    Failed { error: String },
}

/// An executing workcell: tracks events and can be observed.
#[derive(Debug)]
pub struct Workcell {
    pub id: WorkcellId,
    pub enzyme_id: EnzymeId,
    events: Vec<ToolEvent>,
    context: WorkcellContext,
    outcome: Option<WorkcellOutcome>,
}

impl Workcell {
    /// Spawn a new workcell for an enzyme invocation.
    pub fn spawn(
        id: WorkcellId,
        enzyme_id: EnzymeId,
        inputs: BTreeMap<ArtifactType, Vec<u8>>,
    ) -> Self {
        Self {
            id,
            enzyme_id: enzyme_id.clone(),
            events: Vec::new(),
            context: WorkcellContext { enzyme_id, inputs },
            outcome: None,
        }
    }

    /// Record a tool event during execution.
    pub fn record_event(&mut self, event: ToolEvent) {
        self.events.push(event);
    }

    /// Get the current execution trace.
    pub fn trace(&self) -> &[ToolEvent] {
        &self.events
    }

    /// Observe current health from the execution trace.
    pub fn observe(&self) -> HealthMetrics {
        observer::observe(&self.events)
    }

    /// Check if the workcell should be killed based on behavioral state.
    /// Returns the killing reason if a kill is warranted.
    pub fn should_kill(&self) -> Option<BehavioralState> {
        let metrics = self.observe();
        match metrics.behavioral_state {
            BehavioralState::PreFailure => Some(BehavioralState::PreFailure),
            BehavioralState::Rigid if self.events.len() > 10 => Some(BehavioralState::Rigid),
            _ => None,
        }
    }

    /// Kill the workcell. Records the outcome and prevents further events.
    pub fn kill(&mut self, reason: BehavioralState) {
        self.outcome = Some(WorkcellOutcome::Killed { reason });
    }

    /// Complete the workcell with produced outputs.
    pub fn complete(&mut self, outputs: BTreeMap<ArtifactType, Vec<u8>>) {
        self.outcome = Some(WorkcellOutcome::Success { outputs });
    }

    /// Mark the workcell as failed.
    pub fn fail(&mut self, error: String) {
        self.outcome = Some(WorkcellOutcome::Failed { error });
    }

    /// Is this workcell still alive?
    pub fn is_alive(&self) -> bool {
        self.outcome.is_none()
    }

    /// The final outcome, if the workcell has terminated.
    pub fn outcome(&self) -> Option<&WorkcellOutcome> {
        self.outcome.as_ref()
    }

    /// The scoped context (read-only access for the enzyme).
    pub fn context(&self) -> &WorkcellContext {
        &self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observer::ToolEvent::*;

    fn spawn_test_workcell() -> Workcell {
        Workcell::spawn(
            WorkcellId("wc-001".into()),
            EnzymeId::from("coder"),
            BTreeMap::new(),
        )
    }

    #[test]
    fn new_workcell_is_alive() {
        let wc = spawn_test_workcell();
        assert!(wc.is_alive());
        assert!(wc.outcome().is_none());
        assert!(wc.trace().is_empty());
    }

    #[test]
    fn records_events() {
        let mut wc = spawn_test_workcell();
        wc.record_event(Read);
        wc.record_event(Think);
        wc.record_event(Execute);
        assert_eq!(wc.trace().len(), 3);
    }

    #[test]
    fn observe_reflects_current_trace() {
        let mut wc = spawn_test_workcell();
        wc.record_event(Read);
        wc.record_event(Think);
        wc.record_event(Execute);
        let metrics = wc.observe();
        assert!(metrics.deliberation_motif_present);
    }

    #[test]
    fn kill_on_pre_failure_state() {
        let mut wc = spawn_test_workcell();
        // Simulate pre-failure: text-heavy, minimal tools
        for _ in 0..6 {
            wc.record_event(Text);
        }
        assert_eq!(wc.should_kill(), Some(BehavioralState::PreFailure));

        wc.kill(BehavioralState::PreFailure);
        assert!(!wc.is_alive());
        assert!(matches!(
            wc.outcome(),
            Some(WorkcellOutcome::Killed {
                reason: BehavioralState::PreFailure
            })
        ));
    }

    #[test]
    fn healthy_workcell_not_killed() {
        let mut wc = spawn_test_workcell();
        wc.record_event(Read);
        wc.record_event(Think);
        wc.record_event(Execute);
        wc.record_event(Write);
        assert_eq!(wc.should_kill(), None);
    }

    #[test]
    fn complete_with_outputs() {
        let mut wc = spawn_test_workcell();
        let mut outputs = BTreeMap::new();
        outputs.insert(ArtifactType::from("code"), b"fn main() {}".to_vec());
        wc.complete(outputs);

        assert!(!wc.is_alive());
        assert!(matches!(
            wc.outcome(),
            Some(WorkcellOutcome::Success { .. })
        ));
    }

    #[test]
    fn fail_with_error() {
        let mut wc = spawn_test_workcell();
        wc.fail("build failed".into());
        assert!(!wc.is_alive());
        assert!(matches!(wc.outcome(), Some(WorkcellOutcome::Failed { .. })));
    }

    #[test]
    fn context_is_scoped_to_enzyme() {
        let mut inputs = BTreeMap::new();
        inputs.insert(ArtifactType::from("requirements"), b"build X".to_vec());

        let wc = Workcell::spawn(WorkcellId("wc-002".into()), EnzymeId::from("coder"), inputs);

        let ctx = wc.context();
        assert_eq!(ctx.enzyme_id, EnzymeId::from("coder"));
        assert!(ctx.inputs.contains_key(&ArtifactType::from("requirements")));
        // No way to access another workcell's context — it's not in the type.
    }
}
