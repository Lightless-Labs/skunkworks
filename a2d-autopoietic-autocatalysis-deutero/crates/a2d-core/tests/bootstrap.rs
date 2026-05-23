//! Bootstrap integration test: proves the Stage 0 seed works as a system.
//!
//! Success criteria (from bootstrap plan):
//! 1. Three enzymes form a verified RAF (closure ratio = 1.0)
//! 2. The observer reports health from mechanical signals only
//! 3. A mutation that breaks closure is mechanically rejected
//! 4. A mutation that improves fitness is mechanically accepted
//! 5. The germline contains the enzyme definitions that produced it

use a2d_core::germline::Germline;
use a2d_core::observer::{self, BehavioralState, ToolEvent};
use a2d_core::types::{ArtifactType, EnzymeDef, EnzymeId};
use a2d_core::workcell::{Workcell, WorkcellId, WorkcellOutcome};
use std::collections::{BTreeMap, BTreeSet};

fn art(s: &str) -> ArtifactType {
    ArtifactType::from(s)
}

fn enzyme(id: &str, reactants: &[&str], products: &[&str], catalysts: &[&str]) -> EnzymeDef {
    EnzymeDef {
        id: EnzymeId::from(id),
        reactants: reactants.iter().map(|&s| art(s)).collect(),
        products: products.iter().map(|&s| art(s)).collect(),
        catalysts: catalysts.iter().map(|&s| art(s)).collect(),
        ..Default::default()
    }
}

fn food(items: &[&str]) -> BTreeSet<ArtifactType> {
    items.iter().map(|&s| art(s)).collect()
}

/// The minimal irrRAF: Coder → Tester → Evolver → Coder
fn seed_enzymes() -> Vec<EnzymeDef> {
    vec![
        enzyme("coder", &["requirements"], &["code"], &["enzyme_defs"]),
        enzyme("tester", &["code"], &["test_results"], &["code"]),
        enzyme(
            "evolver",
            &["test_results"],
            &["enzyme_defs"],
            &["test_results"],
        ),
    ]
}

// ── Criterion 1: Three enzymes form a verified RAF ───────────────────

#[test]
fn criterion_1_minimal_cycle_achieves_closure() {
    let germline = Germline::new(seed_enzymes(), food(&["requirements"]));
    let status = germline.raf_status();

    assert!(
        status.is_closed(),
        "Seed must achieve full catalytic closure. Coverage: {}, Orphans: {:?}",
        status.coverage,
        status.orphans
    );
    assert_eq!(status.max_raf.len(), 3);
}

// ── Criterion 2: Observer reports health mechanically ────────────────

#[test]
fn criterion_2_observer_reports_numerical_metrics() {
    // Simulate a healthy enzyme execution trace
    let events = vec![
        ToolEvent::Read,
        ToolEvent::Think,
        ToolEvent::Execute,
        ToolEvent::Read,
        ToolEvent::Think,
        ToolEvent::Execute,
        ToolEvent::Write,
    ];

    let metrics = observer::observe(&events);

    // Observer produces structured data, not narrative
    assert_eq!(metrics.behavioral_state, BehavioralState::Deliberating);
    assert!(metrics.deliberation_motif_present);
    assert_eq!(metrics.deliberation_motif_count, 2);
    assert!(metrics.entropy_rate > 0.0);
    assert_eq!(metrics.window_size, 7);
    // tool_to_text_ratio is infinity (no text events) — all mechanical
    assert!(metrics.tool_to_text_ratio.is_infinite());
}

#[test]
fn criterion_2_observer_detects_pre_failure() {
    // Simulate degrading enzyme: text-heavy, no tools
    let events = vec![
        ToolEvent::Text,
        ToolEvent::Text,
        ToolEvent::Text,
        ToolEvent::Text,
        ToolEvent::Text,
        ToolEvent::Text,
    ];

    let metrics = observer::observe(&events);
    assert_eq!(metrics.behavioral_state, BehavioralState::PreFailure);
}

// ── Criterion 3: Closure-breaking mutation is rejected ───────────────

#[test]
fn criterion_3_removing_critical_enzyme_rejected() {
    let mut germline = Germline::new(seed_enzymes(), food(&["requirements"]));

    // Try to remove each enzyme in the cycle — all should fail
    for id in &["coder", "tester", "evolver"] {
        let result = germline.propose_remove(&EnzymeId::from(*id));
        assert!(
            result.is_err(),
            "Removing '{}' should be rejected (would break closure)",
            id
        );
    }

    // Germline should still be intact
    assert!(germline.raf_status().is_closed());
}

#[test]
fn criterion_3_replacing_with_broken_enzyme_rejected() {
    let mut germline = Germline::new(seed_enzymes(), food(&["requirements"]));

    // Replace evolver with one that doesn't produce enzyme_defs
    let broken = enzyme(
        "evolver",
        &["test_results"],
        &["garbage"],
        &["test_results"],
    );
    let result = germline.propose_replace(broken);
    assert!(
        result.is_err(),
        "Replacing evolver with closure-breaking version should be rejected"
    );

    // Original should be preserved
    let evolver = germline.get_enzyme(&EnzymeId::from("evolver")).unwrap();
    assert!(evolver.products.contains(&art("enzyme_defs")));
}

// ── Criterion 4: Fitness-improving mutation is accepted ──────────────

