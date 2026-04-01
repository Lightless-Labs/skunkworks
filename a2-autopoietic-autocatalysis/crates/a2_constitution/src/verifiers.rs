use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use a2_core::error::{A2Error, A2Result};
use a2_core::traits::ConstitutionalVerifier;

use crate::Invariant;

pub struct VerifierRegistry {
    verifiers: BTreeMap<Invariant, Vec<Arc<dyn ConstitutionalVerifier>>>,
}

impl Default for VerifierRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl VerifierRegistry {
    pub fn new() -> Self {
        Self {
            verifiers: BTreeMap::new(),
        }
    }

    pub fn with_builtin_verifiers(workspace_root: impl Into<PathBuf>) -> Self {
        let workspace_root = workspace_root.into();
        let mut registry = Self::new();
        registry.register(
            Invariant::Inv1,
            SelfHostVerifier::new(workspace_root.clone()),
        );
        registry.register(Invariant::Inv2, RepairCoverageVerifier);
        registry
    }

    pub fn register<V>(&mut self, invariant: Invariant, verifier: V)
    where
        V: ConstitutionalVerifier + 'static,
    {
        self.register_arc(invariant, Arc::new(verifier));
    }

    pub fn register_arc(
        &mut self,
        invariant: Invariant,
        verifier: Arc<dyn ConstitutionalVerifier>,
    ) {
        self.verifiers.entry(invariant).or_default().push(verifier);
    }

    pub fn resolve(
        &self,
        invariant: Invariant,
        verifier_names: &[String],
    ) -> A2Result<Vec<Arc<dyn ConstitutionalVerifier>>> {
        let Some(registered) = self.verifiers.get(&invariant) else {
            return Ok(Vec::new());
        };

        if verifier_names.is_empty() {
            return Ok(registered.clone());
        }

        let mut selected = Vec::with_capacity(verifier_names.len());
        for name in verifier_names {
            let Some(verifier) = registered
                .iter()
                .find(|candidate| candidate.invariant_name() == name)
            else {
                return Err(A2Error::ConstitutionalViolation {
                    clause: format!("unknown verifier `{name}` for {}", invariant.code()),
                });
            };

            selected.push(Arc::clone(verifier));
        }

        Ok(selected)
    }
}

pub struct SelfHostVerifier {
    workspace_root: PathBuf,
}

impl SelfHostVerifier {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
}

impl ConstitutionalVerifier for SelfHostVerifier {
    fn invariant_name(&self) -> &str {
        "self_host"
    }

    fn verify(&self) -> A2Result<()> {
        let output = Command::new("cargo")
            .args(["check", "--workspace", "--quiet"])
            .current_dir(self.workspace_root())
            .output()?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if stderr.trim().is_empty() {
            format!(
                "`cargo check --workspace --quiet` failed in `{}` with status {} and produced no stderr",
                self.workspace_root().display(),
                output.status
            )
        } else {
            format!(
                "`cargo check --workspace --quiet` failed in `{}` with status {}: {}",
                self.workspace_root().display(),
                output.status,
                stderr.trim()
            )
        };

        Err(A2Error::InvariantViolation {
            invariant: Invariant::Inv1.code().to_string(),
            detail,
        })
    }
}

#[derive(Default)]
pub struct RepairCoverageVerifier;

impl ConstitutionalVerifier for RepairCoverageVerifier {
    fn invariant_name(&self) -> &str {
        "repair_coverage"
    }

    fn verify(&self) -> A2Result<()> {
        Ok(())
    }
}
