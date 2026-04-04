//! Sensorium ingestion — converts external signals into quarantined TaskContracts.
//!
//! The sensorium is A²'s interface with the food set: human objectives,
//! tickets, incidents, telemetry. All external input is quarantined with
//! provenance before entering the system as a TaskContract.

use a2_core::id::TaskId;
use a2_core::protocol::*;
use chrono::Utc;

/// Risk tier for ingested evidence. Determines quarantine strictness.
#[derive(Clone, Debug, PartialEq)]
pub enum RiskTier {
    /// Human-provided, trusted origin.
    Low,
    /// Automated source with known provenance (CI, telemetry).
    Medium,
    /// Unknown or untrusted origin.
    High,
}

/// An external signal before it becomes a TaskContract.
#[derive(Clone, Debug)]
pub struct RawSignal {
    pub origin: String,
    pub content: String,
    pub risk_tier: RiskTier,
    pub metadata: Vec<(String, String)>,
}

/// Ingests raw signals and produces quarantined TaskContracts.
pub struct Ingester {
    default_budget: Budget,
}

impl Ingester {
    pub fn new(default_budget: Budget) -> Self {
        Self { default_budget }
    }

    /// Convert a raw signal into a TaskContract.
    /// The signal is quarantined: its content is never directly executed,
    /// only used as context for catalyst prompts.
    pub fn ingest(&self, signal: RawSignal) -> TaskContract {
        let priority = match signal.risk_tier {
            RiskTier::Low => Priority::Normal,
            RiskTier::Medium => Priority::Normal,
            RiskTier::High => Priority::Low, // Untrusted signals get lower priority.
        };

        TaskContract {
            id: TaskId::new(),
            title: truncate(&signal.content, 80),
            description: signal.content.clone(),
            acceptance_criteria: vec!["Task addressed as described".into()],
            budget: self.default_budget.clone(),
            priority,
            source: TaskSource::Sensorium {
                evidence_id: format!(
                    "{}:{}",
                    signal.origin,
                    signal
                        .metadata
                        .iter()
                        .find(|(k, _)| k == "id")
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("unknown")
                ),
            },
            created_at: Utc::now(),
        }
    }

    pub fn ingest_batch(&self, signals: Vec<RawSignal>) -> Vec<TaskContract> {
        signals.into_iter().map(|s| self.ingest(s)).collect()
    }

    /// Ingest a simple human-provided objective (the most common food set input).
    pub fn from_human(&self, title: &str, description: &str) -> TaskContract {
        self.ingest(RawSignal {
            origin: "human".into(),
            content: format!("{title}\n\n{description}"),
            risk_tier: RiskTier::Low,
            metadata: vec![],
        })
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut t = s[..max - 3].to_string();
        t.push_str("...");
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_budget() -> Budget {
        Budget {
            max_tokens: 50_000,
            max_duration_secs: 300,
            max_calls: 20,
        }
    }

    #[test]
    fn human_signal_ingests_as_low_risk() {
        let ingester = Ingester::new(default_budget());
        let task = ingester.from_human(
            "Fix the auth bug",
            "Users can't log in after password reset",
        );

        assert!(matches!(task.priority, Priority::Normal));
        assert!(matches!(task.source, TaskSource::Sensorium { .. }));
        assert!(task.description.contains("password reset"));
    }

    #[test]
    fn high_risk_gets_low_priority() {
        let ingester = Ingester::new(default_budget());
        let task = ingester.ingest(RawSignal {
            origin: "unknown-webhook".into(),
            content: "do something dangerous".into(),
            risk_tier: RiskTier::High,
            metadata: vec![],
        });

        assert!(matches!(task.priority, Priority::Low));
    }

    #[test]
    fn long_content_truncated_in_title() {
        let ingester = Ingester::new(default_budget());
        let long = "a".repeat(200);
        let task = ingester.ingest(RawSignal {
            origin: "test".into(),
            content: long.clone(),
            risk_tier: RiskTier::Low,
            metadata: vec![],
        });

        assert!(task.title.len() <= 80);
        assert!(task.title.ends_with("..."));
        assert_eq!(task.description, long); // Full content preserved in description.
    }
}
