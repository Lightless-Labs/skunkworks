//! Structural Observer: mechanical health monitoring for the catalytic network.
//!
//! The observer reads tool call patterns, build outcomes, and behavioral
//! signatures. It produces numerical health metrics — never natural language
//! summaries, never self-assessments. Numbers only.
//!
//! Design rationale (from collision synthesis §I.1):
//! Agents suppress 85.3% of identified risks from their output. The cortex
//! cannot be an agent summarizing system state. It must be a structural
//! observer extracting signals mechanically.

use serde::{Deserialize, Serialize};

/// A single tool call event in an enzyme's execution trace.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolEvent {
    /// Read a file or artifact
    Read,
    /// Think / reason (thinking block)
    Think,
    /// Execute a command (bash, build, test)
    Execute,
    /// Write / edit an artifact
    Write,
    /// Agent text output (no tool use)
    Text,
}

/// Behavioral state classification.
///
/// Based on Third Thoughts' HSMM analysis:
/// - State 3 (pre-failure): 24.6x hazard lift, minimal tool usage,
///   increased text length, very low self-persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BehavioralState {
    /// Normal operation: balanced tool use and text
    Healthy,
    /// Deliberative: strong R-Ak-Tb motif (Read→Think→Execute cycles)
    Deliberating,
    /// Rigid: perseverative loops (same tool repeated). 3.7:1 vs chaotic.
    Rigid,
    /// Pre-failure: minimal tool use, text-heavy, about to fail.
    /// Kill signal should be emitted.
    PreFailure,
    /// Chaotic: high entropy, no coherent pattern.
    Chaotic,
}

/// Health metrics produced by the observer. Numbers only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    pub behavioral_state: BehavioralState,
    /// Shannon entropy of tool call sequence (bits). Lower = more predictable.
    /// Baseline from Third Thoughts: H ≈ 0.7153 bits (69% predictability).
    pub entropy_rate: f64,
    /// Whether the R-Ak-Tb deliberation motif is present.
    /// 16.49x OR in successful sessions (Third Thoughts).
    pub deliberation_motif_present: bool,
    /// Count of R-Ak-Tb motif occurrences in the window.
    pub deliberation_motif_count: usize,
    /// Ratio of tool-using turns to text-only turns.
    pub tool_to_text_ratio: f64,
    /// Number of events in the observation window.
    pub window_size: usize,
}

/// Observe a sequence of tool events and produce health metrics.
pub fn observe(events: &[ToolEvent]) -> HealthMetrics {
    if events.is_empty() {
        return HealthMetrics {
            behavioral_state: BehavioralState::Healthy,
            entropy_rate: 0.0,
            deliberation_motif_present: false,
            deliberation_motif_count: 0,
            tool_to_text_ratio: 0.0,
            window_size: 0,
        };
    }

    let entropy = compute_entropy(events);
    let motif_count = count_deliberation_motifs(events);
    let tool_text_ratio = compute_tool_text_ratio(events);
    let state = classify_state(events, entropy, motif_count, tool_text_ratio);

    HealthMetrics {
        behavioral_state: state,
        entropy_rate: entropy,
        deliberation_motif_present: motif_count > 0,
        deliberation_motif_count: motif_count,
        tool_to_text_ratio: tool_text_ratio,
        window_size: events.len(),
    }
}

/// Shannon entropy of bigram transitions in the tool sequence.
fn compute_entropy(events: &[ToolEvent]) -> f64 {
    if events.len() < 2 {
        return 0.0;
    }

    use std::collections::HashMap;

    // Count bigram transitions
    let mut transition_counts: HashMap<(&ToolEvent, &ToolEvent), usize> = HashMap::new();
    let mut prefix_counts: HashMap<&ToolEvent, usize> = HashMap::new();

    for window in events.windows(2) {
        *transition_counts
            .entry((&window[0], &window[1]))
            .or_default() += 1;
        *prefix_counts.entry(&window[0]).or_default() += 1;
    }

    // Conditional entropy H(X_n | X_{n-1})
    let total_transitions = events.len() - 1;
    let mut entropy = 0.0;

    for ((_from, _to), &count) in &transition_counts {
        let prefix_count = prefix_counts[_from];
        let p_transition = count as f64 / total_transitions as f64;
        let p_conditional = count as f64 / prefix_count as f64;

        if p_conditional > 0.0 {
            entropy -= p_transition * p_conditional.log2();
        }
    }

    entropy
}

