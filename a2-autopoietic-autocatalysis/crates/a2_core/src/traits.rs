//! Core traits that define the A² component contracts.
//!
//! These are the interfaces that make the system composable.
//! Each trait maps to a role in the architecture.

use async_trait::async_trait;

use crate::error::A2Result;
use crate::id::*;
use crate::protocol::*;

/// A model provider that can generate text given messages.
/// Mirrors tundish_core::ModelProvider from converge-refinery.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> A2Result<GenerateResponse>;
    fn provider_id(&self) -> &str;
    fn model_id(&self) -> &str;
}

#[derive(Clone, Debug)]
pub struct GenerateResponse {
    pub text: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

/// A catalyst transforms a TaskContract + ContextPack into a PatchBundle.
/// This is the core unit of work in A².
#[async_trait]
pub trait Catalyst: Send + Sync {
    fn id(&self) -> &CatalystId;
    fn name(&self) -> &str;

    async fn execute(
        &self,
        task: &TaskContract,
        context: &ContextPack,
        model: &dyn ModelProvider,
    ) -> A2Result<PatchBundle>;
}

/// Evaluates a PatchBundle and produces a FitnessRecord.
#[async_trait]
pub trait Evaluator: Send + Sync {
    async fn evaluate(
        &self,
        patch: &PatchBundle,
        task: &TaskContract,
    ) -> A2Result<FitnessRecord>;
}

/// Decides whether a patch should be promoted to germline.
#[async_trait]
pub trait Promoter: Send + Sync {
    async fn decide(
        &self,
        patch: &PatchBundle,
        fitness: &FitnessRecord,
        lineage: &[LineageRecord],
    ) -> A2Result<PromotionDecision>;
}

/// Checks whether an action is allowed by the membrane.
pub trait Membrane: Send + Sync {
    fn check_tool(&self, tool_name: &str, workcell: &WorkcellId) -> A2Result<()>;
    fn check_network(&self, endpoint: &str, workcell: &WorkcellId) -> A2Result<()>;
    fn capability_map(&self, workcell: &WorkcellId) -> CapabilityMap;
}

/// Verifies a constitutional invariant. Returns Ok(()) if the invariant holds.
pub trait ConstitutionalVerifier: Send + Sync {
    fn invariant_name(&self) -> &str;
    fn verify(&self) -> A2Result<()>;
}

/// Stores and retrieves lineage records.
#[async_trait]
pub trait LineageStore: Send + Sync {
    async fn record(&self, entry: LineageRecord) -> A2Result<()>;
    async fn get(&self, id: &LineageId) -> A2Result<Option<LineageRecord>>;
    async fn for_task(&self, task_id: &TaskId) -> A2Result<Vec<LineageRecord>>;
    async fn recent(&self, limit: usize) -> A2Result<Vec<LineageRecord>>;
}

/// Stores and retrieves promotion journal entries.
#[async_trait]
pub trait PromotionJournal: Send + Sync {
    async fn append(&self, entry: PromotionJournalEntry) -> A2Result<()>;
    async fn latest(&self) -> A2Result<Option<PromotionJournalEntry>>;
    async fn history(&self, limit: usize) -> A2Result<Vec<PromotionJournalEntry>>;
}
