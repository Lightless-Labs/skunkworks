use a2_core::protocol::{BoundaryPolicy, CapabilityMap, HardShell, NetworkPolicy};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum BootstrapProfile {
    #[serde(rename = "B0")]
    #[default]
    B0,
    #[serde(rename = "B1")]
    B1,
    #[serde(rename = "B2")]
    B2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum BootstrapGate {
    #[serde(rename = "automated-verifier-gates")]
    AutomatedVerifierGates,
    #[serde(rename = "human-review")]
    HumanReview,
    #[serde(rename = "constitutional-patch-queue")]
    ConstitutionalPatchQueue,
    #[serde(rename = "attested-kernel-patches")]
    AttestedKernelPatches,
    #[serde(rename = "external-sentinel-approval")]
    ExternalSentinelApproval,
    #[serde(rename = "structural-coupling")]
    StructuralCoupling,
    #[serde(rename = "kernel-regeneration-under-attestation")]
    KernelRegenerationUnderAttestation,
    #[serde(rename = "root-of-trust-escrow")]
    RootOfTrustEscrow,
    #[serde(rename = "rollback-authority")]
    RollbackAuthority,
}

impl BootstrapProfile {
    pub fn gates(self) -> &'static [BootstrapGate] {
        use BootstrapGate::{
            AttestedKernelPatches, AutomatedVerifierGates, ConstitutionalPatchQueue,
            ExternalSentinelApproval, HumanReview, KernelRegenerationUnderAttestation,
            RollbackAuthority, RootOfTrustEscrow, StructuralCoupling,
        };

        match self {
            Self::B0 => &[
                AutomatedVerifierGates,
                HumanReview,
                ConstitutionalPatchQueue,
                RootOfTrustEscrow,
                RollbackAuthority,
            ],
            Self::B1 => &[
                AutomatedVerifierGates,
                AttestedKernelPatches,
                ExternalSentinelApproval,
                RootOfTrustEscrow,
                RollbackAuthority,
            ],
            Self::B2 => &[
                AutomatedVerifierGates,
                AttestedKernelPatches,
                ExternalSentinelApproval,
                StructuralCoupling,
                KernelRegenerationUnderAttestation,
                RootOfTrustEscrow,
                RollbackAuthority,
            ],
        }
    }

    pub fn human_review_required(self) -> bool {
        matches!(self, Self::B0)
    }

    pub fn boundary_policy(self, hard_shell: HardShell) -> BoundaryPolicy {
        let soft_membrane = match self {
            Self::B0 => CapabilityMap {
                allowed_tools: vec![
                    "build".to_string(),
                    "test".to_string(),
                    "lineage-read".to_string(),
                ],
                denied_tools: vec![
                    "root-of-trust-write".to_string(),
                    "production-write".to_string(),
                ],
                secret_scopes: vec!["approval-keys".to_string()],
                network_policy: NetworkPolicy::Isolated,
            },
            Self::B1 => CapabilityMap {
                allowed_tools: vec![
                    "build".to_string(),
                    "test".to_string(),
                    "lineage-read".to_string(),
                    "constitutional-patch".to_string(),
                ],
                denied_tools: vec![
                    "root-of-trust-write".to_string(),
                    "production-write".to_string(),
                ],
                secret_scopes: vec!["approval-keys".to_string(), "benchmark-escrow".to_string()],
                network_policy: NetworkPolicy::Isolated,
            },
            Self::B2 => CapabilityMap {
                allowed_tools: vec![
                    "build".to_string(),
                    "test".to_string(),
                    "lineage-read".to_string(),
                    "constitutional-patch".to_string(),
                    "sensorium-contract".to_string(),
                ],
                denied_tools: vec!["root-of-trust-write".to_string()],
                secret_scopes: vec!["approval-keys".to_string(), "benchmark-escrow".to_string()],
                network_policy: NetworkPolicy::AllowList(vec![
                    "quarantine://sensorium".to_string(),
                    "lineage://archive".to_string(),
                ]),
            },
        };

        BoundaryPolicy {
            hard_shell,
            soft_membrane,
        }
    }
}
