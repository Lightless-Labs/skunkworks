use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// Unique identifier for an enzyme in the catalytic network.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EnzymeId(pub String);

impl fmt::Display for EnzymeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for EnzymeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// A type of artifact that can be produced, consumed, or catalyze reactions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ArtifactType(pub String);

impl From<&str> for ArtifactType {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// An enzyme definition in the catalytic reaction system.
///
/// Each enzyme transforms reactants into products, catalyzed by catalysts.
/// The prompt_template is the behavioral definition — it tells the enzyme
/// HOW to do its job. The evolver can modify this to change enzyme behavior
/// without changing the graph topology.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnzymeDef {
    pub id: EnzymeId,
    /// Artifact types consumed by this enzyme.
    pub reactants: BTreeSet<ArtifactType>,
    /// Artifact types produced by this enzyme.
    pub products: BTreeSet<ArtifactType>,
    /// Artifact types that enable/accelerate this enzyme (not consumed).
    pub catalysts: BTreeSet<ArtifactType>,
    /// System prompt template for this enzyme. The evolver can modify this
    /// to change enzyme behavior. If None, the metabolism uses a default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_template: Option<String>,
}

/// The food set: exogenous resources available from outside the system.
pub type FoodSet = BTreeSet<ArtifactType>;

/// Result of RAF detection: the maximal RAF and diagnostic info.
#[derive(Debug, Clone)]
pub struct RafResult {
    /// Enzymes in the maximal RAF (self-sustaining subset).
    pub max_raf: BTreeSet<EnzymeId>,
    /// Enzymes not in any RAF (orphans).
    pub orphans: BTreeSet<EnzymeId>,
    /// Coverage ratio: |maxRAF| / |total enzymes|. 1.0 = full closure.
    pub coverage: f64,
    /// Number of pruning iterations to reach fixed point.
    pub iterations: usize,
}

impl RafResult {
    /// Is the system fully catalytically closed?
    pub fn is_closed(&self) -> bool {
        (self.coverage - 1.0).abs() < f64::EPSILON && !self.max_raf.is_empty()
    }
}
