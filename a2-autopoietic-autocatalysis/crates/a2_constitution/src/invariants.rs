use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Invariant {
    #[serde(rename = "INV-1")]
    Inv1,
    #[serde(rename = "INV-2")]
    Inv2,
    #[serde(rename = "INV-3")]
    Inv3,
    #[serde(rename = "INV-4")]
    Inv4,
    #[serde(rename = "INV-5")]
    Inv5,
}

impl Invariant {
    pub const ALL: [Self; 5] = [Self::Inv1, Self::Inv2, Self::Inv3, Self::Inv4, Self::Inv5];

    pub fn code(self) -> &'static str {
        match self {
            Self::Inv1 => "INV-1",
            Self::Inv2 => "INV-2",
            Self::Inv3 => "INV-3",
            Self::Inv4 => "INV-4",
            Self::Inv5 => "INV-5",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Inv1 => "Self-Hosting",
            Self::Inv2 => "Constitutive Repair Coverage",
            Self::Inv3 => "Evaluation Integrity",
            Self::Inv4 => "Lineage and Provenance",
            Self::Inv5 => "Boundary Integrity",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Inv1 => {
                "The system can instantiate a valid descendant workcell from the current germline."
            }
            Self::Inv2 => {
                "Every constitutive kernel component has an independent repair path and a tested rollback path."
            }
            Self::Inv3 => {
                "Stage-appropriate sentinels and evaluator meta-checks remain intact, and no verifier approves itself."
            }
            Self::Inv4 => {
                "Every heritable change is fully recorded with viable rollback to prior germline states."
            }
            Self::Inv5 => {
                "The membrane preserves the trusted exterior boundary and quarantines untrusted ingress."
            }
        }
    }
}

impl fmt::Display for Invariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InvariantDefinition {
    pub invariant: Invariant,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub verifiers: Vec<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl InvariantDefinition {
    pub fn new(invariant: Invariant) -> Self {
        Self {
            invariant,
            title: Some(invariant.title().to_string()),
            description: Some(invariant.description().to_string()),
            verifiers: Vec::new(),
            enabled: true,
        }
    }

    pub fn resolved_title(&self) -> &str {
        self.title
            .as_deref()
            .unwrap_or_else(|| self.invariant.title())
    }

    pub fn resolved_description(&self) -> &str {
        self.description
            .as_deref()
            .unwrap_or_else(|| self.invariant.description())
    }
}

impl Default for InvariantDefinition {
    fn default() -> Self {
        Self::new(Invariant::Inv1)
    }
}

fn default_enabled() -> bool {
    true
}