/// Count occurrences of the R-Ak-Tb (Read→Think→Execute) deliberation motif.
///
/// This motif family is enriched 11-16x in successful sessions (Third Thoughts).
fn count_deliberation_motifs(events: &[ToolEvent]) -> usize {
    if events.len() < 3 {
        return 0;
    }

    let mut count = 0;
    for window in events.windows(3) {
        if matches!(
            window,
            [ToolEvent::Read, ToolEvent::Think, ToolEvent::Execute]
        ) {
            count += 1;
        }
    }
    count
}

/// Ratio of tool-using events to text-only events.
fn compute_tool_text_ratio(events: &[ToolEvent]) -> f64 {
    let text_count = events.iter().filter(|e| **e == ToolEvent::Text).count();
    let tool_count = events.len() - text_count;

    if text_count == 0 {
        return f64::INFINITY;
    }

    tool_count as f64 / text_count as f64
}

/// Classify behavioral state from observed signals.
fn classify_state(
    events: &[ToolEvent],
    entropy: f64,
    motif_count: usize,
    tool_text_ratio: f64,
) -> BehavioralState {
    // Deliberating takes priority: R-Ak-Tb motifs are the strongest
    // positive signal (16.49x OR in successful sessions).
    if motif_count > 0 {
        return BehavioralState::Deliberating;
    }

    // Pre-failure: mostly text, minimal tool use (HSMM State 3)
    // Threshold: tool/text ratio <= 0.25 with enough events to judge
    if events.len() >= 5 && tool_text_ratio <= 0.25 {
        return BehavioralState::PreFailure;
    }

    // Rigid: very low entropy (perseverative loops)
    // Threshold: H < 0.3 bits with enough events
    if events.len() >= 5 && entropy < 0.3 {
        return BehavioralState::Rigid;
    }

    // Chaotic: very high entropy
    // Threshold: H > 1.5 bits (well above baseline 0.7153)
    if entropy > 1.5 {
        return BehavioralState::Chaotic;
    }

    BehavioralState::Healthy
}

#[cfg(test)]
mod tests {
    use super::*;
    use ToolEvent::*;

    #[test]
    fn empty_events_yields_healthy_defaults() {
        let m = observe(&[]);
        assert_eq!(m.behavioral_state, BehavioralState::Healthy);
        assert_eq!(m.entropy_rate, 0.0);
        assert!(!m.deliberation_motif_present);
        assert_eq!(m.window_size, 0);
    }

    #[test]
    fn deliberation_motif_detected() {
        let events = vec![Read, Think, Execute];
        let m = observe(&events);
        assert!(m.deliberation_motif_present);
        assert_eq!(m.deliberation_motif_count, 1);
    }

    #[test]
    fn multiple_deliberation_motifs() {
        let events = vec![Read, Think, Execute, Read, Think, Execute];
        let m = observe(&events);
        assert_eq!(m.deliberation_motif_count, 2);
        assert_eq!(m.behavioral_state, BehavioralState::Deliberating);
    }

    #[test]
    fn pre_failure_state_on_text_heavy_sequence() {
        // Mostly text, minimal tools — HSMM State 3
        let events = vec![Text, Text, Text, Text, Text, Read];
        let m = observe(&events);
        assert_eq!(m.behavioral_state, BehavioralState::PreFailure);
    }

    #[test]
    fn rigid_state_on_repetitive_sequence() {
        // Same tool over and over — perseverative loop
        let events = vec![Execute, Execute, Execute, Execute, Execute, Execute];
        let m = observe(&events);
        assert_eq!(m.behavioral_state, BehavioralState::Rigid);
    }

    #[test]
    fn entropy_is_zero_for_constant_sequence() {
        let events = vec![Read, Read, Read, Read];
        let m = observe(&events);
        assert_eq!(m.entropy_rate, 0.0);
    }

    #[test]
    fn entropy_is_positive_for_varied_sequence() {
        let events = vec![Read, Think, Execute, Write, Text, Read, Execute];
        let m = observe(&events);
        assert!(m.entropy_rate > 0.0);
    }

    #[test]
    fn tool_text_ratio_correct() {
        // 3 tool events, 2 text events
        let events = vec![Read, Text, Execute, Text, Write];
        let m = observe(&events);
        assert!((m.tool_to_text_ratio - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn all_text_has_zero_tool_ratio() {
        let events = vec![Text, Text, Text];
        let m = observe(&events);
        assert_eq!(m.tool_to_text_ratio, 0.0);
    }

    #[test]
    fn healthy_state_on_balanced_sequence() {
        // Balanced but no specific motif
        let events = vec![Read, Write, Execute, Read];
        let m = observe(&events);
        assert_eq!(m.behavioral_state, BehavioralState::Healthy);
    }
}