#[test]
fn criterion_4_adding_compatible_enzyme_accepted() {
    let mut germline = Germline::new(seed_enzymes(), food(&["requirements"]));

    // Add a doc generator that's catalyzed by code (already produced)
    let doc_gen = enzyme("doc_gen", &["code"], &["docs"], &["code"]);
    let result = germline.propose_add(doc_gen);
    assert!(result.is_ok(), "Adding compatible enzyme should succeed");
    assert_eq!(germline.enzymes().len(), 4);

    // Closure still holds
    assert!(germline.raf_status().is_closed());
}

#[test]
fn criterion_4_improving_enzyme_accepted() {
    let mut germline = Germline::new(seed_enzymes(), food(&["requirements"]));

    // Improve coder to also produce docs
    let better_coder = enzyme(
        "coder",
        &["requirements"],
        &["code", "docs"],
        &["enzyme_defs"],
    );
    let result = germline.propose_replace(better_coder);
    assert!(result.is_ok(), "Improving enzyme should succeed");

    let coder = germline.get_enzyme(&EnzymeId::from("coder")).unwrap();
    assert!(coder.products.contains(&art("docs")));
    assert!(germline.raf_status().is_closed());
}

// ── Criterion 5: Germline contains self-description ──────────────────

#[test]
fn criterion_5_germline_describes_its_own_enzymes() {
    let germline = Germline::new(seed_enzymes(), food(&["requirements"]));

    // The germline knows what enzymes it contains
    let enzyme_ids: BTreeSet<EnzymeId> = germline.enzymes().iter().map(|e| e.id.clone()).collect();

    assert!(enzyme_ids.contains(&EnzymeId::from("coder")));
    assert!(enzyme_ids.contains(&EnzymeId::from("tester")));
    assert!(enzyme_ids.contains(&EnzymeId::from("evolver")));

    // The evolver produces enzyme_defs — the germline's own content type
    let evolver = germline.get_enzyme(&EnzymeId::from("evolver")).unwrap();
    assert!(
        evolver.products.contains(&art("enzyme_defs")),
        "Evolver must produce the germline's own content type"
    );

    // The coder is catalyzed by enzyme_defs — the germline's own content
    let coder = germline.get_enzyme(&EnzymeId::from("coder")).unwrap();
    assert!(
        coder.catalysts.contains(&art("enzyme_defs")),
        "Coder must be catalyzed by the germline's own content"
    );
}

// ── End-to-end: workcell lifecycle with observer-driven kill ─────────

#[test]
fn e2e_workcell_healthy_execution_completes() {
    let mut wc = Workcell::spawn(
        WorkcellId("wc-e2e-1".into()),
        EnzymeId::from("coder"),
        BTreeMap::new(),
    );

    // Simulate healthy execution
    wc.record_event(ToolEvent::Read);
    wc.record_event(ToolEvent::Think);
    wc.record_event(ToolEvent::Execute);
    wc.record_event(ToolEvent::Write);

    // Observer says: healthy
    assert_eq!(wc.should_kill(), None);

    // Complete normally
    let mut outputs = BTreeMap::new();
    outputs.insert(art("code"), b"fn main() {}".to_vec());
    wc.complete(outputs);

    assert!(!wc.is_alive());
    assert!(matches!(
        wc.outcome(),
        Some(WorkcellOutcome::Success { .. })
    ));
}

#[test]
fn e2e_workcell_killed_on_degradation() {
    let mut wc = Workcell::spawn(
        WorkcellId("wc-e2e-2".into()),
        EnzymeId::from("coder"),
        BTreeMap::new(),
    );

    // Simulate degradation: all text, no tools
    for _ in 0..8 {
        wc.record_event(ToolEvent::Text);
    }

    // Observer detects pre-failure
    let kill_reason = wc.should_kill();
    assert_eq!(kill_reason, Some(BehavioralState::PreFailure));

    // Kill it
    wc.kill(BehavioralState::PreFailure);
    assert!(!wc.is_alive());
    assert!(matches!(
        wc.outcome(),
        Some(WorkcellOutcome::Killed {
            reason: BehavioralState::PreFailure
        })
    ));
}

// ── Full cycle: germline mutation through workcell observation ────────

#[test]
fn e2e_full_cycle_mutate_observe_gate() {
    let mut germline = Germline::new(seed_enzymes(), food(&["requirements"]));

    // Step 1: Verify initial closure
    assert!(germline.raf_status().is_closed());

    // Step 2: Simulate workcell for the evolver producing a new enzyme def
    let mut evolver_wc = Workcell::spawn(
        WorkcellId("wc-evolver-1".into()),
        EnzymeId::from("evolver"),
        BTreeMap::new(),
    );
    evolver_wc.record_event(ToolEvent::Read);
    evolver_wc.record_event(ToolEvent::Think);
    evolver_wc.record_event(ToolEvent::Execute);

    // Evolver is healthy
    assert_eq!(evolver_wc.should_kill(), None);
    assert!(evolver_wc.observe().deliberation_motif_present);

    // Step 3: Evolver proposes a mutation — improve coder
    let improved_coder = enzyme(
        "coder",
        &["requirements"],
        &["code", "docs"],
        &["enzyme_defs"],
    );

    // Step 4: Germline gates the mutation mechanically
    let result = germline.propose_replace(improved_coder);
    assert!(result.is_ok(), "Improving coder should pass the RAF gate");

    // Step 5: Verify closure maintained after mutation
    let status = germline.raf_status();
    assert!(status.is_closed());
    assert_eq!(status.max_raf.len(), 3);

    // Step 6: Complete the evolver workcell
    let mut outputs = BTreeMap::new();
    outputs.insert(art("enzyme_defs"), b"improved coder def".to_vec());
    evolver_wc.complete(outputs);
    assert!(!evolver_wc.is_alive());
}
