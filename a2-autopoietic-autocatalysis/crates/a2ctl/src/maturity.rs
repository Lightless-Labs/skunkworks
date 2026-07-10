use serde::Serialize;

/// Operator-visible maturity states from the remediation roadmap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MaturityStatus {
    PreStage0,
    Stage0GatePassed,
    Stage1B0,
    Stage2B0,
}

impl MaturityStatus {
    pub const ALL: [Self; 4] = [
        Self::PreStage0,
        Self::Stage0GatePassed,
        Self::Stage1B0,
        Self::Stage2B0,
    ];

    /// Maturity is advanced only by changing this value after the corresponding
    /// roadmap gate has produced independently verified, current-HEAD evidence.
    pub const fn current() -> Self {
        Self::PreStage0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MissingEvidence {
    pub id: &'static str,
    pub requirement: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MaturityReport {
    pub maturity: MaturityStatus,
    pub bootstrap_profile: &'static str,
    pub live_germline_mutation_enabled: bool,
    pub developer_health_checks: &'static str,
    pub maturity_transitions: [MaturityStatus; 4],
    pub missing_evidence: Vec<MissingEvidence>,
}

impl MaturityReport {
    pub fn current() -> Self {
        Self {
            maturity: MaturityStatus::current(),
            bootstrap_profile: "B0 (human approval required)",
            live_germline_mutation_enabled: false,
            developer_health_checks: "the legacy `sentinel` command runs six public developer health checks; it is not the Stage-0 gate or hidden escrow",
            maturity_transitions: MaturityStatus::ALL,
            missing_evidence: vec![
                MissingEvidence {
                    id: "bazel",
                    requirement: "canonical Bazel workspace, crate target parity, and clean-checkout Stage-0 suite",
                },
                MissingEvidence {
                    id: "mission",
                    requirement: "frozen public mission battery and category floors",
                },
                MissingEvidence {
                    id: "escrow",
                    requirement: "operator-mounted root-of-trust and hidden-sentinel escrow validation",
                },
                MissingEvidence {
                    id: "promotion",
                    requirement: "B0-approved atomic Git admission, durable journal, descendant inheritance, and rollback",
                },
                MissingEvidence {
                    id: "constitution",
                    requirement: "digest-checked constitutional semantics with non-vacuous executable invariants",
                },
                MissingEvidence {
                    id: "membrane",
                    requirement: "end-to-end capability and ingress boundary enforcement",
                },
                MissingEvidence {
                    id: "external_value",
                    requirement: "independently verified sealed external-value campaign",
                },
            ],
        }
    }
}

pub fn live_germline_mutation_block_reason() -> &'static str {
    "live germline mutation is disabled at PreStage0; `--apply` cannot be used until the Phase-4 Stage-0 gate and Phase-5 B0-approved end-to-end admission trace both pass"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_report_is_explicitly_pre_stage_zero_and_fail_closed() {
        let report = MaturityReport::current();
        assert_eq!(report.maturity, MaturityStatus::PreStage0);
        assert!(!report.live_germline_mutation_enabled);
        assert_eq!(
            report
                .missing_evidence
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![
                "bazel",
                "mission",
                "escrow",
                "promotion",
                "constitution",
                "membrane",
                "external_value",
            ]
        );
    }

    #[test]
    fn maturity_report_json_uses_authoritative_state_names() {
        let value = serde_json::to_value(MaturityReport::current()).unwrap();
        assert_eq!(value["maturity"], "PreStage0");
        assert_eq!(
            value["maturity_transitions"],
            serde_json::json!(["PreStage0", "Stage0GatePassed", "Stage1B0", "Stage2B0"])
        );
    }
}
