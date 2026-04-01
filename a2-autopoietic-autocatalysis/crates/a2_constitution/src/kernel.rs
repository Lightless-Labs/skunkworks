use std::fs;
use std::path::{Path, PathBuf};

use a2_core::error::{A2Error, A2Result};
use a2_core::protocol::BoundaryPolicy;
use serde::{Deserialize, Serialize};

use crate::invariants::{Invariant, InvariantDefinition};
use crate::profile::{BootstrapGate, BootstrapProfile};
use crate::verifiers::VerifierRegistry;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstitutionSpec {
    #[serde(default)]
    pub profile: Option<BootstrapProfile>,
    #[serde(default = "default_invariants")]
    pub invariants: Vec<InvariantDefinition>,
}

impl ConstitutionSpec {
    pub fn load(path: impl AsRef<Path>) -> A2Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)?;
        let extension = path.extension().and_then(|ext| ext.to_str());

        match extension {
            Some("json") => Ok(serde_json::from_str(&raw)?),
            Some("toml") => parse_toml(path, &raw),
            _ => serde_json::from_str(&raw).or_else(|json_err| {
                toml::from_str(&raw).map_err(|toml_err| A2Error::ConstitutionalViolation {
                    clause: format!(
                        "failed to parse constitution spec {} as JSON ({json_err}) or TOML ({toml_err})",
                        path.display()
                    ),
                })
            }),
        }
    }

    pub fn effective_profile(&self, fallback: BootstrapProfile) -> BootstrapProfile {
        self.profile.unwrap_or(fallback)
    }
}

impl Default for ConstitutionSpec {
    fn default() -> Self {
        Self {
            profile: None,
            invariants: default_invariants(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct VerifierReport {
    pub name: String,
    pub passed: bool,
    pub detail: Option<String>,
}

#[derive(Clone, Debug)]
pub struct InvariantReport {
    pub invariant: Invariant,
    pub title: String,
    pub description: String,
    pub passed: bool,
    pub verifiers: Vec<VerifierReport>,
}

#[derive(Clone, Debug)]
pub struct ConstitutionalReport {
    pub profile: BootstrapProfile,
    pub active_gates: Vec<BootstrapGate>,
    pub boundary_policy: BoundaryPolicy,
    pub invariants: Vec<InvariantReport>,
}

impl ConstitutionalReport {
    pub fn all_passed(&self) -> bool {
        self.invariants.iter().all(|report| report.passed)
    }
}

pub struct ConstitutionalKernel {
    profile: BootstrapProfile,
    boundary_policy: BoundaryPolicy,
    spec: ConstitutionSpec,
    registry: VerifierRegistry,
    spec_path: Option<PathBuf>,
}

impl ConstitutionalKernel {
    pub fn new(
        profile: BootstrapProfile,
        boundary_policy: BoundaryPolicy,
        registry: VerifierRegistry,
    ) -> Self {
        Self {
            profile,
            boundary_policy,
            spec: ConstitutionSpec::default(),
            registry,
            spec_path: None,
        }
    }

    pub fn from_spec_path(
        spec_path: impl AsRef<Path>,
        profile: BootstrapProfile,
        boundary_policy: BoundaryPolicy,
        registry: VerifierRegistry,
    ) -> A2Result<Self> {
        let spec_path = spec_path.as_ref().to_path_buf();
        let spec = ConstitutionSpec::load(&spec_path)?;
        let profile = spec.effective_profile(profile);

        Ok(Self {
            profile,
            boundary_policy,
            spec,
            registry,
            spec_path: Some(spec_path),
        })
    }

    pub fn load_spec(&mut self, spec_path: impl AsRef<Path>) -> A2Result<()> {
        let spec_path = spec_path.as_ref().to_path_buf();
        let spec = ConstitutionSpec::load(&spec_path)?;
        self.profile = spec.effective_profile(self.profile);
        self.spec = spec;
        self.spec_path = Some(spec_path);
        Ok(())
    }

    pub fn spec(&self) -> &ConstitutionSpec {
        &self.spec
    }

    pub fn profile(&self) -> BootstrapProfile {
        self.profile
    }

    pub fn spec_path(&self) -> Option<&Path> {
        self.spec_path.as_deref()
    }

    pub fn registry(&self) -> &VerifierRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut VerifierRegistry {
        &mut self.registry
    }

    pub fn run_all(&self) -> ConstitutionalReport {
        let invariants = self
            .spec
            .invariants
            .iter()
            .filter(|definition| definition.enabled)
            .map(|definition| self.run_invariant(definition))
            .collect();

        ConstitutionalReport {
            profile: self.profile,
            active_gates: self.profile.gates().to_vec(),
            boundary_policy: self.boundary_policy.clone(),
            invariants,
        }
    }

    fn run_invariant(&self, definition: &InvariantDefinition) -> InvariantReport {
        let verifiers = match self
            .registry
            .resolve(definition.invariant, &definition.verifiers)
        {
            Ok(verifiers) if verifiers.is_empty() => {
                return missing_verifier_report(definition, "no verifiers registered");
            }
            Ok(verifiers) => verifiers,
            Err(error) => {
                return failed_invariant_report(
                    definition,
                    "<registry>",
                    format!("failed to resolve verifiers: {error}"),
                );
            }
        };

        let verifier_reports = verifiers
            .into_iter()
            .map(|verifier| {
                let name = verifier.invariant_name().to_string();
                match verifier.verify() {
                    Ok(()) => VerifierReport {
                        name,
                        passed: true,
                        detail: None,
                    },
                    Err(error) => VerifierReport {
                        name,
                        passed: false,
                        detail: Some(error.to_string()),
                    },
                }
            })
            .collect::<Vec<_>>();

        let passed = verifier_reports.iter().all(|report| report.passed);

        InvariantReport {
            invariant: definition.invariant,
            title: definition.resolved_title().to_string(),
            description: definition.resolved_description().to_string(),
            passed,
            verifiers: verifier_reports,
        }
    }
}

fn default_invariants() -> Vec<InvariantDefinition> {
    Invariant::ALL
        .into_iter()
        .map(InvariantDefinition::new)
        .collect()
}

fn parse_toml(path: &Path, raw: &str) -> A2Result<ConstitutionSpec> {
    toml::from_str(raw).map_err(|error| A2Error::ConstitutionalViolation {
        clause: format!(
            "failed to parse TOML constitution spec {}: {error}",
            path.display()
        ),
    })
}

fn missing_verifier_report(
    definition: &InvariantDefinition,
    detail: impl Into<String>,
) -> InvariantReport {
    failed_invariant_report(definition, "<none>", detail)
}

fn failed_invariant_report(
    definition: &InvariantDefinition,
    verifier_name: impl Into<String>,
    detail: impl Into<String>,
) -> InvariantReport {
    InvariantReport {
        invariant: definition.invariant,
        title: definition.resolved_title().to_string(),
        description: definition.resolved_description().to_string(),
        passed: false,
        verifiers: vec![VerifierReport {
            name: verifier_name.into(),
            passed: false,
            detail: Some(detail.into()),
        }],
    }
}
