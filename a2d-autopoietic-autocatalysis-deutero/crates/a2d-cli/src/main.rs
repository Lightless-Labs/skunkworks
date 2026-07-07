//! A²D CLI: run the catalytic cycle.
//!
//! Usage:
//!   a2d cycle              Run one catalytic cycle
//!   a2d status             Show RAF closure status
//!   a2d enzymes            List enzymes in the germline

use a2d_core::benchmark::{CaseResult, FitnessReport, seed_benchmark};
use a2d_core::germline::Germline;
use a2d_core::lineage::LineageArchive;
use a2d_core::metabolism::{CycleReport, InvocationLineage, Metabolism, fitness_evidence_artifact};
use a2d_core::provider::{InvocationRequest, Provider, ProviderPolicy, ProviderRegistry};
use a2d_core::self_sandbox;
use a2d_core::types::{ArtifactType, EnzymeDef, EnzymeId};
use a2d_providers::cli::CliProvider;
use senior_swe_bench::{
    SeniorSweBenchOfficialEvaluatorManifestSummary, SeniorSweBenchTask,
    SeniorSweBenchTaskPackageSummary, SeniorSweBenchVariant, build_senior_swe_bench_audit,
    build_senior_swe_bench_cycle_input, build_senior_swe_bench_cycle_input_feedback,
    build_senior_swe_bench_cycle_retry_plan, build_senior_swe_bench_cycle_retry_step,
    build_senior_swe_bench_local_evaluation, build_senior_swe_bench_task_package,
    extract_senior_swe_bench_tasks, parse_senior_swe_bench_cycle_input,
    parse_senior_swe_bench_official_evaluator_manifest, parse_senior_swe_bench_task_package,
    render_senior_swe_bench_task_context,
    validate_senior_swe_bench_retry_plan_and_cycle_input_for_attempt,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod challenges;
mod senior_swe_bench;

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn main() {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("status");
    let arg2 = args.get(2).map(|s| s.as_str()).unwrap_or("");

    match command {
        "cycle" => {
            let (num_cycles, req) = parse_cycle_args(arg2, args.get(3).map(|s| s.as_str()));
            run_cycle(num_cycles, &req);
        }
        "cycle-input" => {
            run_cycle_input(&args[2..]);
        }
        "challenge" => {
            let challenge_name = arg2;
            let num_cycles: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
            run_challenge(challenge_name, num_cycles);
        }
        "compare-topologies" | "benchmark-topologies" => {
            let challenge_name = if arg2.is_empty() { "sudoku" } else { arg2 };
            let num_cycles: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
            run_topology_comparison(challenge_name, num_cycles);
        }
        "score-artifact" => {
            let challenge_name = if arg2.is_empty() { "sudoku" } else { arg2 };
            let artifact_path = args.get(3).map(String::as_str).unwrap_or("-");
            run_score_artifact(challenge_name, artifact_path);
        }
        "fitness-evidence-inspect" => {
            run_fitness_evidence_inspect(&args[2..]);
        }
        "senior-swe-bench-audit" => {
            if args.len() > 5 {
                eprintln!(
                    "Usage: a2d senior-swe-bench-audit <html|-> [task-context|task-package|task-cycle-input <task-id>]"
                );
                std::process::exit(1);
            }
            let input_path = args.get(2).map(String::as_str).unwrap_or("-");
            let mode = args.get(3).map(String::as_str);
            let task_id = args.get(4).map(String::as_str);
            run_senior_swe_bench_audit(input_path, mode, task_id);
        }
        "senior-swe-bench-evaluate" => {
            run_senior_swe_bench_evaluate(&args[2..]);
        }
        "senior-swe-bench-official-evaluator-manifest-inspect" => {
            run_senior_swe_bench_official_evaluator_manifest_inspect(&args[2..]);
        }
        "senior-swe-bench-extract-patch" => {
            let artifact_path = args.get(2).map(String::as_str).unwrap_or("-");
            run_senior_swe_bench_extract_patch(artifact_path);
        }
        "senior-swe-bench-diagnose-artifact" => {
            let artifact_path = args.get(2).map(String::as_str).unwrap_or("-");
            run_senior_swe_bench_diagnose_artifact(artifact_path);
        }
        "senior-swe-bench-select-candidate-artifact" => {
            let manifest_path = args.get(2).map(String::as_str).unwrap_or("-");
            run_senior_swe_bench_select_candidate_artifact(manifest_path);
        }
        "senior-swe-bench-cycle-input-feedback" => {
            let cycle_input_path = args.get(2).map(String::as_str).unwrap_or("-");
            let evaluation_path = args.get(3).map(String::as_str).unwrap_or_else(|| {
                eprintln!("Usage: a2d senior-swe-bench-cycle-input-feedback <task-cycle-input.json|-> <local-evaluation.json|->");
                std::process::exit(1);
            });
            run_senior_swe_bench_cycle_input_feedback(cycle_input_path, evaluation_path);
        }
        "senior-swe-bench-retry-plan" => {
            let cycle_input_path = args.get(2).map(String::as_str).unwrap_or("-");
            let max_attempts = args
                .get(3)
                .map(|raw| {
                    raw.parse::<usize>().unwrap_or_else(|_| {
                        eprintln!("Senior SWE-Bench retry plan max-attempts must be an integer");
                        std::process::exit(1);
                    })
                })
                .unwrap_or(3);
            run_senior_swe_bench_retry_plan(cycle_input_path, max_attempts);
        }
        "senior-swe-bench-retry-step" => {
            run_senior_swe_bench_retry_step(&args[2..]);
        }
        "senior-swe-bench-retry-attempt-plan" => {
            run_senior_swe_bench_retry_attempt_plan(&args[2..]);
        }
        "senior-swe-bench-retry-attempt-extract-patch" => {
            run_senior_swe_bench_retry_attempt_extract_patch(&args[2..]);
        }
        "senior-swe-bench-retry-attempt-evaluate" => {
            run_senior_swe_bench_retry_attempt_evaluate(&args[2..]);
        }
        "senior-swe-bench-retry-attempt-step" => {
            run_senior_swe_bench_retry_attempt_step(&args[2..]);
        }
        "senior-swe-bench-retry-attempt-step-evidence" => {
            run_senior_swe_bench_retry_attempt_step_evidence(&args[2..]);
        }
        "senior-swe-bench-retry-run-result" => {
            run_senior_swe_bench_retry_run_result(&args[2..]);
        }
        "senior-swe-bench-retry-status" => {
            run_senior_swe_bench_retry_status(&args[2..]);
        }
        "senior-swe-bench-retry-run-next-gate" => {
            run_senior_swe_bench_retry_run_next_gate(&args[2..]);
        }
        "senior-swe-bench-retry-execute" => {
            run_senior_swe_bench_retry_execute(&args[2..]);
        }
        "senior-swe-bench-retry-resume-attempt-plan" => {
            run_senior_swe_bench_retry_resume_attempt_plan(&args[2..]);
        }
        "senior-swe-bench-retry-resume-attempt-execute" => {
            run_senior_swe_bench_retry_resume_attempt_execute(&args[2..]);
        }
        "senior-swe-bench-retry-run-next-cycle" => {
            run_senior_swe_bench_retry_run_next_cycle(&args[2..]);
        }
        "compare-provider-policy" | "policy-gate" => {
            let challenge_name = if arg2.is_empty() { "sudoku" } else { arg2 };
            let num_cycles: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
            let proposed_policy_arg = args.get(4).map(String::as_str);
            run_provider_policy_comparison_cli(challenge_name, num_cycles, proposed_policy_arg);
        }
        "validate-escalation" => {
            let challenge_name = if arg2.is_empty() { "sudoku" } else { arg2 };
            let enzyme_id = args.get(3).map(String::as_str).unwrap_or("coder");
            run_escalation_validation(challenge_name, enzyme_id);
        }
        "compare-role-providers" => {
            let challenge_name = if arg2.is_empty() { "sudoku" } else { arg2 };
            let enzyme_id = args.get(3).map(String::as_str).unwrap_or("tester");
            run_role_provider_comparison(challenge_name, enzyme_id, &args[4..]);
        }
        "autopilot" => run_autopilot(AutopilotConfig::parse(&args[2..])),
        "status" => show_status(),
        "enzymes" => list_enzymes(),
        "lineage" => show_lineage(),
        _ => {
            eprintln!(
                "Usage: a2d <cycle|cycle-input|challenge|score-artifact|fitness-evidence-inspect|senior-swe-bench-audit|senior-swe-bench-evaluate|senior-swe-bench-official-evaluator-manifest-inspect|senior-swe-bench-extract-patch|senior-swe-bench-diagnose-artifact|senior-swe-bench-select-candidate-artifact|senior-swe-bench-cycle-input-feedback|senior-swe-bench-retry-plan|senior-swe-bench-retry-step|senior-swe-bench-retry-attempt-plan|senior-swe-bench-retry-attempt-extract-patch|senior-swe-bench-retry-attempt-evaluate|senior-swe-bench-retry-attempt-step|senior-swe-bench-retry-attempt-step-evidence|senior-swe-bench-retry-run-result|senior-swe-bench-retry-status|senior-swe-bench-retry-run-next-gate|senior-swe-bench-retry-execute|senior-swe-bench-retry-resume-attempt-plan|senior-swe-bench-retry-run-next-cycle|compare-topologies|compare-provider-policy|compare-role-providers|validate-escalation|autopilot|status|enzymes|lineage>"
            );
            std::process::exit(1);
        }
    }
}

fn load_or_seed_germline() -> Germline {
    if force_seed_germline(env::var("A2D_GERMLINE").ok().as_deref()) {
        println!("Using seed germline (A2D_GERMLINE=seed)");
        return seed_germline();
    }

    if let Some(germline) = load_lineage_germline() {
        println!(
            "Loaded germline from lineage ({} enzymes)",
            germline.enzymes().len()
        );
        return germline;
    }

    // Fall back to hardcoded seed
    println!("Using seed germline");
    seed_germline()
}

fn load_lineage_germline() -> Option<Germline> {
    let dir = lineage_dir();
    let archive = LineageArchive::init(&dir).ok()?;
    let enzymes = archive.read_germline().ok()?;
    if enzymes.is_empty() {
        None
    } else {
        Some(Germline::new(
            normalize_loaded_enzymes(enzymes),
            baseline_food(),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopologyMode {
    Seed,
    Evolved,
    CurrentPolicy,
    ProposedPolicy,
}

impl TopologyMode {
    fn label(self) -> &'static str {
        match self {
            TopologyMode::Seed => "seed",
            TopologyMode::Evolved => "evolved",
            TopologyMode::CurrentPolicy => "current",
            TopologyMode::ProposedPolicy => "proposed",
        }
    }
}

fn load_germline_for_topology(mode: TopologyMode) -> Germline {
    match mode {
        TopologyMode::Seed => seed_germline(),
        TopologyMode::Evolved | TopologyMode::CurrentPolicy | TopologyMode::ProposedPolicy => {
            load_lineage_germline().unwrap_or_else(seed_germline)
        }
    }
}

fn force_seed_germline(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "seed" | "baseline" | "4" | "true" | "1"
        )
    })
}

fn seed_germline() -> Germline {
    let enzymes = vec![
        EnzymeDef {
            id: EnzymeId::from("coder"),
            reactants: BTreeSet::from([
                ArtifactType::from("design"),
                ArtifactType::from("plan"),
                ArtifactType::from("requirements"),
            ]),
            products: BTreeSet::from([ArtifactType::from("code")]),
            catalysts: BTreeSet::from([
                ArtifactType::from("enzyme_defs"),
                ArtifactType::from("failure_report"),
            ]),
            prompt_template: Some(coder_prompt_template()),
        },
        EnzymeDef {
            id: EnzymeId::from("tester"),
            reactants: BTreeSet::from([ArtifactType::from("code")]),
            products: BTreeSet::from([ArtifactType::from("test_results")]),
            catalysts: BTreeSet::from([ArtifactType::from("code")]),
            prompt_template: Some(
                "You are a code reviewer. Given Rust code, evaluate it.\n\
                 Check: does it compile? Are there tests? Do the tests cover edge cases?\n\
                 Output a brief assessment with pass/fail for each check."
                    .to_string(),
            ),
        },
        EnzymeDef {
            id: EnzymeId::from("evolver"),
            reactants: BTreeSet::from([ArtifactType::from("fitness_report")]),
            products: BTreeSet::from([ArtifactType::from("enzyme_defs")]),
            catalysts: BTreeSet::from([
                ArtifactType::from("enzyme_defs"),
                ArtifactType::from("failure_report"),
                ArtifactType::from("fitness_report"),
                ArtifactType::from("provider_health_report"),
                ArtifactType::from("provider_policy"),
            ]),
            prompt_template: None, // Evolver uses the specialized germline+fitness prompt
        },
        EnzymeDef {
            id: EnzymeId::from("architect"),
            reactants: BTreeSet::from([
                ArtifactType::from("failure_report"),
                ArtifactType::from("fitness_report"),
            ]),
            products: BTreeSet::from([ArtifactType::from("system_patch")]),
            catalysts: BTreeSet::from([
                ArtifactType::from("provider_health_report"),
                ArtifactType::from("provider_policy"),
                ArtifactType::from("system_code"),
            ]),
            prompt_template: None, // Architect uses the specialized system code prompt
        },
    ];

    Germline::new(enzymes, baseline_food())
}

fn baseline_food() -> BTreeSet<ArtifactType> {
    BTreeSet::from([
        ArtifactType::from("design"),
        ArtifactType::from("plan"),
        ArtifactType::from("requirements"),
        // Exogenous artifacts — produced by the sandbox/disk, not by enzymes.
        // Seeded empty, populated by the metabolism after benchmark evaluation.
        ArtifactType::from("failure_report"),
        ArtifactType::from("fitness_report"),
        // provider_health_report: mechanical provider-role outcomes, populated
        // by the metabolism after invocations so adaptation can see latency and
        // timeout patterns without human log reading.
        ArtifactType::from("provider_health_report"),
        // provider_policy: typed provider-role assignment policy, populated by
        // the metabolism from the active ProviderRegistry.
        ArtifactType::from("provider_policy"),
        // system_code: snapshot of modifiable source files, read from disk.
        ArtifactType::from("system_code"),
    ])
}

fn coder_prompt_template() -> String {
    format!(
        "You are a programmer. You receive three complementary artifacts:\n\
         - design: concrete structure or reference implementation details\n\
         - plan: the intended architecture or implementation steps\n\
         - requirements: the user-visible contract to satisfy\n\n\
         Prefer the most specific, most constrained instructions when the artifacts differ.\n\
         Default deliverable: synthesize all three into a SINGLE complete Rust source file.\n\
         If the requirements, design, or plan explicitly require another artifact format (for example, a unified diff candidate patch), follow that explicit deliverable instead.\n\
         {}\n\
         For Rust-source deliverables, the file MUST:\n\
         1. Contain a main() function\n\
         2. Include the exact Rust test module header `#[cfg(test)] mod tests` with at least 3 test cases; omitting this fails the mechanical `has_tests` fitness gate even if the solution code works\n\
         3. Use Result<T, E> for error handling where appropriate\n\
         4. Include /// doc comments on public functions\n\
         5. Compile with `rustc --edition 2024`\n\
         6. Do NOT define a module named `a2d_acceptance` — that module will be appended by the system. If you define it, compilation will fail with a duplicate definition error.\n\
         7. Place all your tests in a module named `tests` (i.e. `#[cfg(test)] mod tests {{ ... }}`), NOT `a2d_acceptance`.\n\
         8. Test the normal path, at least one edge/invalid input, and one end-to-end behavior so the tests are meaningful rather than placeholder assertions.\n\n\
         For unified diff deliverables, output raw unified diff text without markdown fences so evaluators can apply and hash the patch bytes directly.\n\
         Output ONLY the requested artifact. No explanation.",
        coder_benchmark_integrity_rule()
    )
}

fn coder_benchmark_integrity_rule() -> &'static str {
    "Benchmark integrity rule: if the requirements, design, or plan say not to search GitHub, issues, pull requests, commits, forks, public web pages, or solution writeups for benchmark answers, obey that restriction strictly. Solve from the provided context and local tests only."
}

fn normalize_loaded_enzymes(mut enzymes: Vec<EnzymeDef>) -> Vec<EnzymeDef> {
    // Upgrade coder contract
    if let Some(coder) = enzymes
        .iter_mut()
        .find(|enzyme| enzyme.id == EnzymeId::from("coder"))
    {
        coder.reactants = BTreeSet::from([
            ArtifactType::from("design"),
            ArtifactType::from("plan"),
            ArtifactType::from("requirements"),
        ]);
        coder.products.insert(ArtifactType::from("code"));
        coder.catalysts.insert(ArtifactType::from("enzyme_defs"));
        coder.catalysts.insert(ArtifactType::from("failure_report"));

        let structurally_legacy_prompt = coder
            .prompt_template
            .as_deref()
            .is_none_or(|template| !(template.contains("design") && template.contains("plan")));
        if structurally_legacy_prompt {
            coder.prompt_template = Some(coder_prompt_template());
        } else if coder
            .prompt_template
            .as_deref()
            .is_some_and(|template| !template.contains("Benchmark integrity rule"))
        {
            let existing = coder.prompt_template.take().unwrap_or_default();
            coder.prompt_template = Some(format!(
                "{existing}\n\n{}",
                coder_benchmark_integrity_rule()
            ));
        }
    }

    // Ensure evolver learns directly from mechanical sandbox fitness instead
    // of waiting for model-generated tester output.
    if let Some(evolver) = enzymes
        .iter_mut()
        .find(|enzyme| enzyme.id == EnzymeId::from("evolver"))
    {
        evolver
            .reactants
            .remove(&ArtifactType::from("test_results"));
        evolver
            .reactants
            .insert(ArtifactType::from("fitness_report"));
        evolver
            .catalysts
            .remove(&ArtifactType::from("test_results"));
        evolver.catalysts.insert(ArtifactType::from("enzyme_defs"));
        evolver
            .catalysts
            .insert(ArtifactType::from("failure_report"));
        evolver
            .catalysts
            .insert(ArtifactType::from("fitness_report"));
        evolver
            .catalysts
            .insert(ArtifactType::from("provider_health_report"));
        evolver
            .catalysts
            .insert(ArtifactType::from("provider_policy"));
    }

    // Add architect enzyme if missing (true autopoiesis)
    let has_architect = enzymes
        .iter()
        .any(|enzyme| enzyme.id == EnzymeId::from("architect"));
    if !has_architect {
        enzymes.push(EnzymeDef {
            id: EnzymeId::from("architect"),
            reactants: BTreeSet::from([
                ArtifactType::from("failure_report"),
                ArtifactType::from("fitness_report"),
            ]),
            products: BTreeSet::from([ArtifactType::from("system_patch")]),
            catalysts: BTreeSet::from([
                ArtifactType::from("provider_health_report"),
                ArtifactType::from("provider_policy"),
                ArtifactType::from("system_code"),
            ]),
            prompt_template: None,
        });
    }

    if let Some(architect) = enzymes
        .iter_mut()
        .find(|enzyme| enzyme.id == EnzymeId::from("architect"))
    {
        architect
            .catalysts
            .insert(ArtifactType::from("provider_health_report"));
        architect
            .catalysts
            .insert(ArtifactType::from("provider_policy"));
    }

    enzymes
}

fn encode_artifact_value(value: &Value) -> Vec<u8> {
    match value {
        Value::String(text) => text.as_bytes().to_vec(),
        _ => serde_json::to_vec(value).expect("artifact value must serialize"),
    }
}

fn input_artifacts_from_request(input: &str) -> BTreeMap<ArtifactType, Vec<u8>> {
    let mut artifacts = BTreeMap::new();

    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(input) {
        let fallback = map
            .get("requirements")
            .and_then(Value::as_str)
            .or_else(|| map.get("plan").and_then(Value::as_str))
            .or_else(|| map.get("design").and_then(Value::as_str))
            .unwrap_or(input)
            .as_bytes()
            .to_vec();

        for (key, value) in map {
            artifacts.insert(ArtifactType(key), encode_artifact_value(&value));
        }

        // Keep single-string workflows working by backfilling the coder's baseline inputs.
        for artifact in ["requirements", "plan", "design"] {
            artifacts
                .entry(ArtifactType::from(artifact))
                .or_insert_with(|| fallback.clone());
        }
    } else {
        let bytes = input.as_bytes().to_vec();
        for artifact in ["requirements", "plan", "design"] {
            artifacts.insert(ArtifactType::from(artifact), bytes.clone());
        }
    }

    artifacts
}

fn seed_input_artifacts(metabolism: &mut Metabolism, input: &str) {
    for (artifact_type, bytes) in input_artifacts_from_request(input) {
        metabolism.seed_artifact(artifact_type, bytes);
    }
}

fn seed_initial_runtime_artifacts(metabolism: &mut Metabolism, input: &str) {
    seed_input_artifacts(metabolism, input);
    // Don't seed fitness_report or failure_report here — they're populated
    // by the benchmark after the first cycle. The architect should not fire
    // until real data exists.
}

fn build_runtime_registry(germline: &Germline) -> ProviderRegistry {
    build_runtime_registry_with_options(
        germline,
        force_seed_germline(env::var("A2D_GERMLINE").ok().as_deref()),
        runtime_provider_overrides_from_env(),
    )
}

fn build_runtime_registry_with_options(
    germline: &Germline,
    seed_mode: bool,
    runtime_overrides: BTreeMap<String, Option<String>>,
) -> ProviderRegistry {
    let mut registry = build_registry();

    if !seed_mode {
        if let Some(policy) = load_lineage_provider_policy() {
            let application = apply_loaded_provider_policy(&mut registry, germline, &policy);
            if !application.accepted.is_empty() || !application.rejected.is_empty() {
                println!(
                    "Loaded provider policy from lineage ({} accepted, {} rejected)",
                    application.accepted.len(),
                    application.rejected.len()
                );
            }
        }
    }

    // Seed mode bypasses persisted lineage policy, but runtime overrides are
    // explicit operator experiments and must still apply. This keeps
    // compare-topologies seed/evolved comparisons controlled when testing a
    // provider assignment.
    let application = apply_runtime_provider_overrides(&mut registry, runtime_overrides);
    report_runtime_provider_overrides(application);

    registry
}

fn experimental_opencode_model_for_provider(provider_name: &str) -> Option<&'static str> {
    match provider_name {
        "opencode/kimi-for-coding/k2p7" => Some("kimi-for-coding/k2p7"),
        "opencode/zai-coding-plan/glm-5.2" => Some("zai-coding-plan/glm-5.2"),
        // Minimax 3's exact OpenCode ID has varied across provider listings; keep
        // these aliases opt-in so a wrong alias can fail visibly at invocation
        // time without changing the default runtime portfolio.
        "opencode/minimax-coding-plan/MiniMax-3" => Some("minimax-coding-plan/MiniMax-3"),
        "opencode/minimax-coding-plan/Minimax-3" => Some("minimax-coding-plan/Minimax-3"),
        "opencode/minimax-coding-plan/MiniMax-M3" => Some("minimax-coding-plan/MiniMax-M3"),
        _ => None,
    }
}

fn experimental_pi_model_for_provider(provider_name: &str) -> Option<&'static str> {
    match provider_name {
        // Verified with `pi --list-models` on 2026-06-16. Keep these lanes
        // opt-in so A²D can probe Pi-backed models without changing the
        // default OpenCode portfolio or broad rung-6 scope.
        "pi/kimi-coding/k2p7" => Some("kimi-coding/k2p7"),
        "pi/minimax/MiniMax-M3" => Some("minimax/MiniMax-M3"),
        "pi/zai/glm-5.2" => Some("zai/glm-5.2"),
        _ => None,
    }
}

fn register_experimental_provider_if_known(
    registry: &mut ProviderRegistry,
    provider_name: &str,
) -> bool {
    if registry.provider_named(provider_name).is_some() {
        return true;
    }
    if let Some(model) = experimental_opencode_model_for_provider(provider_name) {
        registry.register(Box::new(CliProvider::opencode(model)));
        return true;
    }
    if let Some(model) = experimental_pi_model_for_provider(provider_name) {
        registry.register(Box::new(CliProvider::pi(Some(model))));
        return true;
    }
    false
}

fn register_experimental_providers_from_policy(
    registry: &mut ProviderRegistry,
    policy: &ProviderPolicy,
) {
    for provider_name in policy.assignments.values() {
        register_experimental_provider_if_known(registry, provider_name);
    }
}

fn build_registry() -> ProviderRegistry {
    // Multi-model assignment via CLI providers (they manage their own auth).
    //
    // The experiment: can non-frontier models through the catalytic cycle
    // compete with Gemini 3 Pro running one-shot?
    //
    // Current live split:
    // - coder default: Kimi k2.6, with DeepSeek v4 flash as a cheap parallel
    //   fallback. Direct provider smokes completed in ~3s for both, while GLM,
    //   MiniMax highspeed, and Kimi k2.5 timed out or repeatedly consumed the
    //   critical coder window.
    // - evolver: Kimi k2.6. Live runs showed GLM could consume the full
    //   feedback-metabolism window after mechanical fitness became ready.
    // - tester/architect: GLM 5.1, preserving the previous default for
    //   review/planning roles while removing it from the critical coder/evolver
    //   path.
    //
    // Newly available lanes (OpenCode Kimi k2.7, GLM 5.2, provisional Minimax
    // 3 aliases, plus verified Pi Kimi k2.7 / Minimax 3 / GLM 5.2 IDs) are
    // intentionally opt-in: they are auto-registered only when a runtime
    // override, loaded/provider-comparison policy, or direct role comparison
    // names them. This keeps default coder portfolios and escalation scopes
    // stable until replicated outcome evidence justifies a default change.
    //
    // Gemini is intentionally not registered in the default live configuration:
    // repeated Gemini quota failures consumed ~5 minute timeout windows before
    // the provider circuit breaker could route later invocations.
    let coder = CliProvider::opencode("kimi-for-coding/k2p6");
    let mut registry = ProviderRegistry::new(Box::new(coder));

    registry.register(Box::new(CliProvider::opencode(
        "opencode/deepseek-v4-flash-free",
    )));
    let glm = registry.register(Box::new(CliProvider::opencode("zai-coding-plan/glm-5.1")));
    let pi = registry.register(Box::new(CliProvider::pi(None)));

    registry.assign(EnzymeId::from("evolver"), 0);
    registry.assign(EnzymeId::from("maintainer"), pi);
    for enzyme in ["tester", "architect"] {
        registry.assign(EnzymeId::from(enzyme), glm);
    }

    registry
}

fn runtime_provider_overrides_from_env() -> BTreeMap<String, Option<String>> {
    BTreeMap::from([
        ("tester".to_string(), env::var("A2D_TESTER_PROVIDER").ok()),
        (
            "architect".to_string(),
            env::var("A2D_ARCHITECT_PROVIDER").ok(),
        ),
    ])
}

fn apply_runtime_provider_overrides(
    registry: &mut ProviderRegistry,
    overrides: BTreeMap<String, Option<String>>,
) -> a2d_core::provider::ProviderPolicyApplication {
    let assignments = overrides
        .into_iter()
        .filter_map(|(enzyme, provider)| provider.map(|provider| (enzyme, provider)))
        .collect::<BTreeMap<_, _>>();
    let policy = ProviderPolicy { assignments };
    register_experimental_providers_from_policy(registry, &policy);
    let valid_enzyme_ids = BTreeSet::from([EnzymeId::from("tester"), EnzymeId::from("architect")]);
    registry.apply_policy(&policy, &valid_enzyme_ids)
}

fn report_runtime_provider_overrides(application: a2d_core::provider::ProviderPolicyApplication) {
    for accepted in application.accepted {
        eprintln!(
            "Runtime provider override: {} {} -> {}",
            accepted.enzyme_id, accepted.previous_provider, accepted.provider
        );
    }
    for rejected in application.rejected {
        eprintln!(
            "Rejected runtime provider override for {} -> {}: {}",
            rejected
                .enzyme_id
                .as_ref()
                .map(|id| id.0.as_str())
                .unwrap_or("<unknown>"),
            rejected.provider.as_deref().unwrap_or("<unknown>"),
            rejected.reason
        );
    }
}

fn autopilot_provider_for_attempt<'a>(
    registry: &'a ProviderRegistry,
    enzyme_id: &EnzymeId,
    attempt: usize,
    configured_repair_provider: Option<&str>,
) -> AutopilotProviderAttempt<'a> {
    let primary = registry.provider_for(enzyme_id);
    let alternate = registry.alternative_provider_for(enzyme_id);
    let provider_topology = registry
        .providers()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();

    if attempt == 1 {
        if let Some(provider_name) =
            configured_repair_provider.filter(|name| !name.trim().is_empty())
        {
            if let Some(configured) = registry.provider_named(provider_name) {
                if configured.name() != primary.name() {
                    return AutopilotProviderAttempt {
                        provider: configured,
                        primary_provider: primary.name().to_string(),
                        provider_topology,
                        escalated: true,
                        escalation_reason: format!(
                            "first repair attempt uses configured repair provider {provider_name}"
                        ),
                    };
                }
            }
        }

        if alternate.name() != primary.name() {
            return AutopilotProviderAttempt {
                provider: alternate,
                primary_provider: primary.name().to_string(),
                provider_topology,
                escalated: true,
                escalation_reason:
                    "first repair attempt uses configured alternate maintainer provider".to_string(),
            };
        }
    }

    AutopilotProviderAttempt {
        provider: primary,
        primary_provider: primary.name().to_string(),
        provider_topology,
        escalated: false,
        escalation_reason: if attempt == 0 {
            "primary maintainer attempt".to_string()
        } else if configured_repair_provider
            .is_some_and(|name| registry.provider_named(name).is_none())
        {
            format!(
                "configured repair provider {} is not registered; returning to primary provider",
                configured_repair_provider.unwrap_or_default()
            )
        } else if alternate.name() == primary.name() {
            "no alternate maintainer provider configured".to_string()
        } else {
            "alternate repair attempt already consumed; returning to primary provider".to_string()
        },
    }
}

fn load_lineage_provider_policy() -> Option<ProviderPolicy> {
    let dir = lineage_dir();
    let archive = LineageArchive::init(&dir).ok()?;
    archive.read_provider_policy().ok()
}

fn apply_loaded_provider_policy(
    registry: &mut ProviderRegistry,
    germline: &Germline,
    policy: &ProviderPolicy,
) -> a2d_core::provider::ProviderPolicyApplication {
    register_experimental_providers_from_policy(registry, policy);
    let valid_enzyme_ids = germline
        .enzymes()
        .into_iter()
        .map(|enzyme| enzyme.id.clone())
        .collect::<BTreeSet<_>>();
    registry.apply_policy(policy, &valid_enzyme_ids)
}

fn provider_policy_for_germline(policy: &ProviderPolicy, germline: &Germline) -> ProviderPolicy {
    let valid_enzyme_ids = germline
        .enzymes()
        .into_iter()
        .map(|enzyme| enzyme.id.0.clone())
        .collect::<BTreeSet<_>>();
    ProviderPolicy {
        assignments: policy
            .assignments
            .iter()
            .filter(|(enzyme, _)| valid_enzyme_ids.contains(*enzyme))
            .map(|(enzyme, provider)| (enzyme.clone(), provider.clone()))
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutopilotConfig {
    iterations: usize,
    dry_run: bool,
    allow_dirty: bool,
    repair_attempts: usize,
    repair_provider: Option<String>,
    source_fitness_evidence: Option<PathBuf>,
}

impl AutopilotConfig {
    fn parse(args: &[String]) -> Self {
        let mut config = Self {
            iterations: 1,
            dry_run: false,
            allow_dirty: false,
            repair_attempts: env::var("A2D_AUTOPILOT_REPAIR_ATTEMPTS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(1),
            repair_provider: env::var("A2D_AUTOPILOT_REPAIR_PROVIDER")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            source_fitness_evidence: env::var("A2D_AUTOPILOT_SOURCE_FITNESS_EVIDENCE")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from),
        };

        let mut idx = 0;
        while idx < args.len() {
            match args[idx].as_str() {
                "--iterations" | "-n" => {
                    if let Some(value) = args.get(idx + 1).and_then(|value| value.parse().ok()) {
                        config.iterations = value;
                    }
                    idx += 2;
                }
                "--dry-run" => {
                    config.dry_run = true;
                    idx += 1;
                }
                "--allow-dirty" => {
                    config.allow_dirty = true;
                    idx += 1;
                }
                "--repair-attempts" => {
                    if let Some(value) = args.get(idx + 1).and_then(|value| value.parse().ok()) {
                        config.repair_attempts = value;
                    }
                    idx += 2;
                }
                "--repair-provider" => {
                    if let Some(value) = args.get(idx + 1).filter(|value| !value.trim().is_empty())
                    {
                        config.repair_provider = Some(value.clone());
                    }
                    idx += 2;
                }
                "--source-fitness-evidence" => {
                    if let Some(value) = args.get(idx + 1).filter(|value| !value.trim().is_empty())
                    {
                        config.source_fitness_evidence = Some(PathBuf::from(value));
                    }
                    idx += 2;
                }
                _ => idx += 1,
            }
        }

        config
    }
}

struct AutopilotProviderAttempt<'a> {
    provider: &'a dyn Provider,
    primary_provider: String,
    provider_topology: Vec<String>,
    escalated: bool,
    escalation_reason: String,
}

impl AutopilotProviderAttempt<'_> {
    fn metadata_text(&self, attempt: usize) -> String {
        format!(
            "attempt: {attempt}\nprimary_provider: {}\nattempted_provider: {}\nescalated: {}\nescalation_reason: {}\nregistered_providers: {}",
            self.primary_provider,
            self.provider.name(),
            self.escalated,
            self.escalation_reason,
            self.provider_topology.join(", ")
        )
    }
}

fn autopilot_fault_injection_for_attempt(
    setting: Option<&str>,
    attempt: usize,
) -> Option<&'static str> {
    let normalized = setting?.trim().to_ascii_lowercase().replace('-', "_");
    match (normalized.as_str(), attempt) {
        ("attempt0_parse_failure" | "parse_attempt0" | "parse_failure", 0) => {
            Some("attempt0_parse_failure")
        }
        _ => None,
    }
}

fn configured_autopilot_fault_injection(attempt: usize) -> Option<&'static str> {
    autopilot_fault_injection_for_attempt(
        env::var("A2D_AUTOPILOT_FAULT_INJECTION").ok().as_deref(),
        attempt,
    )
}

#[derive(Debug, Clone)]
struct ProjectState {
    handoff_preview: String,
    todos: Vec<ProjectDoc>,
    plans: Vec<ProjectDoc>,
    git_status: String,
    a2d_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectDoc {
    path: String,
    title: String,
    body: String,
    body_preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectTask {
    source_path: String,
    objective: String,
    acceptance_gates: Vec<String>,
    allows_self_modification: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct ProjectPatchset {
    #[serde(default)]
    replacements: Vec<ProjectFileReplacement>,
    commit_message: String,
    #[serde(default)]
    validation_commands: Vec<String>,
    #[serde(default)]
    handoff_update: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ProjectFileReplacement {
    path: String,
    new_content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectPatchGateReport {
    accepted: bool,
    rejected: Vec<String>,
    requires_cargo_test: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectValidationReport {
    accepted: bool,
    errors: Vec<String>,
    command_results: Vec<ProjectCommandResult>,
    worktree_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectCommandResult {
    command: Vec<String>,
    success: bool,
    status: Option<i32>,
    stdout_preview: String,
    stderr_preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectApplyReport {
    accepted: bool,
    committed: bool,
    errors: Vec<String>,
    command_results: Vec<ProjectCommandResult>,
    commit_hash: Option<String>,
    touched_paths: Vec<String>,
    fitness_evidence_required: bool,
    fitness_evidence_path: Option<String>,
}

#[derive(Debug, Clone)]
struct AutopilotLogger {
    run_id: String,
    aggregate_log: PathBuf,
    run_log: PathBuf,
    run_dir: PathBuf,
}

impl AutopilotLogger {
    fn new(root: &Path) -> Self {
        let run_id = format!("run-{}", unique_suffix());
        let base = root.join(".a2d").join("autopilot");
        let run_dir = base.join("runs").join(&run_id);
        let _ = fs::create_dir_all(&run_dir);
        Self {
            run_id,
            aggregate_log: base.join("events.jsonl"),
            run_log: run_dir.join("events.jsonl"),
            run_dir,
        }
    }

    fn event(&self, event: &str, data: Value) {
        let record = json!({
            "ts_unix_ms": unix_millis(),
            "run_id": self.run_id,
            "event": event,
            "data": data,
        });
        self.append_jsonl(&self.aggregate_log, &record);
        self.append_jsonl(&self.run_log, &record);
    }

    fn artifact(&self, name: &str, content: &str) -> PathBuf {
        let path = self.run_dir.join(name);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(error) = fs::write(&path, content) {
            self.event(
                "artifact_write_failed",
                json!({"name": name, "error": error.to_string()}),
            );
        } else {
            self.event(
                "artifact_written",
                json!({"name": name, "path": path.to_string_lossy()}),
            );
        }
        path
    }

    fn append_jsonl(&self, path: &Path, record: &Value) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{}", record);
        }
    }
}

fn unique_suffix() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        unix_millis(),
        UNIQUE_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

fn unique_temp_path(prefix: &str, extension: &str) -> PathBuf {
    env::temp_dir().join(format!("{prefix}-{}.{}", unique_suffix(), extension))
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn run_autopilot(config: AutopilotConfig) {
    println!("A²D Autopilot ({:?})", config);
    println!("═══════════════════");

    let root = project_root();
    let logger = AutopilotLogger::new(&root);
    logger.event(
        "run_started",
        json!({
            "iterations": config.iterations,
            "dry_run": config.dry_run,
            "allow_dirty": config.allow_dirty,
            "repair_attempts": config.repair_attempts,
            "repair_provider": config.repair_provider,
            "source_fitness_evidence": config.source_fitness_evidence.as_ref().map(|path| path.to_string_lossy().to_string()),
        }),
    );
    println!("Autopilot run id: {}", logger.run_id);
    println!("Autopilot logs: {}", logger.run_log.to_string_lossy());

    let mut state = collect_project_state(&root);
    logger.event(
        "project_state_collected",
        json!({
            "todos": state.todos.len(),
            "plans": state.plans.len(),
            "git_dirty": !state.git_status.trim().is_empty(),
            "git_status_preview": preview(&state.git_status, 1200),
            "a2d_status_preview": preview(&state.a2d_status, 1200),
        }),
    );
    logger.artifact("project-state-handoff-preview.txt", &state.handoff_preview);

    if !config.allow_dirty && !state.git_status.trim().is_empty() {
        println!("Working tree is dirty; autopilot stops before self-modification.");
        println!("Use --allow-dirty only for explicit experiments. Current status:");
        println!("{}", state.git_status);
        logger.event(
            "run_stopped_dirty_worktree",
            json!({"git_status": state.git_status}),
        );
        return;
    }

    'iterations: for iteration in 1..=config.iterations {
        println!("\nIteration {iteration}/{}", config.iterations);
        let Some(task) = select_project_task(&state) else {
            println!("No actionable project task found.");
            logger.event("run_stopped_no_task", json!({"iteration": iteration}));
            return;
        };
        logger.event(
            "task_selected",
            json!({
                "iteration": iteration,
                "source_path": task.source_path,
                "objective": task.objective,
                "acceptance_gates": task.acceptance_gates,
                "allows_self_modification": task.allows_self_modification,
            }),
        );

        println!("Selected task: {}", task.source_path);
        println!("Objective: {}", task.objective);
        println!(
            "Self-modification allowed: {}",
            if task.allows_self_modification {
                "yes"
            } else {
                "no"
            }
        );
        println!("Acceptance gates:");
        for gate in &task.acceptance_gates {
            println!("  - {gate}");
        }

        let prompt = build_maintainer_prompt(&state, &task);
        let prompt_path = logger.artifact(
            &format!("iteration-{iteration}/maintainer-prompt.txt"),
            &prompt,
        );
        logger.event(
            "maintainer_prompt_built",
            json!({
                "iteration": iteration,
                "bytes": prompt.len(),
                "artifact": prompt_path.to_string_lossy(),
            }),
        );
        println!("Maintainer prompt: {} bytes", prompt.len());

        if config.dry_run {
            println!("Dry-run: stopping before provider invocation or filesystem changes.");
            logger.event("dry_run_stop", json!({"iteration": iteration}));
            continue;
        }

        let registry = build_registry();
        let maintainer_id = EnzymeId::from("maintainer");
        let primary_provider = registry.provider_for(&maintainer_id).name().to_string();
        let provider_topology = registry.providers();
        logger.event(
            "maintainer_provider_topology",
            json!({
                "iteration": iteration,
                "primary_provider": primary_provider,
                "registered_providers": provider_topology,
                "repair_escalation": "attempt 1 uses A2D_AUTOPILOT_REPAIR_PROVIDER/--repair-provider when registered, otherwise the configured alternate provider",
                "configured_repair_provider": config.repair_provider,
            }),
        );
        let original_prompt = prompt.clone();
        let mut attempt_prompt = prompt;
        let max_attempts = config.repair_attempts + 1;

        for attempt in 0..max_attempts {
            let provider_attempt = autopilot_provider_for_attempt(
                &registry,
                &maintainer_id,
                attempt,
                config.repair_provider.as_deref(),
            );
            let provider = provider_attempt.provider;
            let provider_metadata = provider_attempt.metadata_text(attempt);

            if attempt == 0 {
                println!("Invoking maintainer via {}...", provider.name());
                logger.event(
                    "maintainer_invocation_started",
                    json!({
                        "iteration": iteration,
                        "attempt": attempt,
                        "provider": provider.name(),
                        "primary_provider": provider_attempt.primary_provider,
                        "registered_providers": provider_attempt.provider_topology,
                        "escalated": provider_attempt.escalated,
                        "escalation_reason": provider_attempt.escalation_reason,
                    }),
                );
            } else {
                println!(
                    "Invoking repair attempt {}/{} via {}...",
                    attempt,
                    config.repair_attempts,
                    provider.name()
                );
                logger.event(
                    "repair_attempt_started",
                    json!({
                        "iteration": iteration,
                        "attempt": attempt,
                        "provider": provider.name(),
                        "primary_provider": provider_attempt.primary_provider,
                        "registered_providers": provider_attempt.provider_topology,
                        "escalated": provider_attempt.escalated,
                        "escalation_reason": provider_attempt.escalation_reason,
                    }),
                );
            }

            let mut response = match provider.invoke(&InvocationRequest {
                enzyme_id: maintainer_id.clone(),
                system: maintainer_system_prompt(),
                prompt: attempt_prompt.clone(),
                max_tokens: 12_000,
            }) {
                Ok(response) => response,
                Err(error) => {
                    let failure = format!("maintainer invocation failed: {error}");
                    logger.event(
                        "maintainer_invocation_failed",
                        json!({"iteration": iteration, "attempt": attempt, "provider": provider.name(), "error": error.to_string()}),
                    );
                    if attempt + 1 < max_attempts {
                        attempt_prompt =
                            build_repair_prompt(&original_prompt, "", &failure, &provider_metadata);
                        continue;
                    }
                    println!("Maintainer invocation failed: {error}");
                    logger.event(
                        "repair_budget_exhausted",
                        json!({"iteration": iteration, "attempt": attempt, "failure": failure}),
                    );
                    return;
                }
            };

            if let Some(fault) = configured_autopilot_fault_injection(attempt) {
                let original_output_bytes = response.text.len();
                response.text = format!(
                    "INTENTIONAL_AUTOPILOT_FAULT[{fault}]: malformed ProjectPatchset for repair-path validation"
                );
                logger.event(
                    "autopilot_fault_injected",
                    json!({
                        "iteration": iteration,
                        "attempt": attempt,
                        "provider": provider.name(),
                        "fault": fault,
                        "original_output_bytes": original_output_bytes,
                    }),
                );
            }

            let output_path = logger.artifact(
                &format!("iteration-{iteration}/attempt-{attempt}/maintainer-output.txt"),
                &response.text,
            );
            if let Some(raw) = &response.raw_output {
                logger.artifact(
                    &format!("iteration-{iteration}/attempt-{attempt}/maintainer-raw-output.txt"),
                    raw,
                );
            }
            logger.event(
                if attempt == 0 {
                    "maintainer_output_received"
                } else {
                    "repair_output_received"
                },
                json!({
                    "iteration": iteration,
                    "attempt": attempt,
                    "provider": provider.name(),
                    "output_bytes": response.text.len(),
                    "raw_output_bytes": response.raw_output.as_ref().map(|raw| raw.len()),
                    "artifact": output_path.to_string_lossy(),
                    "output_preview": preview(&response.text, 1200),
                }),
            );

            let patchset = match parse_project_patchset(&response.text) {
                Ok(patchset) => patchset,
                Err(error) => {
                    let failure = format!("patchset parse failed: {error}");
                    println!("Maintainer returned malformed project_patchset: {error}");
                    println!("Parsed output preview: {}", preview(&response.text, 1200));
                    logger.event(
                        "patchset_parse_failed",
                        json!({
                            "iteration": iteration,
                            "attempt": attempt,
                            "error": error.to_string(),
                            "output_artifact": output_path.to_string_lossy(),
                        }),
                    );
                    if attempt + 1 < max_attempts {
                        attempt_prompt = build_repair_prompt(
                            &original_prompt,
                            &response.text,
                            &failure,
                            &provider_metadata,
                        );
                        continue;
                    }
                    logger.event(
                        "repair_budget_exhausted",
                        json!({"iteration": iteration, "attempt": attempt, "failure": failure}),
                    );
                    return;
                }
            };

            let patchset_summary = serde_json::to_string_pretty(&json!({
                "commit_message": patchset.commit_message,
                "validation_commands": patchset.validation_commands,
                "handoff_update": patchset.handoff_update,
                "replacements": patchset.replacements.iter().map(|replacement| json!({
                    "path": replacement.path,
                    "new_content_bytes": replacement.new_content.len(),
                    "new_content_preview": preview(&replacement.new_content, 400),
                })).collect::<Vec<_>>(),
            }))
            .unwrap_or_default();
            logger.artifact(
                &format!("iteration-{iteration}/attempt-{attempt}/patchset-summary.json"),
                &patchset_summary,
            );

            let gate = validate_project_patchset_paths(&patchset);
            println!("Patchset replacements: {}", patchset.replacements.len());
            println!("Commit message: {}", patchset.commit_message);
            if !patchset.validation_commands.is_empty() {
                println!("Validation commands: {:?}", patchset.validation_commands);
            }
            if !patchset.handoff_update.trim().is_empty() {
                println!(
                    "Handoff update: {}",
                    compact_one_line(&patchset.handoff_update, 240)
                );
            }
            logger.event(
                "patchset_path_gate_evaluated",
                json!({
                    "iteration": iteration,
                    "attempt": attempt,
                    "accepted": gate.accepted,
                    "rejected": gate.rejected,
                    "requires_cargo_test": gate.requires_cargo_test,
                    "replacement_paths": patchset.replacements.iter().map(|replacement| replacement.path.clone()).collect::<Vec<_>>(),
                }),
            );
            if !gate.accepted {
                let failure = format!("path gate rejected patchset: {}", gate.rejected.join("; "));
                println!("Patchset rejected by path gate:");
                for reason in &gate.rejected {
                    println!("  - {reason}");
                }
                if attempt + 1 < max_attempts {
                    attempt_prompt = build_repair_prompt(
                        &original_prompt,
                        &response.text,
                        &failure,
                        &provider_metadata,
                    );
                    continue;
                }
                logger.event(
                    "repair_budget_exhausted",
                    json!({"iteration": iteration, "attempt": attempt, "failure": failure}),
                );
                return;
            }
            if gate.requires_cargo_test {
                println!(
                    "Patchset includes eligible source self-modification; cargo test/self-sandbox required before apply."
                );
            }

            let validation = validate_project_patchset_in_temp_worktree(&root, &patchset, &gate);
            let validation_json = project_validation_report_json(&validation);
            logger.artifact(
                &format!("iteration-{iteration}/attempt-{attempt}/validation-report.json"),
                &serde_json::to_string_pretty(&validation_json).unwrap_or_default(),
            );
            logger.event(
                "temp_worktree_validation_completed",
                json!({
                    "iteration": iteration,
                    "attempt": attempt,
                    "accepted": validation.accepted,
                    "errors": validation.errors,
                    "worktree_path": validation.worktree_path.to_string_lossy(),
                    "commands": validation.command_results.iter().map(|result| json!({
                        "command": result.command,
                        "success": result.success,
                        "status": result.status,
                        "stdout_preview": result.stdout_preview,
                        "stderr_preview": result.stderr_preview,
                    })).collect::<Vec<_>>(),
                }),
            );
            if !validation.accepted {
                let failure = format!(
                    "temp-worktree validation failed: {}",
                    validation.errors.join("; ")
                );
                println!("Temp-worktree validation failed:");
                for error in &validation.errors {
                    println!("  - {error}");
                }
                if attempt + 1 < max_attempts {
                    attempt_prompt = build_repair_prompt(
                        &original_prompt,
                        &response.text,
                        &format!("{failure}\n{}", validation_json),
                        &provider_metadata,
                    );
                    continue;
                }
                logger.event(
                    "repair_budget_exhausted",
                    json!({"iteration": iteration, "attempt": attempt, "failure": failure}),
                );
                return;
            }

            println!(
                "Patchset passed temp-worktree validation. Applying to real tree and committing..."
            );
            logger.event(
                "real_tree_apply_started",
                json!({"iteration": iteration, "attempt": attempt}),
            );
            let apply_report = apply_validated_patchset_to_real_tree(
                &root,
                &patchset,
                &gate,
                config.source_fitness_evidence.as_deref(),
            );
            let apply_json = project_apply_report_json(&apply_report);
            logger.artifact(
                &format!("iteration-{iteration}/attempt-{attempt}/apply-report.json"),
                &serde_json::to_string_pretty(&apply_json).unwrap_or_default(),
            );
            logger.event(
                "real_tree_apply_completed",
                json!({
                    "iteration": iteration,
                    "attempt": attempt,
                    "accepted": apply_report.accepted,
                    "committed": apply_report.committed,
                    "commit_hash": apply_report.commit_hash,
                    "errors": apply_report.errors,
                    "touched_paths": apply_report.touched_paths,
                    "fitness_evidence_required": apply_report.fitness_evidence_required,
                    "fitness_evidence_path": apply_report.fitness_evidence_path,
                    "commands": apply_report.command_results.iter().map(|result| json!({
                        "command": result.command,
                        "success": result.success,
                        "status": result.status,
                        "stdout_preview": result.stdout_preview,
                        "stderr_preview": result.stderr_preview,
                    })).collect::<Vec<_>>(),
                }),
            );
            if apply_report.accepted {
                println!(
                    "Autopilot committed {}",
                    apply_report.commit_hash.as_deref().unwrap_or("unknown")
                );
                if iteration < config.iterations {
                    state = collect_project_state(&root);
                    logger.event(
                        "project_state_refreshed",
                        json!({
                            "completed_iteration": iteration,
                            "next_iteration": iteration + 1,
                            "todos": state.todos.len(),
                            "plans": state.plans.len(),
                            "git_dirty": !state.git_status.trim().is_empty(),
                            "git_status_preview": preview(&state.git_status, 1200),
                            "a2d_status_preview": preview(&state.a2d_status, 1200),
                        }),
                    );
                    logger.artifact(
                        &format!(
                            "iteration-{iteration}/refreshed-project-state-handoff-preview.txt"
                        ),
                        &state.handoff_preview,
                    );
                    if !config.allow_dirty && !state.git_status.trim().is_empty() {
                        println!(
                            "Working tree is dirty after committed iteration; autopilot stops before the next self-modification."
                        );
                        println!("{}", state.git_status);
                        logger.event(
                            "run_stopped_dirty_worktree_after_refresh",
                            json!({"iteration": iteration, "git_status": state.git_status}),
                        );
                        return;
                    }
                    continue 'iterations;
                }
                return;
            }

            let failure = format!(
                "real-tree application failed: {}",
                apply_report.errors.join("; ")
            );
            println!("Real-tree application failed and was rolled back:");
            for error in &apply_report.errors {
                println!("  - {error}");
            }
            if attempt + 1 < max_attempts {
                attempt_prompt = build_repair_prompt(
                    &original_prompt,
                    &response.text,
                    &format!("{failure}\n{}", apply_json),
                    &provider_metadata,
                );
                continue;
            }
            logger.event(
                "repair_budget_exhausted",
                json!({"iteration": iteration, "attempt": attempt, "failure": failure}),
            );
            return;
        }

        return;
    }
}

fn collect_project_state(root: &Path) -> ProjectState {
    let handoff = fs::read_to_string(root.join("docs/HANDOFF.md")).unwrap_or_default();
    ProjectState {
        handoff_preview: preview(&handoff, 2000),
        todos: read_project_docs(root, "todos"),
        plans: read_project_docs(root, "docs/plans"),
        git_status: command_stdout(root, "git", &["status", "--short", "--", "."]),
        a2d_status: command_stdout(root, "cargo", &["run", "-q", "-p", "a2d", "--", "status"]),
    }
}

fn read_project_docs(root: &Path, dir: &str) -> Vec<ProjectDoc> {
    let mut docs = Vec::new();
    collect_markdown_docs(root, &root.join(dir), &mut docs);
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    docs
}

fn collect_markdown_docs(root: &Path, dir: &Path, docs: &mut Vec<ProjectDoc>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_docs(root, &path, docs);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let Ok(body) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        docs.push(ProjectDoc {
            path: relative.to_string_lossy().replace('\\', "/"),
            title: first_markdown_title(&body).unwrap_or_else(|| "Untitled".to_string()),
            body_preview: preview(&body, 1200),
            body,
        });
    }
}

fn select_project_task(state: &ProjectState) -> Option<ProjectTask> {
    let preferred = [
        "todos/autonomous-project-loop.md",
        "todos/provider-policy-topology-gate.md",
        "todos/escalation-rungs-4-6.md",
    ];

    let doc = preferred
        .iter()
        .find_map(|path| {
            state
                .todos
                .iter()
                .find(|doc| doc.path == *path && project_doc_is_actionable(doc))
        })
        .or_else(|| {
            state
                .todos
                .iter()
                .find(|doc| project_doc_is_actionable(doc))
        })?;

    let allows_self_modification = doc.path == "todos/autonomous-project-loop.md"
        || doc.body_preview.contains("self-modification")
        || doc.body_preview.contains("source/mechanism")
        || doc.body_preview.contains("crates/");

    Some(ProjectTask {
        source_path: doc.path.clone(),
        objective: format!(
            "Advance {}: {}",
            doc.title,
            compact_one_line(&doc.body_preview, 220)
        ),
        acceptance_gates: vec![
            "typed project_patchset JSON only".to_string(),
            "path gate rejects protected files and traversal".to_string(),
            "eligible source self-modification goes through self-sandbox/cargo test".to_string(),
            "cargo test passes before commit".to_string(),
            "eligible source self-modification has fresh source-bound a2d.fitness-evidence.v1 actual-test evidence".to_string(),
            "docs/HANDOFF.md updated before commit".to_string(),
        ],
        allows_self_modification,
    })
}

fn project_doc_is_actionable(doc: &ProjectDoc) -> bool {
    let mut saw_checkbox = false;
    for line in doc.body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- [ ]") || trimmed.starts_with("* [ ]") {
            return true;
        }
        if trimmed.starts_with("- [x]")
            || trimmed.starts_with("- [X]")
            || trimmed.starts_with("* [x]")
            || trimmed.starts_with("* [X]")
        {
            saw_checkbox = true;
        }
    }

    !saw_checkbox
}

fn maintainer_system_prompt() -> String {
    "You are A²D's outer-loop maintainer enzyme. Your job is gated autonomous self-modification of this repository.\n\
     Return JSON only. Do not run shell commands. Do not describe changes outside JSON.\n\
     Filesystem tools are unavailable and unnecessary: use only the project_state and file contents provided in the prompt.\n\
     Produce a project_patchset with: commit_message, validation_commands, handoff_update, replacements.\n\
     replacements must be complete file contents. Source self-modification is allowed for eligible mechanism files; protected files are not.\n\
     Source self-modification is committed only with fresh source-bound a2d.fitness-evidence.v1 actual-test evidence; cargo test alone is not enough.\n\
     The path gate rejects empty patchsets: replacements MUST contain at least one file replacement.\n\
     For docs/todo/plan tasks, update the selected markdown file or a directly relevant markdown file with a small concrete improvement.\n\
     Markdown replacements are semantically gated: any referenced repo path such as crates/... or docs/... must exist after the patch.\n\
     Prefer one small atomic change that advances the selected project_task."
        .to_string()
}

fn build_maintainer_prompt(state: &ProjectState, task: &ProjectTask) -> String {
    let todo_index = state
        .todos
        .iter()
        .map(|doc| format!("- {} — {}", doc.path, doc.title))
        .collect::<Vec<_>>()
        .join("\n");
    let plan_index = state
        .plans
        .iter()
        .map(|doc| format!("- {} — {}", doc.path, doc.title))
        .collect::<Vec<_>>()
        .join("\n");

    let selected_doc = state
        .todos
        .iter()
        .chain(state.plans.iter())
        .find(|doc| doc.path == task.source_path);
    let selected_doc_body = selected_doc
        .map(|doc| doc.body.as_str())
        .unwrap_or("selected task body unavailable");

    format!(
        "PROJECT_STATE\n\
         git_status:\n{}\n\n\
         a2d_status:\n{}\n\n\
         handoff_preview:\n{}\n\n\
         todos:\n{}\n\n\
         plans:\n{}\n\n\
         SELECTED_PROJECT_TASK\n\
         source_path: {}\n\
         objective: {}\n\
         allows_self_modification: {}\n\
         acceptance_gates:\n{}\n\n\
         SELECTED_TASK_FILE_CONTENT\n\
         ```markdown\n{}\n```\n\n\
         OUTPUT CONTRACT\n\
         Return exactly one JSON object matching:\n\
         {{\"commit_message\":\"Autopilot: ...\",\"validation_commands\":[\"cargo test\"],\"handoff_update\":\"...\",\"replacements\":[{{\"path\":\"relative/path\",\"new_content\":\"complete file content\"}}]}}\n\
         The replacements array MUST NOT be empty. The path gate rejects replacements: [].\n\
         If the task is documentation/todo/plan work, replace source_path or another approved markdown file with complete updated content.\n\
         Do not claim repo paths that are absent: markdown replacements and handoff_update fail validation when referenced crates/..., docs/..., todos/..., examples/..., or research/... paths do not exist after the patch.\n\
         For source changes under crates/..., cargo test is necessary but not sufficient: autopilot also requires fresh source-bound a2d.fitness-evidence.v1 actual-test evidence before commit.",
        state.git_status,
        state.a2d_status,
        state.handoff_preview,
        todo_index,
        plan_index,
        task.source_path,
        task.objective,
        task.allows_self_modification,
        task.acceptance_gates
            .iter()
            .map(|gate| format!("- {gate}"))
            .collect::<Vec<_>>()
            .join("\n"),
        selected_doc_body,
    )
}

fn build_repair_prompt(
    original_prompt: &str,
    failed_output: &str,
    failure_report: &str,
    provider_metadata: &str,
) -> String {
    format!(
        "The previous autopilot maintainer attempt failed mechanical gates.\n\
         You must repair the output, not explain the failure. Return exactly one ProjectPatchset JSON object.\n\n\
         PROVIDER_ATTEMPT_METADATA\n```text\n{}\n```\n\n\
         FAILURE_REPORT\n```text\n{}\n```\n\n\
         PREVIOUS_OUTPUT\n```text\n{}\n```\n\n\
         ORIGINAL_TASK_AND_CONTEXT\n{}\n\n\
         REPAIR REQUIREMENTS\n\
         - Do not return replacements: []. Empty patchsets fail the path gate.\n\
         - Include at least one complete file replacement that directly advances the original task.\n\
         - For markdown/todo/plan tasks, update source_path or another approved markdown file using complete file content from ORIGINAL_TASK_AND_CONTEXT.\n\
         - Do not invent repo paths. Markdown replacements and handoff_update fail validation when referenced crates/..., docs/..., todos/..., examples/..., or research/... paths do not exist after the patch.\n\
         - Source changes under crates/... require fresh source-bound a2d.fitness-evidence.v1 actual-test evidence before commit; cargo test alone is not enough.\n\
         - Preserve the same typed ProjectPatchset contract; do not return shell commands or prose outside JSON.\n\n\
         OUTPUT CONTRACT\n\
         Return exactly one JSON object matching:\n\
         {{\"commit_message\":\"Autopilot: ...\",\"validation_commands\":[\"cargo test\"],\"handoff_update\":\"...\",\"replacements\":[{{\"path\":\"relative/path\",\"new_content\":\"complete file content\"}}]}}",
        provider_metadata,
        failure_report,
        preview(failed_output, 6000),
        original_prompt,
    )
}

fn parse_project_patchset(text: &str) -> Result<ProjectPatchset, serde_json::Error> {
    let json = extract_json_from_autopilot_output(text).unwrap_or_else(|| text.to_string());
    serde_json::from_str(&json)
}

fn validate_project_patchset_paths(patchset: &ProjectPatchset) -> ProjectPatchGateReport {
    let mut rejected = Vec::new();
    let mut requires_cargo_test = false;

    if patchset.commit_message.trim().is_empty() {
        rejected.push("commit_message must not be empty".to_string());
    }
    if patchset.replacements.is_empty() {
        rejected.push("patchset must contain at least one replacement".to_string());
    }

    for replacement in &patchset.replacements {
        let path = replacement.path.replace('\\', "/");
        if replacement.new_content.is_empty() {
            rejected.push(format!("{path}: new_content must not be empty"));
        }
        if path_is_unsafe(&path) {
            rejected.push(format!("{path}: absolute paths and traversal are rejected"));
            continue;
        }
        if self_sandbox::is_protected(&path) {
            rejected.push(format!(
                "{path}: protected file self-modification is rejected"
            ));
            continue;
        }
        if path.starts_with("crates/") {
            requires_cargo_test = true;
            if !self_sandbox::is_automated_modifiable(&path) {
                rejected.push(format!(
                    "{path}: source self-modification is not in the automated modifiable allowlist"
                ));
            }
            continue;
        }
        if !is_allowed_project_doc_path(&path) {
            rejected.push(format!(
                "{path}: only eligible source files or markdown under docs/plans, docs/solutions, or todos are allowed"
            ));
        }
    }

    ProjectPatchGateReport {
        accepted: rejected.is_empty(),
        rejected,
        requires_cargo_test,
    }
}

fn validate_project_patchset_in_temp_worktree(
    root: &Path,
    patchset: &ProjectPatchset,
    gate: &ProjectPatchGateReport,
) -> ProjectValidationReport {
    let worktree_path = env::temp_dir().join(format!("a2d-autopilot-worktree-{}", unique_suffix()));
    let mut errors = Vec::new();
    let mut command_results = Vec::new();

    if !gate.accepted {
        errors.extend(gate.rejected.clone());
        return ProjectValidationReport {
            accepted: false,
            errors,
            command_results,
            worktree_path,
        };
    }

    if let Err(error) = copy_project_for_autopilot(root, &worktree_path) {
        errors.push(format!("failed to create temp worktree: {error}"));
        return ProjectValidationReport {
            accepted: false,
            errors,
            command_results,
            worktree_path,
        };
    }

    for replacement in &patchset.replacements {
        let normalized = replacement.path.replace('\\', "/");
        let target = worktree_path.join(&normalized);
        if normalized.starts_with("crates/") && !target.exists() {
            errors.push(format!(
                "{normalized}: source self-modification target does not exist in temp worktree"
            ));
            continue;
        }
        if let Some(parent) = target.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            errors.push(format!(
                "{normalized}: failed to create parent dir: {error}"
            ));
            continue;
        }
        if let Err(error) = fs::write(&target, &replacement.new_content) {
            errors.push(format!(
                "{normalized}: failed to write replacement: {error}"
            ));
        }
    }

    errors.extend(validate_patchset_markdown_references(
        &worktree_path,
        patchset,
    ));

    for command in rejected_validation_commands(patchset) {
        errors.push(format!(
            "validation command is not in the allowlist: {command}"
        ));
    }

    let commands = validation_commands_for_patchset(patchset, gate);
    for command in commands {
        match run_allowed_validation_command(&worktree_path, &command) {
            Ok(result) => {
                if !result.success {
                    errors.push(format!(
                        "validation command failed: {}",
                        result.command.join(" ")
                    ));
                }
                command_results.push(result);
            }
            Err(error) => errors.push(error),
        }
    }

    ProjectValidationReport {
        accepted: errors.is_empty(),
        errors,
        command_results,
        worktree_path,
    }
}

fn validate_patchset_markdown_references(root: &Path, patchset: &ProjectPatchset) -> Vec<String> {
    let mut errors = Vec::new();
    for replacement in &patchset.replacements {
        let path = replacement.path.replace('\\', "/");
        if is_markdown_path(&path) {
            errors.extend(validate_markdown_project_references(
                root,
                &path,
                &replacement.new_content,
            ));
        }
    }

    if !patchset.handoff_update.trim().is_empty() {
        errors.extend(validate_markdown_project_references(
            root,
            "handoff_update",
            &patchset.handoff_update,
        ));
    }

    errors
}

fn validate_markdown_project_references(root: &Path, source: &str, text: &str) -> Vec<String> {
    markdown_project_reference_candidates(text)
        .into_iter()
        .filter_map(|reference| {
            if path_is_unsafe(&reference) {
                return Some(format!(
                    "{source}: referenced repo path is unsafe: {reference}"
                ));
            }
            if root.join(&reference).exists() {
                None
            } else {
                Some(format!(
                    "{source}: referenced repo path does not exist: {reference}"
                ))
            }
        })
        .collect()
}

fn markdown_project_reference_candidates(text: &str) -> Vec<String> {
    let mut candidates = BTreeSet::new();
    let mut token = String::new();

    for ch in text.chars() {
        if ch.is_whitespace()
            || matches!(
                ch,
                '`' | '"' | '\'' | '(' | ')' | '[' | ']' | '<' | '>' | ','
            )
        {
            insert_markdown_reference_candidate(&mut candidates, &token);
            token.clear();
        } else {
            token.push(ch);
        }
    }
    insert_markdown_reference_candidate(&mut candidates, &token);

    candidates.into_iter().collect()
}

fn insert_markdown_reference_candidate(candidates: &mut BTreeSet<String>, raw: &str) {
    let Some(candidate) = normalize_markdown_reference_candidate(raw) else {
        return;
    };
    if is_repo_path_reference(&candidate) {
        candidates.insert(candidate);
    }
}

fn normalize_markdown_reference_candidate(raw: &str) -> Option<String> {
    let mut candidate = raw
        .trim_matches(|ch: char| {
            matches!(
                ch,
                '`' | '*'
                    | '_'
                    | '~'
                    | '!'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '<'
                    | '>'
                    | '"'
                    | '\''
                    | ','
                    | ';'
                    | '.'
            )
        })
        .to_string();

    if candidate.is_empty()
        || candidate.contains("://")
        || candidate.contains(char::is_whitespace)
        || candidate
            .chars()
            .any(|ch| matches!(ch, '*' | '?' | '[' | ']' | '{' | '}' | '$'))
    {
        return None;
    }

    if let Some((before_anchor, _)) = candidate.split_once('#') {
        candidate = before_anchor.to_string();
    }

    if let Some((path, suffix)) = candidate.rsplit_once(':') {
        if suffix.chars().all(|ch| ch.is_ascii_digit() || ch == '-')
            && suffix.chars().any(|ch| ch.is_ascii_digit())
        {
            candidate = path.to_string();
        }
    }

    candidate = candidate
        .trim_end_matches(|ch: char| matches!(ch, ':' | ',' | ';' | '.'))
        .to_string();

    if candidate.is_empty() {
        None
    } else {
        Some(candidate)
    }
}

fn is_repo_path_reference(path: &str) -> bool {
    path.starts_with("crates/")
        || path.starts_with("docs/")
        || path.starts_with("todos/")
        || path.starts_with("examples/")
        || path.starts_with("research/")
        || matches!(
            path,
            "Cargo.toml"
                | "Cargo.lock"
                | "MODULE.bazel"
                | "README.md"
                | "AGENTS.md"
                | "CLAUDE.md"
                | "CONSTITUTION.md"
        )
}

fn is_markdown_path(path: &str) -> bool {
    path.ends_with(".md")
}

fn validation_commands_for_patchset(
    patchset: &ProjectPatchset,
    gate: &ProjectPatchGateReport,
) -> Vec<Vec<String>> {
    let mut commands = patchset
        .validation_commands
        .iter()
        .filter_map(|command| parse_allowed_validation_command(command))
        .collect::<Vec<_>>();

    if gate.requires_cargo_test && !commands.iter().any(|command| command == &["cargo", "test"]) {
        commands.push(vec!["cargo".to_string(), "test".to_string()]);
    }

    commands
}

fn rejected_validation_commands(patchset: &ProjectPatchset) -> Vec<String> {
    patchset
        .validation_commands
        .iter()
        .filter(|command| parse_allowed_validation_command(command).is_none())
        .cloned()
        .collect()
}

fn parse_allowed_validation_command(command: &str) -> Option<Vec<String>> {
    let parts = command
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if is_allowed_validation_command(&parts) {
        Some(parts)
    } else {
        None
    }
}

fn is_allowed_validation_command(parts: &[String]) -> bool {
    matches!(
        parts,
        [cargo, test] if cargo == "cargo" && test == "test"
    ) || matches!(
        parts,
        [cargo, test, package_flag, package] if cargo == "cargo" && test == "test" && package_flag == "-p" && package == "a2d"
    ) || matches!(
        parts,
        [cargo, fmt, check] if cargo == "cargo" && fmt == "fmt" && check == "--check"
    ) || matches!(
        parts,
        [cargo, run, quiet, package_flag, package, separator, status]
            if cargo == "cargo"
                && run == "run"
                && quiet == "-q"
                && package_flag == "-p"
                && package == "a2d"
                && separator == "--"
                && status == "status"
    ) || matches!(
        parts,
        [cargo, run, quiet, package_flag, package, separator, autopilot, iterations, one, dry_run]
            if cargo == "cargo"
                && run == "run"
                && quiet == "-q"
                && package_flag == "-p"
                && package == "a2d"
                && separator == "--"
                && autopilot == "autopilot"
                && iterations == "--iterations"
                && one == "1"
                && dry_run == "--dry-run"
    )
}

fn run_allowed_validation_command(
    worktree: &Path,
    command: &[String],
) -> Result<ProjectCommandResult, String> {
    if !is_allowed_validation_command(command) {
        return Err(format!(
            "validation command is not in the allowlist: {}",
            command.join(" ")
        ));
    }
    let Some((program, args)) = command.split_first() else {
        return Err("validation command is empty".to_string());
    };

    match Command::new(program)
        .args(args)
        .current_dir(worktree)
        .output()
    {
        Ok(output) => Ok(ProjectCommandResult {
            command: command.to_vec(),
            success: output.status.success(),
            status: output.status.code(),
            stdout_preview: preview(&String::from_utf8_lossy(&output.stdout), 4000),
            stderr_preview: preview(&String::from_utf8_lossy(&output.stderr), 4000),
        }),
        Err(error) => Err(format!(
            "failed to run validation command {}: {error}",
            command.join(" ")
        )),
    }
}

fn project_validation_report_json(report: &ProjectValidationReport) -> Value {
    json!({
        "accepted": report.accepted,
        "errors": report.errors,
        "worktree_path": report.worktree_path.to_string_lossy(),
        "command_results": report.command_results.iter().map(|result| json!({
            "command": result.command,
            "success": result.success,
            "status": result.status,
            "stdout_preview": result.stdout_preview,
            "stderr_preview": result.stderr_preview,
        })).collect::<Vec<_>>(),
    })
}

fn apply_validated_patchset_to_real_tree(
    root: &Path,
    patchset: &ProjectPatchset,
    gate: &ProjectPatchGateReport,
    source_fitness_evidence_path: Option<&Path>,
) -> ProjectApplyReport {
    let mut errors = Vec::new();
    let mut command_results = Vec::new();
    let mut fitness_evidence_path = None;
    let mut touched_paths = patchset
        .replacements
        .iter()
        .map(|replacement| replacement.path.replace('\\', "/"))
        .collect::<Vec<_>>();

    if !patchset.handoff_update.trim().is_empty()
        && !touched_paths.iter().any(|path| path == "docs/HANDOFF.md")
    {
        touched_paths.push("docs/HANDOFF.md".to_string());
    }
    touched_paths.sort();
    touched_paths.dedup();

    let originals = snapshot_paths(root, &touched_paths);

    for replacement in &patchset.replacements {
        let normalized = replacement.path.replace('\\', "/");
        if let Err(error) = write_real_tree_file(root, &normalized, &replacement.new_content) {
            errors.push(format!(
                "{normalized}: failed to apply replacement: {error}"
            ));
        }
    }

    if !patchset.handoff_update.trim().is_empty()
        && !patchset
            .replacements
            .iter()
            .any(|replacement| replacement.path.replace('\\', "/") == "docs/HANDOFF.md")
    {
        let handoff_path = root.join("docs/HANDOFF.md");
        match fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&handoff_path)
        {
            Ok(mut file) => {
                let _ = writeln!(
                    file,
                    "\n## Autopilot update {}\n\n{}\n",
                    unix_millis(),
                    patchset.handoff_update.trim()
                );
            }
            Err(error) => errors.push(format!(
                "docs/HANDOFF.md: failed to append handoff update: {error}"
            )),
        }
    }

    for command in rejected_validation_commands(patchset) {
        errors.push(format!(
            "validation command is not in the allowlist: {command}"
        ));
    }

    for command in validation_commands_for_patchset(patchset, gate) {
        match run_allowed_validation_command(root, &command) {
            Ok(result) => {
                if !result.success {
                    errors.push(format!(
                        "real-tree validation command failed: {}",
                        result.command.join(" ")
                    ));
                }
                command_results.push(result);
            }
            Err(error) => errors.push(error),
        }
    }

    if gate.requires_cargo_test {
        match source_fitness_evidence_path {
            Some(path) => match validate_autopilot_source_fitness_evidence(root, path) {
                Ok(_) => fitness_evidence_path = Some(path.to_string_lossy().to_string()),
                Err(error) => errors.push(format!(
                    "source fitness evidence rejected: {error}"
                )),
            },
            None => errors.push(
                "source self-modification requires fresh source-bound a2d.fitness-evidence.v1 actual-test evidence"
                    .to_string(),
            ),
        }
    }

    if !errors.is_empty() {
        restore_paths(root, &originals);
        reset_git_paths(root, &touched_paths);
        return ProjectApplyReport {
            accepted: false,
            committed: false,
            errors,
            command_results,
            commit_hash: None,
            touched_paths,
            fitness_evidence_required: gate.requires_cargo_test,
            fitness_evidence_path,
        };
    }

    if let Err(error) = git_add_paths(root, &touched_paths) {
        errors.push(error);
        restore_paths(root, &originals);
        reset_git_paths(root, &touched_paths);
        return ProjectApplyReport {
            accepted: false,
            committed: false,
            errors,
            command_results,
            commit_hash: None,
            touched_paths,
            fitness_evidence_required: gate.requires_cargo_test,
            fitness_evidence_path,
        };
    }

    match git_commit_paths(root, &patchset.commit_message, &touched_paths) {
        Ok(hash) => ProjectApplyReport {
            accepted: true,
            committed: true,
            errors,
            command_results,
            commit_hash: Some(hash),
            touched_paths,
            fitness_evidence_required: gate.requires_cargo_test,
            fitness_evidence_path,
        },
        Err(error) => {
            errors.push(error);
            restore_paths(root, &originals);
            reset_git_paths(root, &touched_paths);
            ProjectApplyReport {
                accepted: false,
                committed: false,
                errors,
                command_results,
                commit_hash: None,
                touched_paths,
                fitness_evidence_required: gate.requires_cargo_test,
                fitness_evidence_path,
            }
        }
    }
}

fn project_apply_report_json(report: &ProjectApplyReport) -> Value {
    json!({
        "accepted": report.accepted,
        "committed": report.committed,
        "errors": report.errors,
        "commit_hash": report.commit_hash,
        "touched_paths": report.touched_paths,
        "fitness_evidence_required": report.fitness_evidence_required,
        "fitness_evidence_path": report.fitness_evidence_path,
        "command_results": report.command_results.iter().map(|result| json!({
            "command": result.command,
            "success": result.success,
            "status": result.status,
            "stdout_preview": result.stdout_preview,
            "stderr_preview": result.stderr_preview,
        })).collect::<Vec<_>>(),
    })
}

fn validate_autopilot_source_fitness_evidence(
    root: &Path,
    evidence_path: &Path,
) -> Result<Value, String> {
    let path = if evidence_path.is_absolute() {
        evidence_path.to_path_buf()
    } else {
        root.join(evidence_path)
    };
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "failed to read source fitness evidence {}: {error}",
            path.display()
        )
    })?;
    let value = validate_exportable_fitness_evidence_shape(&bytes)?;
    let source_revision = value
        .get("source_revision")
        .and_then(Value::as_str)
        .ok_or_else(|| "source fitness evidence missing source_revision".to_string())?;
    let source_tree_dirty = value
        .get("source_tree_dirty")
        .and_then(Value::as_bool)
        .ok_or_else(|| "source fitness evidence missing source_tree_dirty".to_string())?;
    let source_diff_scope = value
        .get("source_diff_scope")
        .and_then(Value::as_str)
        .ok_or_else(|| "source fitness evidence missing source_diff_scope".to_string())?;
    if source_diff_scope != "crates" {
        return Err(format!(
            "source fitness evidence source_diff_scope must be crates, got {source_diff_scope}"
        ));
    }
    let source_diff_hash = value
        .get("source_diff_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| "source fitness evidence missing source_diff_hash".to_string())?;
    let evidence_command = value
        .get("evidence_command")
        .and_then(Value::as_str)
        .ok_or_else(|| "source fitness evidence missing evidence_command".to_string())?;
    if evidence_command.trim().is_empty() || evidence_command == "<unknown>" {
        return Err("source fitness evidence evidence_command is empty".to_string());
    }

    let current_revision = git_scope_revision_at(root, source_diff_scope)?;
    if source_revision != current_revision {
        return Err(format!(
            "source fitness evidence source_revision {source_revision} does not match current revision {current_revision}"
        ));
    }
    let current_status = git_status_for_scope_at(root, source_diff_scope)?;
    reject_untracked_source_files(source_diff_scope, &current_status)?;
    let current_dirty = !current_status.is_empty();
    if source_tree_dirty != current_dirty {
        return Err(format!(
            "source fitness evidence source_tree_dirty {source_tree_dirty} does not match current dirty status {current_dirty}"
        ));
    }
    let current_diff_hash = git_diff_hash_for_scope_at(root, source_diff_scope)?;
    if source_diff_hash != current_diff_hash {
        return Err(format!(
            "source fitness evidence source_diff_hash {source_diff_hash} does not match current {source_diff_scope} diff hash {current_diff_hash}"
        ));
    }

    Ok(value)
}

fn snapshot_paths(root: &Path, paths: &[String]) -> BTreeMap<String, Option<String>> {
    paths
        .iter()
        .map(|path| {
            let content = fs::read_to_string(root.join(path)).ok();
            (path.clone(), content)
        })
        .collect()
}

fn restore_paths(root: &Path, originals: &BTreeMap<String, Option<String>>) {
    for (path, content) in originals {
        let target = root.join(path);
        match content {
            Some(content) => {
                if let Some(parent) = target.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&target, content);
            }
            None => {
                let _ = fs::remove_file(&target);
            }
        }
    }
}

fn write_real_tree_file(root: &Path, path: &str, content: &str) -> std::io::Result<()> {
    let target = root.join(path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target, content)
}

fn git_add_paths(root: &Path, paths: &[String]) -> Result<(), String> {
    let output = Command::new("git")
        .arg("add")
        .args(paths)
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to run git add: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git add failed: {}",
            preview(&String::from_utf8_lossy(&output.stderr), 1200)
        ))
    }
}

fn reset_git_paths(root: &Path, paths: &[String]) {
    let _ = Command::new("git")
        .args(["reset", "--"])
        .args(paths)
        .current_dir(root)
        .output();
}

fn git_commit_paths(root: &Path, message: &str, paths: &[String]) -> Result<String, String> {
    if paths.is_empty() {
        return Err("git commit refused: no scoped paths to commit".to_string());
    }

    let commit = Command::new("git")
        .args(["commit", "-m", message, "--"])
        .args(paths)
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to run git commit: {error}"))?;
    if !commit.status.success() {
        return Err(format!(
            "git commit failed: {}{}",
            preview(&String::from_utf8_lossy(&commit.stdout), 1200),
            preview(&String::from_utf8_lossy(&commit.stderr), 1200)
        ));
    }

    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to read commit hash: {error}"))?;
    if hash.status.success() {
        Ok(String::from_utf8_lossy(&hash.stdout).trim().to_string())
    } else {
        Ok("unknown".to_string())
    }
}

fn copy_project_for_autopilot(src: &Path, dst: &Path) -> std::io::Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir_all(dst)?;
    copy_dir_for_autopilot(src, dst)
}

fn copy_dir_for_autopilot(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(name.as_ref(), ".git" | ".a2d" | "target") {
            continue;
        }
        let target = dst.join(name.as_ref());
        if path.is_dir() {
            fs::create_dir_all(&target)?;
            copy_dir_for_autopilot(&path, &target)?;
        } else if path.is_file() {
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

fn path_is_unsafe(path: &str) -> bool {
    let path = Path::new(path);
    path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

fn is_allowed_project_doc_path(path: &str) -> bool {
    path == "docs/HANDOFF.md"
        || (path.ends_with(".md")
            && (path.starts_with("docs/plans/")
                || path.starts_with("docs/solutions/")
                || path.starts_with("todos/")))
}

fn extract_json_from_autopilot_output(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed.to_string());
    }

    let fence_start = trimmed.find("```json").or_else(|| trimmed.find("```"))?;
    let after_fence = &trimmed[fence_start..];
    let first_newline = after_fence.find('\n')?;
    let body = &after_fence[first_newline + 1..];
    let fence_end = body.find("```")?;
    Some(body[..fence_end].trim().to_string())
}

fn first_markdown_title(body: &str) -> Option<String> {
    body.lines().find_map(|line| {
        line.strip_prefix("# ")
            .map(|title| title.trim().to_string())
    })
}

fn preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let mut out = text
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        out.push('…');
        out
    }
}

fn command_stdout(root: &Path, command: &str, args: &[&str]) -> String {
    match Command::new(command).args(args).current_dir(root).output() {
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).to_string();
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.trim().is_empty() {
                    text.push_str(&stderr);
                }
            }
            preview(&text, 4000)
        }
        Err(error) => format!("failed to run {command}: {error}"),
    }
}

fn show_status() {
    let germline = load_or_seed_germline();
    let status = germline.raf_status();

    println!("A²D Status");
    println!("══════════");
    println!("RAF coverage: {:.0}%", status.coverage * 100.0);
    println!("Closed: {}", if status.is_closed() { "yes" } else { "no" });
    println!("Enzymes in RAF: {}", status.max_raf.len());
    if !status.orphans.is_empty() {
        println!("Orphans: {:?}", status.orphans);
    }
}

fn list_enzymes() {
    let germline = load_or_seed_germline();
    println!("A²D Enzymes");
    println!("═══════════");
    for enzyme in germline.enzymes() {
        println!("\n{}", enzyme.id);
        println!("  reactants: {:?}", enzyme.reactants);
        println!("  products:  {:?}", enzyme.products);
        println!("  catalysts: {:?}", enzyme.catalysts);
    }
}

fn parse_cycle_args(arg2: &str, arg3: Option<&str>) -> (usize, String) {
    let default_req = "Implement a function that checks if a number is prime".to_string();

    // a2d cycle                     → 1 cycle, default req
    // a2d cycle 3                   → 3 cycles, default req
    // a2d cycle "build a parser"    → 1 cycle, custom req
    // a2d cycle 3 "build a parser"  → 3 cycles, custom req
    if arg2.is_empty() {
        (1, default_req)
    } else if let Ok(n) = arg2.parse::<usize>() {
        let req = arg3.map(|s| s.to_string()).unwrap_or(default_req);
        (n, req)
    } else {
        (1, arg2.to_string())
    }
}

const BENCHMARK_CHECKOUT_CONTEXT_ARTIFACT: &str = "benchmark_checkout_context";
const BENCHMARK_CHECKOUT_CONTEXT_MAX_FILES: usize = 48;
const BENCHMARK_CHECKOUT_CONTEXT_MAX_BYTES: usize = 96 * 1024;
const BENCHMARK_CHECKOUT_CONTEXT_MAX_BYTES_PER_FILE: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CycleInputConfig {
    path: String,
    num_cycles: usize,
    output_artifacts: Option<PathBuf>,
    checkout: Option<PathBuf>,
}

fn parse_cycle_input_args(args: &[String]) -> Result<CycleInputConfig, String> {
    let path = args
        .first()
        .ok_or_else(|| "missing cycle input path".to_string())?;
    let mut num_cycles: Option<usize> = None;
    let mut output_artifacts: Option<PathBuf> = None;
    let mut checkout: Option<PathBuf> = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--output-artifacts" => {
                if output_artifacts.is_some() {
                    return Err("duplicate --output-artifacts argument".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--output-artifacts requires a directory".to_string())?;
                output_artifacts = Some(PathBuf::from(value));
                index += 2;
            }
            "--checkout" => {
                if checkout.is_some() {
                    return Err("duplicate --checkout argument".to_string());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--checkout requires a directory".to_string())?;
                checkout = Some(PathBuf::from(value));
                index += 2;
            }
            value if num_cycles.is_none() => {
                let parsed = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid cycle count for cycle-input: {value}"))?;
                num_cycles = Some(parsed);
                index += 1;
            }
            value => return Err(format!("unknown cycle-input argument: {value}")),
        }
    }
    let num_cycles = num_cycles.unwrap_or(1);
    if num_cycles == 0 {
        return Err("cycle-input cycle count must be greater than zero".to_string());
    }
    Ok(CycleInputConfig {
        path: path.to_string(),
        num_cycles,
        output_artifacts,
        checkout,
    })
}

fn validate_cycle_input_bundle(input: &str) -> Result<(), String> {
    let Value::Object(map) = serde_json::from_str::<Value>(input)
        .map_err(|_| "cycle-input requires a JSON object artifact bundle".to_string())?
    else {
        return Err("cycle-input requires a JSON object artifact bundle".to_string());
    };
    validate_cycle_input_artifact_keys(&map)
}

fn validate_cycle_input_artifact_keys(map: &serde_json::Map<String, Value>) -> Result<(), String> {
    for key in map.keys() {
        if is_reserved_cycle_input_artifact(key) {
            return Err(format!(
                "cycle-input cannot seed reserved runtime artifact: {key}"
            ));
        }
    }
    Ok(())
}

fn enrich_cycle_input_with_checkout(input: &str, checkout: &Path) -> Result<String, String> {
    let Value::Object(mut map) = serde_json::from_str::<Value>(input)
        .map_err(|_| "cycle-input requires a JSON object artifact bundle".to_string())?
    else {
        return Err("cycle-input requires a JSON object artifact bundle".to_string());
    };
    validate_cycle_input_artifact_keys(&map)?;
    let checkout_context = build_benchmark_checkout_context(checkout)?;

    let design = map
        .get("design")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let enriched_design = format!(
        "{design}\n\nBENCHMARK CHECKOUT CONTEXT (read-only snapshot supplied by A²D; solve from this local context and local tests only; do not search GitHub or the public web):\nCRITICAL: provider invocations are no-tools/artifact-only in isolated temporary working directories. You cannot run ls, cat, find, grep, shell commands, or any filesystem inspection tools against the benchmark checkout. Use only the supplied checkout snapshot text below, then return only a unified diff candidate patch.\n{checkout_context}"
    );
    map.insert("design".to_string(), Value::String(enriched_design));
    map.insert(
        BENCHMARK_CHECKOUT_CONTEXT_ARTIFACT.to_string(),
        Value::String(checkout_context),
    );

    serde_json::to_string(&Value::Object(map))
        .map_err(|error| format!("failed to serialize enriched cycle input: {error}"))
}

fn build_benchmark_checkout_context(checkout: &Path) -> Result<String, String> {
    let link_metadata = fs::symlink_metadata(checkout).map_err(|error| {
        format!(
            "failed to read checkout {}: {error}",
            checkout.to_string_lossy()
        )
    })?;
    if link_metadata.file_type().is_symlink() {
        return Err(format!(
            "cycle-input --checkout must not be a symlink: {}",
            checkout.to_string_lossy()
        ));
    }
    if !link_metadata.is_dir() {
        return Err(format!(
            "cycle-input --checkout must be a directory: {}",
            checkout.to_string_lossy()
        ));
    }
    let canonical_checkout = fs::canonicalize(checkout).map_err(|error| {
        format!(
            "failed to canonicalize checkout {}: {error}",
            checkout.to_string_lossy()
        )
    })?;

    let mut files = Vec::new();
    collect_checkout_context_files(&canonical_checkout, &canonical_checkout, &mut files)?;
    files.sort();

    let mut output = String::new();
    output.push_str("schema_version: a2d.benchmark-checkout-context.v1\n");
    output.push_str("checkout: <benchmark-checkout>\n");
    output.push_str(&format!(
        "limits: max_files={BENCHMARK_CHECKOUT_CONTEXT_MAX_FILES}, max_total_bytes={BENCHMARK_CHECKOUT_CONTEXT_MAX_BYTES}, max_bytes_per_file={BENCHMARK_CHECKOUT_CONTEXT_MAX_BYTES_PER_FILE}\n"
    ));
    output.push_str("files:\n");

    let mut included_files = 0usize;
    let mut total_bytes = output.len();
    for relative in files {
        if included_files >= BENCHMARK_CHECKOUT_CONTEXT_MAX_FILES {
            break;
        }
        let path = canonical_checkout.join(&relative);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "failed to inspect checkout file {}: {error}",
                path.to_string_lossy()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        let canonical_path = fs::canonicalize(&path).map_err(|error| {
            format!(
                "failed to canonicalize checkout file {}: {error}",
                path.to_string_lossy()
            )
        })?;
        if !canonical_path.starts_with(&canonical_checkout) {
            continue;
        }
        let bytes = fs::read(&canonical_path).map_err(|error| {
            format!(
                "failed to read checkout file {}: {error}",
                path.to_string_lossy()
            )
        })?;
        if bytes.is_empty() || bytes.len() > BENCHMARK_CHECKOUT_CONTEXT_MAX_BYTES_PER_FILE {
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        let entry =
            format!("\n--- BEGIN FILE {relative} ---\n{text}\n--- END FILE {relative} ---\n");
        if total_bytes + entry.len() > BENCHMARK_CHECKOUT_CONTEXT_MAX_BYTES {
            break;
        }
        output.push_str(&entry);
        total_bytes += entry.len();
        included_files += 1;
    }

    if included_files == 0 {
        return Err(format!(
            "cycle-input --checkout found no bounded UTF-8 source/context files under {}",
            checkout.to_string_lossy()
        ));
    }
    output.push_str(&format!("\nincluded_files: {included_files}\n"));
    Ok(output)
}

fn collect_checkout_context_files(
    root: &Path,
    dir: &Path,
    files: &mut Vec<String>,
) -> Result<(), String> {
    if files.len() >= BENCHMARK_CHECKOUT_CONTEXT_MAX_FILES * 3 {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|error| {
        format!(
            "failed to read checkout directory {}: {error}",
            dir.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to read checkout directory entry {}: {error}",
                dir.display()
            )
        })?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if is_skipped_checkout_segment(&name) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "failed to inspect checkout path {}: {error}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_checkout_context_files(root, &path, files)?;
        } else if metadata.is_file()
            && metadata.len() <= BENCHMARK_CHECKOUT_CONTEXT_MAX_BYTES_PER_FILE as u64
            && let Some(relative) = checkout_relative_context_path(root, &path)
            && is_checkout_context_file(&relative)
            && !is_sensitive_checkout_context_path(&relative)
        {
            files.push(relative);
        }
    }
    Ok(())
}

fn checkout_relative_context_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root).ok().and_then(|relative| {
        let parts = relative
            .components()
            .map(|component| match component {
                Component::Normal(part) => Some(part.to_string_lossy().to_string()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        Some(parts.join("/"))
    })
}

fn is_skipped_checkout_segment(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        ".git"
            | "target"
            | "node_modules"
            | ".venv"
            | "vendor"
            | "dist"
            | "build"
            | ".ssh"
            | ".aws"
            | ".config"
            | ".gnupg"
    ) || lower.starts_with(".env")
        || lower.contains("secret")
        || lower.contains("credential")
        || lower.contains("token")
}

fn is_sensitive_checkout_context_path(relative: &str) -> bool {
    let lower = relative.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    name.starts_with(".env")
        || matches!(
            name,
            ".npmrc" | ".pypirc" | "credentials" | "credentials.json"
        )
        || name.contains("secret")
        || name.contains("credential")
        || name.contains("token")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
}

fn is_checkout_context_file(relative: &str) -> bool {
    let Some(name) = relative.rsplit('/').next() else {
        return false;
    };
    if matches!(name, "README" | "README.md" | "Cargo.toml" | "package.json") {
        return true;
    }
    let Some(extension) = name.rsplit_once('.').map(|(_, extension)| extension) else {
        return false;
    };
    matches!(
        extension,
        "rs" | "toml"
            | "md"
            | "txt"
            | "json"
            | "yaml"
            | "yml"
            | "py"
            | "go"
            | "js"
            | "ts"
            | "tsx"
            | "jsx"
            | "java"
            | "kt"
            | "swift"
            | "rb"
            | "php"
            | "c"
            | "h"
            | "cpp"
            | "hpp"
            | "cs"
            | "scala"
            | "ex"
            | "exs"
            | "sql"
            | "sh"
    )
}

fn is_reserved_cycle_input_artifact(key: &str) -> bool {
    matches!(
        key,
        "fitness_report"
            | "failure_report"
            | "provider_health_report"
            | "provider_policy"
            | "system_code"
            | BENCHMARK_CHECKOUT_CONTEXT_ARTIFACT
    )
}

fn run_cycle_input(args: &[String]) {
    let config = parse_cycle_input_args(args).unwrap_or_else(|error| {
        eprintln!("{error}");
        eprintln!(
            "Usage: a2d cycle-input <artifact-bundle.json|-> [cycles] [--checkout <dir>] [--output-artifacts <dir>]"
        );
        std::process::exit(1);
    });
    let input = read_artifact_or_exit(&config.path);
    validate_cycle_input_bundle(&input).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
    let input = if let Some(checkout) = &config.checkout {
        enrich_cycle_input_with_checkout(&input, checkout).unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(1);
        })
    } else {
        input
    };
    run_cycle_with_options(
        config.num_cycles,
        &input,
        config.output_artifacts.as_deref(),
    );
}

fn run_cycle(num_cycles: usize, requirements: &str) {
    run_cycle_with_options(num_cycles, requirements, None);
}

fn export_cycle_output_artifacts(
    report: &CycleReport,
    output_dir: &Path,
    manifest_records: &mut Vec<Value>,
    reserved_paths: &mut BTreeSet<PathBuf>,
) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(output_dir).map_err(|error| {
        format!(
            "failed to create output artifact directory {}: {error}",
            output_dir.display()
        )
    })?;

    let mut paths = Vec::new();
    for entry in &report.lineage {
        for (artifact, bytes) in &entry.outputs {
            let file_name = format!(
                "cycle-{}-{}-{}-{}.artifact",
                entry.cycle,
                sanitize_output_artifact_segment(&entry.workcell_id.0),
                sanitize_output_artifact_segment(&entry.enzyme_id.0),
                sanitize_output_artifact_segment(&artifact.0)
            );
            let path = output_dir.join(file_name);
            if path.exists() || !reserved_paths.insert(path.clone()) {
                return Err(format!(
                    "cycle output artifact path already exists or collides: {}",
                    path.display()
                ));
            }
            fs::write(&path, bytes)
                .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
            let hash = git_hash_object_bytes(bytes)?;
            manifest_records.push(json!({
                "cycle": entry.cycle,
                "report_cycle": report.cycle,
                "workcell_id": entry.workcell_id.0,
                "enzyme_id": entry.enzyme_id.0,
                "provider": entry.provider,
                "artifact_type": artifact.0,
                "path": path.to_string_lossy(),
                "git_object_hash": hash,
                "bytes": bytes.len(),
            }));
            paths.push(path);
        }
    }

    Ok(paths)
}

fn write_cycle_output_artifact_manifest(
    output_dir: &Path,
    records: &[Value],
) -> Result<Option<PathBuf>, String> {
    if records.is_empty() {
        return Ok(None);
    }
    let manifest_path = output_dir.join("manifest.json");
    if manifest_path.exists() {
        return Err(format!(
            "cycle output artifact manifest already exists: {}",
            manifest_path.display()
        ));
    }
    let manifest = json!({
        "schema_version": "a2d.cycle-output-artifacts.v1",
        "artifacts": records,
    });
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("failed to serialize output artifact manifest: {error}"))?;
    fs::write(&manifest_path, bytes)
        .map_err(|error| format!("failed to write {}: {error}", manifest_path.display()))?;
    Ok(Some(manifest_path))
}

fn sanitize_output_artifact_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "artifact".to_string()
    } else {
        sanitized
    }
}

fn run_cycle_with_options(num_cycles: usize, requirements: &str, output_artifacts: Option<&Path>) {
    println!("A²D Catalytic Cycle ({num_cycles} cycle(s))");
    println!("Requirements: {requirements}");
    println!("═══════════════════");

    let germline = load_or_seed_germline();
    let registry = build_runtime_registry(&germline);
    let mut metabolism = apply_runtime_env(
        Metabolism::new(germline, registry)
            .with_benchmark(seed_benchmark())
            .with_project_root(project_root()),
    );

    seed_initial_runtime_artifacts(&mut metabolism, requirements);

    let lineage_dir = lineage_dir();
    let archive = LineageArchive::init(&lineage_dir).ok();

    let mut total_mutations = 0;
    let mut total_invocations = 0;
    let mut output_artifact_manifest_records = Vec::new();
    let mut output_artifact_reserved_paths = BTreeSet::new();

    if let Some(output_dir) = output_artifacts {
        let manifest_path = output_dir.join("manifest.json");
        if manifest_path.exists() {
            eprintln!(
                "Output artifact export error: manifest already exists: {}",
                manifest_path.display()
            );
            std::process::exit(1);
        }
    }

    for cycle_num in 1..=num_cycles {
        println!("\nRunning cycle {cycle_num}/{num_cycles}...");
        let provider_policy_before_cycle =
            provider_policy_for_germline(&metabolism.provider_policy(), metabolism.germline());
        let report = metabolism.run_cycle();

        total_mutations += report.accepted_mutations;
        total_invocations += report.invocations;

        for entry in &report.lineage {
            println!("\n  [{} via {}]", entry.enzyme_id, entry.provider);
            match &entry.outcome {
                a2d_core::workcell::WorkcellOutcome::Success { .. } => {
                    println!("    outcome: SUCCESS");
                }
                a2d_core::workcell::WorkcellOutcome::Failed { error } => {
                    println!("    outcome: FAILED — {error}");
                }
                a2d_core::workcell::WorkcellOutcome::Killed { reason } => {
                    println!("    outcome: KILLED — {reason:?}");
                }
            }
        }

        if let Some(output_dir) = output_artifacts {
            match export_cycle_output_artifacts(
                &report,
                output_dir,
                &mut output_artifact_manifest_records,
                &mut output_artifact_reserved_paths,
            ) {
                Ok(paths) if paths.is_empty() => {
                    println!("  Output artifacts: none materialized");
                }
                Ok(paths) => {
                    println!(
                        "  Output artifacts: {} file(s) under {}",
                        paths.len(),
                        output_dir.display()
                    );
                }
                Err(error) => {
                    eprintln!("  Output artifact export error: {error}");
                    std::process::exit(1);
                }
            }
        }

        println!("\n  Invocations: {}", report.invocations);
        println!("  Completed:   {}", report.completed);
        println!("  Killed:      {}", report.killed);
        println!("  Failed:      {}", report.failed);
        println!("  Mutations accepted: {}", report.accepted_mutations);
        println!("  Mutations rejected: {}", report.rejected_mutations);
        if report.accepted_patches > 0 || report.rejected_patches > 0 {
            println!("  Patches accepted:  {}", report.accepted_patches);
            println!("  Patches rejected:  {}", report.rejected_patches);
        }
        if report.accepted_provider_policy_changes > 0
            || report.rejected_provider_policy_changes > 0
        {
            println!(
                "  Provider policy accepted: {}",
                report.accepted_provider_policy_changes
            );
            println!(
                "  Provider policy rejected: {}",
                report.rejected_provider_policy_changes
            );
        }

        if report.capped {
            println!("  ⚠ Cycle capped (max invocations reached — advanced with current fitness)");
        }
        if report.wall_clock_capped {
            println!("  ⚠ Cycle wall-clock capped — advanced with current fitness");
        }

        let status = metabolism.germline().raf_status();
        print!(
            "  RAF: {:.0}% | Closed: {}",
            status.coverage * 100.0,
            if status.is_closed() { "yes" } else { "no" }
        );

        if let Some(ref fitness) = report.fitness {
            let delta = report.fitness_delta.unwrap_or(0.0);
            let arrow = if delta > 0.0 {
                "↑"
            } else if delta < 0.0 {
                "↓"
            } else {
                "→"
            };
            print!(
                " | Fitness: {:.0}% ({}/{}) {arrow}{:.0}%",
                fitness.fitness * 100.0,
                fitness.passed,
                fitness.total,
                delta.abs() * 100.0
            );
        }
        println!();

        // Fitness-gated persistence: accepted mutations become durable only when
        // this cycle produced actual benchmark evidence. RAF closure alone is
        // not self-improvement.
        let regressed = report.fitness_delta.is_some_and(|d| d < 0.0);
        let has_fitness_evidence = report_has_actual_fitness_evidence(&report);
        if report.accepted_mutations > 0 {
            if !has_fitness_evidence {
                println!("  ⚠ No actual-test fitness evidence — skipping lineage commit");
            } else if regressed {
                println!("  ⚠ Fitness regressed — skipping lineage commit");
                // Archive stays at previous generation's state.
                // The in-memory germline has the regression but it won't persist.
            } else if let Some(ref archive) = archive {
                match archive.commit_germline(metabolism.germline(), &report) {
                    Ok(hash) => println!("  Lineage: {hash}"),
                    Err(e) => eprintln!("  Lineage error: {e}"),
                }
            }
        }
        if report.accepted_provider_policy_changes > 0 {
            if !has_fitness_evidence {
                println!("  ⚠ No actual-test fitness evidence — skipping provider policy commit");
            } else if regressed {
                println!("  ⚠ Fitness regressed — skipping provider policy commit");
            } else if let Some(ref archive) = archive {
                let proposed_policy = provider_policy_for_germline(
                    &metabolism.provider_policy(),
                    metabolism.germline(),
                );
                let gate = run_provider_policy_gate(
                    metabolism.germline().clone(),
                    &provider_policy_gate_challenge("sudoku"),
                    provider_policy_gate_cycles(),
                    &provider_policy_before_cycle,
                    &proposed_policy,
                );
                print_provider_policy_gate_summary(&gate);
                match commit_provider_policy_if_gate_accepts(
                    archive,
                    &proposed_policy,
                    &report,
                    &gate.decision,
                ) {
                    Ok(Some(hash)) => println!("  Provider policy lineage: {hash}"),
                    Ok(None) => println!(
                        "  ⚠ Provider policy gate rejected durable commit: {}",
                        gate.decision.reason
                    ),
                    Err(e) => eprintln!("  Provider policy lineage error: {e}"),
                }
            }
        }

        // Apply accepted system patches to the real source tree only when the
        // patch-producing cycle was grounded in actual benchmark evidence.
        if report.accepted_patches > 0 {
            if has_fitness_evidence && !regressed {
                apply_accepted_patches(&metabolism);
            } else {
                println!(
                    "  ⚠ No non-regressing actual-test fitness evidence — skipping patch apply"
                );
            }
        }
    }

    if let Some(output_dir) = output_artifacts {
        if let Err(error) =
            write_cycle_output_artifact_manifest(output_dir, &output_artifact_manifest_records)
        {
            eprintln!("Output artifact manifest error: {error}");
            std::process::exit(1);
        }
    }

    println!("\n═══════════════════");
    println!(
        "Total: {total_invocations} invocations, {total_mutations} mutations across {num_cycles} cycle(s)"
    );
}

fn challenge_by_name(name: &str) -> Option<challenges::Challenge> {
    match name {
        "chess" => Some(challenges::chess_engine()),
        "sudoku" => Some(challenges::sudoku_solver()),
        "rubiks" => Some(challenges::rubiks_cube()),
        _ => None,
    }
}

fn load_challenge_or_exit(name: &str) -> challenges::Challenge {
    challenge_by_name(name).unwrap_or_else(|| {
        eprintln!("Unknown challenge: {name}");
        eprintln!("Available: chess, sudoku, rubiks");
        std::process::exit(1);
    })
}

#[derive(Debug, Clone)]
struct TopologyRunSummary {
    topology: TopologyMode,
    challenge: String,
    requested_cycles: usize,
    enzymes: usize,
    elapsed_secs: f64,
    total_invocations: usize,
    total_mutations: usize,
    accepted_patches: usize,
    rejected_patches: usize,
    provider_failures: usize,
    killed: usize,
    invocation_capped_cycles: usize,
    wall_clock_capped_cycles: usize,
    best_fitness: f64,
    best_passed: usize,
    best_total: usize,
    cycles_to_full_fitness: Option<usize>,
}

impl TopologyRunSummary {
    fn new(
        topology: TopologyMode,
        challenge: &str,
        requested_cycles: usize,
        enzymes: usize,
    ) -> Self {
        Self {
            topology,
            challenge: challenge.to_string(),
            requested_cycles,
            enzymes,
            elapsed_secs: 0.0,
            total_invocations: 0,
            total_mutations: 0,
            accepted_patches: 0,
            rejected_patches: 0,
            provider_failures: 0,
            killed: 0,
            invocation_capped_cycles: 0,
            wall_clock_capped_cycles: 0,
            best_fitness: 0.0,
            best_passed: 0,
            best_total: 0,
            cycles_to_full_fitness: None,
        }
    }

    fn record_cycle(&mut self, cycle_num: usize, report: &CycleReport) {
        self.total_invocations += report.invocations;
        self.total_mutations += report.accepted_mutations;
        self.accepted_patches += report.accepted_patches;
        self.rejected_patches += report.rejected_patches;
        self.provider_failures += report.failed;
        self.killed += report.killed;

        if report.capped {
            self.invocation_capped_cycles += 1;
        }
        if report.wall_clock_capped {
            self.wall_clock_capped_cycles += 1;
        }

        if let Some(fitness) = &report.fitness {
            if self.best_total == 0 || fitness.fitness > self.best_fitness {
                self.best_fitness = fitness.fitness;
                self.best_passed = fitness.passed;
                self.best_total = fitness.total;
            }
            if self.cycles_to_full_fitness.is_none()
                && fitness.total > 0
                && fitness.passed == fitness.total
            {
                self.cycles_to_full_fitness = Some(cycle_num);
            }
        }
    }

    fn full_fitness_display(&self) -> String {
        self.cycles_to_full_fitness
            .map(|cycle| cycle.to_string())
            .unwrap_or_else(|| "—".to_string())
    }

    fn caps_display(&self) -> String {
        format!(
            "{} invocation / {} wall",
            self.invocation_capped_cycles, self.wall_clock_capped_cycles
        )
    }
}

fn run_challenge(name: &str, num_cycles: usize) {
    let challenge = load_challenge_or_exit(name);

    println!("A²D Challenge: {} ({num_cycles} cycles)", challenge.name);
    println!("═══════════════════");

    let germline = load_or_seed_germline();
    let registry = build_runtime_registry(&germline);
    let mut metabolism = apply_runtime_env(
        Metabolism::new(germline, registry)
            .with_benchmark(challenge.scoring_benchmark())
            .with_project_root(project_root()),
    );

    seed_initial_runtime_artifacts(&mut metabolism, challenge.requirements);

    let lineage_dir = lineage_dir();
    let archive = LineageArchive::init(&lineage_dir).ok();

    let mut best_fitness: f64 = 0.0;

    for cycle_num in 1..=num_cycles {
        println!("\nCycle {cycle_num}/{num_cycles}...");
        let provider_policy_before_cycle =
            provider_policy_for_germline(&metabolism.provider_policy(), metabolism.germline());
        let report = metabolism.run_cycle();

        if let Some(export_dir) = fitness_evidence_export_dir() {
            match export_cycle_fitness_evidence(
                &metabolism,
                &report,
                &export_dir,
                challenge.name,
                None,
            ) {
                Ok(path) => println!("  Fitness evidence: {}", path.display()),
                Err(error) => {
                    eprintln!("  Fitness evidence export error: {error}");
                    std::process::exit(1);
                }
            }
        }

        for entry in &report.lineage {
            println!(
                "  [{} via {}] {:?}",
                entry.enzyme_id,
                entry.provider,
                match &entry.outcome {
                    a2d_core::workcell::WorkcellOutcome::Success { .. } => "OK".to_string(),
                    a2d_core::workcell::WorkcellOutcome::Failed { error } =>
                        format!("FAIL: {error}"),
                    a2d_core::workcell::WorkcellOutcome::Killed { reason } =>
                        format!("KILL: {reason:?}"),
                }
            );
        }

        let status = metabolism.germline().raf_status();
        print!(
            "  {} invocations, {} mutations, {} patches, {} provider policy changes | RAF: {:.0}%",
            report.invocations,
            report.accepted_mutations,
            report.accepted_patches,
            report.accepted_provider_policy_changes,
            status.coverage * 100.0
        );

        if let Some(ref fitness) = report.fitness {
            let delta = report.fitness_delta.unwrap_or(0.0);
            let arrow = if delta > 0.0 {
                "↑"
            } else if delta < 0.0 {
                "↓"
            } else {
                "→"
            };
            print!(
                " | Fitness: {:.0}% ({}/{}) {arrow}",
                fitness.fitness * 100.0,
                fitness.passed,
                fitness.total
            );

            if fitness.fitness > best_fitness && delta >= 0.0 {
                best_fitness = fitness.fitness;
                if let Some(ref archive) = archive {
                    match archive.commit_germline(metabolism.germline(), &report) {
                        Ok(hash) => print!(" [committed: {hash}]"),
                        Err(e) => print!(" [lineage error: {e}]"),
                    }
                }
            } else if delta < 0.0 {
                print!(" [regressed, skipped]");
            }
            if report.accepted_provider_policy_changes > 0 && delta >= 0.0 {
                if let Some(ref archive) = archive {
                    let proposed_policy = provider_policy_for_germline(
                        &metabolism.provider_policy(),
                        metabolism.germline(),
                    );
                    let gate = run_provider_policy_gate(
                        metabolism.germline().clone(),
                        challenge.name,
                        provider_policy_gate_cycles(),
                        &provider_policy_before_cycle,
                        &proposed_policy,
                    );
                    print_provider_policy_gate_summary(&gate);
                    match commit_provider_policy_if_gate_accepts(
                        archive,
                        &proposed_policy,
                        &report,
                        &gate.decision,
                    ) {
                        Ok(Some(hash)) => print!(" [policy: {hash}]"),
                        Ok(None) => print!(" [policy gate rejected: {}]", gate.decision.reason),
                        Err(e) => print!(" [policy lineage error: {e}]"),
                    }
                }
            }
        }
        if report.capped {
            print!(" [invocation-capped]");
        }
        if report.wall_clock_capped {
            print!(" [wall-clock-capped]");
        }
        println!();

        // Apply accepted system patches to the real source tree only when the
        // patch-producing cycle was grounded in actual benchmark evidence.
        if report.accepted_patches > 0 {
            if report_has_actual_fitness_evidence(&report)
                && !report.fitness_delta.is_some_and(|delta| delta < 0.0)
            {
                apply_accepted_patches(&metabolism);
            } else {
                println!(
                    "  ⚠ No non-regressing actual-test fitness evidence — skipping patch apply"
                );
            }
        }
    }

    println!("\n═══════════════════");
    println!(
        "Challenge: {} | Best fitness: {:.0}%",
        challenge.name,
        best_fitness * 100.0
    );
}

fn run_score_artifact(challenge_name: &str, artifact_path: &str) {
    let challenge = load_challenge_or_exit(challenge_name);
    let artifact = read_artifact_or_exit(artifact_path);
    let report = challenge.score_artifact(&artifact);
    print!("{}", format_score_artifact_report(challenge.name, &report));
    if let Some(export_dir) = fitness_evidence_export_dir() {
        match export_score_artifact_fitness_evidence(&report, &export_dir, challenge.name) {
            Ok(path) => println!("Fitness evidence: {}", path.display()),
            Err(error) => {
                eprintln!("Fitness evidence export error: {error}");
                std::process::exit(1);
            }
        }
    }
    let exit_code = score_artifact_exit_code(&report);
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

fn run_fitness_evidence_inspect(args: &[String]) {
    let config = parse_fitness_evidence_inspect_args(args).unwrap_or_else(|error| {
        eprintln!("{error}");
        eprintln!("Usage: a2d fitness-evidence-inspect <evidence.json> [--require-all-tests-pass]");
        std::process::exit(1);
    });
    let bytes = fs::read(&config.path).unwrap_or_else(|error| {
        eprintln!(
            "Fitness evidence inspect error: failed to read {}: {error}",
            config.path.display()
        );
        std::process::exit(1);
    });
    let value = serde_json::from_slice::<Value>(&bytes).unwrap_or_else(|error| {
        eprintln!(
            "Fitness evidence inspect error: {} is not JSON: {error}",
            config.path.display()
        );
        std::process::exit(1);
    });
    inspect_fitness_evidence_value(&value, config.require_all_tests_pass).unwrap_or_else(|error| {
        eprintln!("Fitness evidence inspect error: {error}");
        std::process::exit(1);
    });
    print_fitness_evidence_inspection(&config.path, &value);
}

struct FitnessEvidenceInspectConfig {
    path: PathBuf,
    require_all_tests_pass: bool,
}

fn parse_fitness_evidence_inspect_args(
    args: &[String],
) -> Result<FitnessEvidenceInspectConfig, String> {
    let path = args
        .first()
        .ok_or_else(|| "missing fitness evidence path".to_string())?;
    let mut require_all_tests_pass = false;
    for arg in &args[1..] {
        match arg.as_str() {
            "--require-all-tests-pass" => require_all_tests_pass = true,
            other => {
                return Err(format!(
                    "unknown fitness-evidence-inspect argument: {other}"
                ));
            }
        }
    }
    Ok(FitnessEvidenceInspectConfig {
        path: PathBuf::from(path),
        require_all_tests_pass,
    })
}

fn inspect_fitness_evidence_value(
    value: &Value,
    require_all_tests_pass: bool,
) -> Result<(), String> {
    validate_exported_fitness_evidence_value(value)?;
    if !value
        .get("actual_tests_evaluated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err("fitness evidence did not evaluate actual tests".to_string());
    }
    if !value
        .get("non_regressing")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err("fitness evidence is regressing".to_string());
    }
    if require_all_tests_pass {
        require_fitness_evidence_all_tests_pass(value)?;
    }
    Ok(())
}

fn require_fitness_evidence_all_tests_pass(value: &Value) -> Result<(), String> {
    if !fitness_evidence_result_passed(value, "all_tests_pass") {
        return Err("fitness evidence does not pass all_tests_pass".to_string());
    }
    let failed = value.get("failed").and_then(Value::as_u64).unwrap_or(1);
    let passed = value.get("passed").and_then(Value::as_u64).unwrap_or(0);
    let total = value
        .get("total")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX);
    if failed != 0 || passed != total {
        return Err(format!(
            "fitness evidence all_tests_pass is inconsistent with passed/failed totals: passed={passed}, total={total}, failed={failed}"
        ));
    }
    let results = value
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| "fitness evidence missing results array".to_string())?;
    if let Some(failed_result) = results
        .iter()
        .find(|result| result.get("passed").and_then(Value::as_bool) == Some(false))
    {
        let name = failed_result
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        return Err(format!(
            "fitness evidence all_tests_pass is inconsistent with failed result: {name}"
        ));
    }
    Ok(())
}

fn fitness_evidence_result_passed(value: &Value, name: &str) -> bool {
    value
        .get("results")
        .and_then(Value::as_array)
        .map(|results| {
            results.iter().any(|result| {
                result.get("name").and_then(Value::as_str) == Some(name)
                    && result.get("passed").and_then(Value::as_bool) == Some(true)
            })
        })
        .unwrap_or(false)
}

fn fitness_evidence_result_status(value: &Value, name: &str) -> &'static str {
    value
        .get("results")
        .and_then(Value::as_array)
        .and_then(|results| {
            results.iter().find_map(|result| {
                if result.get("name").and_then(Value::as_str) == Some(name) {
                    match result.get("passed").and_then(Value::as_bool) {
                        Some(true) => Some("true"),
                        Some(false) => Some("false"),
                        None => Some("invalid"),
                    }
                } else {
                    None
                }
            })
        })
        .unwrap_or("not_present")
}

fn print_fitness_evidence_inspection(path: &Path, value: &Value) {
    println!("Fitness evidence: {}", path.display());
    println!(
        "  schema_version: {}",
        value["schema_version"].as_str().unwrap_or("<missing>")
    );
    println!(
        "  actual_tests_evaluated: {}",
        value["actual_tests_evaluated"].as_bool().unwrap_or(false)
    );
    println!(
        "  non_regressing: {}",
        value["non_regressing"].as_bool().unwrap_or(false)
    );
    println!(
        "  fitness: {}",
        value["fitness"].as_f64().unwrap_or_default()
    );
    println!(
        "  passed/total: {}/{}",
        value["passed"].as_u64().unwrap_or_default(),
        value["total"].as_u64().unwrap_or_default()
    );
    println!(
        "  all_tests_pass: {}",
        fitness_evidence_result_passed(value, "all_tests_pass")
    );
    println!(
        "  hidden_acceptance: {}",
        fitness_evidence_result_status(value, "hidden_acceptance")
    );
    if let Some(failed_cases) = value.get("failed_cases") {
        println!("  failed_cases: {failed_cases}");
    }
    if let Some(source_diff_hash) = value.get("source_diff_hash").and_then(Value::as_str) {
        println!("  source_diff_hash: {source_diff_hash}");
    }
}

fn export_score_artifact_fitness_evidence(
    report: &a2d_core::benchmark::FitnessReport,
    export_dir: &Path,
    challenge_name: &str,
) -> Result<PathBuf, String> {
    let bytes = fitness_evidence_artifact(0, report, 0.0);
    let value = validate_exportable_fitness_evidence(&bytes, 0)?;
    let value = add_export_source_provenance(value)?;
    validate_exported_fitness_evidence_value(&value)?;
    fs::create_dir_all(export_dir).map_err(|error| {
        format!(
            "failed to create fitness evidence export dir {}: {error}",
            export_dir.display()
        )
    })?;
    let path = fitness_evidence_export_path(export_dir, challenge_name, Some("baseline"), 0, 0);
    let json = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("failed to serialize fitness evidence: {error}"))?;
    fs::write(&path, json).map_err(|error| {
        format!(
            "failed to write fitness evidence export {}: {error}",
            path.display()
        )
    })?;
    Ok(path)
}

fn read_artifact_or_exit(path_or_dash: &str) -> String {
    if path_or_dash == "-" {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .unwrap_or_else(|error| {
                eprintln!("Failed to read artifact from stdin: {error}");
                std::process::exit(1);
            });
        return input;
    }

    fs::read_to_string(path_or_dash).unwrap_or_else(|error| {
        eprintln!("Failed to read artifact {path_or_dash}: {error}");
        std::process::exit(1);
    })
}

fn run_senior_swe_bench_extract_patch(artifact_path: &str) {
    let artifact = read_artifact_or_exit(artifact_path);
    match extract_senior_swe_bench_candidate_patch(&artifact) {
        Ok(diff) => print!("{diff}"),
        Err(error) => {
            eprintln!("Senior SWE-Bench candidate patch extraction error: {error}");
            std::process::exit(1);
        }
    }
}

fn run_senior_swe_bench_diagnose_artifact(artifact_path: &str) {
    let artifact = read_artifact_or_exit(artifact_path);
    let diagnosis = diagnose_senior_swe_bench_candidate_patch_artifact(&artifact);
    println!(
        "{}",
        serde_json::to_string_pretty(&diagnosis).expect("diagnosis must serialize")
    );
}

fn run_senior_swe_bench_select_candidate_artifact(manifest_path: &str) {
    let manifest = read_artifact_or_exit(manifest_path);
    let selection = select_senior_swe_bench_candidate_artifact(&manifest).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench candidate artifact selection error: {error}");
        std::process::exit(1);
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&selection)
            .expect("candidate artifact selection must serialize")
    );
}

fn run_senior_swe_bench_cycle_input_feedback(cycle_input_path: &str, evaluation_path: &str) {
    let cycle_input = read_artifact_or_exit(cycle_input_path);
    let local_evaluation = read_artifact_or_exit(evaluation_path);
    let feedback = build_senior_swe_bench_cycle_input_feedback(&cycle_input, &local_evaluation)
        .unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench cycle input feedback error: {error}");
            std::process::exit(1);
        });
    println!(
        "{}",
        serde_json::to_string_pretty(&feedback)
            .expect("Senior SWE-Bench cycle input feedback must serialize")
    );
}

fn run_senior_swe_bench_retry_plan(cycle_input_path: &str, max_attempts: usize) {
    let cycle_input = read_artifact_or_exit(cycle_input_path);
    let plan = build_senior_swe_bench_cycle_retry_plan(&cycle_input, max_attempts).unwrap_or_else(
        |error| {
            eprintln!("Senior SWE-Bench retry plan error: {error}");
            std::process::exit(1);
        },
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&plan).expect("Senior SWE-Bench retry plan must serialize")
    );
}

fn run_senior_swe_bench_retry_attempt_plan(args: &[String]) {
    let config = parse_senior_swe_bench_retry_attempt_plan_args(args).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench retry attempt plan error: {error}");
        eprintln!("Usage: a2d senior-swe-bench-retry-attempt-plan --retry-plan <json|-> --attempt-index <n> --task-cycle-input <json|-> --cycle-output-manifest <json|-> --checkout <dir> --attempt-dir <dir> [--apply-candidate-patch] [--official-evaluator-manifest <json> --official-evaluator-manifest-inspection <json>] -- <evaluator> [args...]");
        std::process::exit(1);
    });
    let plan = build_senior_swe_bench_retry_attempt_plan(&config).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench retry attempt plan error: {error}");
        std::process::exit(1);
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&plan)
            .expect("Senior SWE-Bench retry attempt plan must serialize")
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeniorSweBenchRetryAttemptPlanConfig {
    retry_plan: PathBuf,
    attempt_index: usize,
    task_cycle_input: PathBuf,
    cycle_output_manifest: PathBuf,
    checkout: PathBuf,
    attempt_dir: PathBuf,
    apply_candidate_patch: bool,
    official_evaluator_manifest: Option<PathBuf>,
    official_evaluator_manifest_inspection: Option<PathBuf>,
    evaluator_command: Vec<String>,
}

fn parse_senior_swe_bench_retry_attempt_plan_args(
    args: &[String],
) -> Result<SeniorSweBenchRetryAttemptPlanConfig, String> {
    let mut retry_plan = None;
    let mut attempt_index = None;
    let mut task_cycle_input = None;
    let mut cycle_output_manifest = None;
    let mut checkout = None;
    let mut attempt_dir = None;
    let mut apply_candidate_patch = false;
    let mut official_evaluator_manifest = None;
    let mut official_evaluator_manifest_inspection = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--" => {
                let evaluator_command = args[index + 1..].to_vec();
                if evaluator_command.is_empty() {
                    return Err(
                        "Senior SWE-Bench retry attempt evaluator command is empty".to_string()
                    );
                }
                let config = SeniorSweBenchRetryAttemptPlanConfig {
                    retry_plan: retry_plan.ok_or_else(|| "missing --retry-plan".to_string())?,
                    attempt_index: attempt_index
                        .ok_or_else(|| "missing --attempt-index".to_string())?,
                    task_cycle_input: task_cycle_input
                        .ok_or_else(|| "missing --task-cycle-input".to_string())?,
                    cycle_output_manifest: cycle_output_manifest
                        .ok_or_else(|| "missing --cycle-output-manifest".to_string())?,
                    checkout: checkout.ok_or_else(|| "missing --checkout".to_string())?,
                    attempt_dir: attempt_dir.ok_or_else(|| "missing --attempt-dir".to_string())?,
                    apply_candidate_patch,
                    official_evaluator_manifest,
                    official_evaluator_manifest_inspection,
                    evaluator_command,
                };
                validate_retry_attempt_plan_stdin_inputs(&config)?;
                return Ok(config);
            }
            "--retry-plan" => {
                index += 1;
                retry_plan = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--retry-plan requires a path".to_string())?,
                ));
            }
            "--attempt-index" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--attempt-index requires a value".to_string())?;
                attempt_index = Some(value.parse::<usize>().map_err(|error| {
                    format!("--attempt-index must be a non-negative integer: {error}")
                })?);
            }
            "--task-cycle-input" => {
                index += 1;
                task_cycle_input =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--task-cycle-input requires a path".to_string()
                    })?));
            }
            "--cycle-output-manifest" => {
                index += 1;
                cycle_output_manifest =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--cycle-output-manifest requires a path".to_string()
                    })?));
            }
            "--checkout" => {
                index += 1;
                checkout = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--checkout requires a path".to_string())?,
                ));
            }
            "--attempt-dir" => {
                index += 1;
                attempt_dir =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--attempt-dir requires a path".to_string()
                    })?));
            }
            "--apply-candidate-patch" => apply_candidate_patch = true,
            "--official-evaluator-manifest" => {
                index += 1;
                official_evaluator_manifest =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--official-evaluator-manifest requires a path".to_string()
                    })?));
            }
            "--official-evaluator-manifest-inspection" => {
                index += 1;
                official_evaluator_manifest_inspection =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--official-evaluator-manifest-inspection requires a path".to_string()
                    })?));
            }
            other => {
                return Err(format!(
                    "unknown senior-swe-bench-retry-attempt-plan argument: {other}"
                ));
            }
        }
        index += 1;
    }
    Err("missing -- <evaluator> command".to_string())
}

fn validate_retry_attempt_plan_stdin_inputs(
    config: &SeniorSweBenchRetryAttemptPlanConfig,
) -> Result<(), String> {
    let stdin_inputs = [
        ("--retry-plan", &config.retry_plan),
        ("--task-cycle-input", &config.task_cycle_input),
        ("--cycle-output-manifest", &config.cycle_output_manifest),
    ]
    .into_iter()
    .filter(|(_, path)| path.as_path() == Path::new("-"))
    .map(|(flag, _)| flag)
    .collect::<Vec<_>>();
    if stdin_inputs.len() > 1 {
        return Err(format!(
            "at most one retry-attempt-plan input may be read from stdin (-); got {}",
            stdin_inputs.join(", ")
        ));
    }
    Ok(())
}

fn validate_official_manifest_inspection_for_retry_plan(
    package: &SeniorSweBenchTaskPackageSummary,
    manifest_path: &Path,
    inspection_path: &Path,
    evaluator_command: &[String],
) -> Result<(), String> {
    let inspection_text = read_artifact_to_string(inspection_path)?;
    let inspection: Value = serde_json::from_str(&inspection_text).map_err(|error| {
        format!("invalid Senior SWE-Bench official evaluator manifest inspection JSON: {error}")
    })?;
    validate_retry_execute_official_manifest_inspection(
        &inspection,
        package,
        manifest_path,
        evaluator_command,
    )?;
    Ok(())
}

fn build_senior_swe_bench_retry_attempt_plan(
    config: &SeniorSweBenchRetryAttemptPlanConfig,
) -> Result<Value, String> {
    let retry_plan = read_artifact_to_string(&config.retry_plan)?;
    let cycle_input = read_artifact_to_string(&config.task_cycle_input)?;
    let (retry_plan_value, package) =
        validate_senior_swe_bench_retry_plan_and_cycle_input_for_attempt(
            &retry_plan,
            config.attempt_index,
            &cycle_input,
        )?;
    if !config.checkout.is_dir() {
        return Err(format!(
            "Senior SWE-Bench checkout directory not found: {}",
            config.checkout.display()
        ));
    }
    match (
        &config.official_evaluator_manifest,
        &config.official_evaluator_manifest_inspection,
    ) {
        (Some(manifest), Some(inspection)) => {
            if manifest == Path::new("-") {
                return Err(
                    "Senior SWE-Bench official evaluator manifest must be a file path, not stdin"
                        .to_string(),
                );
            }
            if inspection == Path::new("-") {
                return Err(
                    "Senior SWE-Bench official evaluator manifest inspection must be a file path, not stdin"
                        .to_string(),
                );
            }
            if !manifest.is_file() {
                return Err(format!(
                    "Senior SWE-Bench official evaluator manifest not found: {}",
                    manifest.display()
                ));
            }
            if !inspection.is_file() {
                return Err(format!(
                    "Senior SWE-Bench official evaluator manifest inspection not found: {}",
                    inspection.display()
                ));
            }
            validate_official_manifest_inspection_for_retry_plan(
                &package,
                manifest,
                inspection,
                &config.evaluator_command,
            )?;
        }
        (Some(_), None) => {
            return Err("Senior SWE-Bench retry attempt plan requires --official-evaluator-manifest-inspection when --official-evaluator-manifest is supplied; run senior-swe-bench-official-evaluator-manifest-inspect first".to_string());
        }
        (None, Some(_)) => {
            return Err("Senior SWE-Bench retry attempt plan --official-evaluator-manifest-inspection requires --official-evaluator-manifest".to_string());
        }
        (None, None) => {}
    }
    let manifest = read_artifact_to_string(&config.cycle_output_manifest)?;
    let selection = select_senior_swe_bench_candidate_artifact(&manifest)?;
    let selected_path = selection
        .get("selected")
        .and_then(|selected| selected.get("path"))
        .and_then(Value::as_str)
        .ok_or_else(|| "candidate artifact selection missing selected.path".to_string())?
        .to_string();
    let artifact = fs::read_to_string(&selected_path).map_err(|error| {
        format!("failed to read selected candidate artifact {selected_path}: {error}")
    })?;
    let candidate_patch = match extract_senior_swe_bench_candidate_patch(&artifact) {
        Ok(patch) => patch,
        Err(error) => {
            return Ok(json!({
                "schema_version": "a2d.senior-swe-bench-retry-attempt-plan.v1",
                "task_id": package.task_id,
                "repo": package.repo,
                "attempt_index": config.attempt_index,
                "max_attempts": retry_plan_value["max_attempts"].clone(),
                "task_cycle_input": config.task_cycle_input.display().to_string(),
                "cycle_output_manifest": config.cycle_output_manifest.display().to_string(),
                "attempt_dir": config.attempt_dir.display().to_string(),
                "decision": "stop",
                "stop_reason": "candidate_patch_extraction_failed",
                "extraction_error": error,
                "candidate_selection": selection,
                "provider_invocations_started": false,
                "evaluator_invocations_started": false,
                "fitness_claim_allowed_before_evidence": false,
                "github_solution_search_allowed": false,
                "note": "deterministic retry-attempt planning only: candidate artifact is not extractable, so no evaluator args are emitted",
            }));
        }
    };
    let candidate_patch_hash = git_hash_object_bytes(candidate_patch.as_bytes())?;
    let candidate_patch_path = config.attempt_dir.join("candidate.patch");
    let local_evaluation_path = config.attempt_dir.join("local-evaluation.json");
    let mut evaluate_args = vec![
        "senior-swe-bench-evaluate".to_string(),
        "--task-cycle-input".to_string(),
        config.task_cycle_input.display().to_string(),
        "--candidate-patch-artifact".to_string(),
        selected_path.clone(),
        "--extracted-candidate-patch".to_string(),
        candidate_patch_path.display().to_string(),
        "--checkout".to_string(),
        config.checkout.display().to_string(),
    ];
    if config.apply_candidate_patch {
        evaluate_args.push("--apply-candidate-patch".to_string());
    }
    if let Some(manifest) = &config.official_evaluator_manifest {
        evaluate_args.push("--official-evaluator-manifest".to_string());
        evaluate_args.push(manifest.display().to_string());
    }
    if let Some(inspection) = &config.official_evaluator_manifest_inspection {
        evaluate_args.push("--official-evaluator-manifest-inspection".to_string());
        evaluate_args.push(inspection.display().to_string());
    }
    evaluate_args.push("--output".to_string());
    evaluate_args.push(local_evaluation_path.display().to_string());
    evaluate_args.push("--".to_string());
    evaluate_args.extend(config.evaluator_command.clone());

    Ok(json!({
        "schema_version": "a2d.senior-swe-bench-retry-attempt-plan.v1",
        "task_id": package.task_id,
        "repo": package.repo,
        "attempt_index": config.attempt_index,
        "max_attempts": retry_plan_value["max_attempts"].clone(),
        "task_cycle_input": config.task_cycle_input.display().to_string(),
        "cycle_output_manifest": config.cycle_output_manifest.display().to_string(),
        "attempt_dir": config.attempt_dir.display().to_string(),
        "decision": "extract_and_evaluate_candidate_patch",
        "candidate_selection": selection,
        "selected_artifact": selection["selected"].clone(),
        "contains_unified_diff_candidate_patch": true,
        "candidate_patch_hash": candidate_patch_hash,
        "planned_outputs": {
            "candidate_patch": candidate_patch_path,
            "local_evaluation": local_evaluation_path
        },
        "extract_patch_args": [
            "senior-swe-bench-extract-patch",
            selected_path
        ],
        "evaluate_args": evaluate_args,
        "retry_step_args": [
            "senior-swe-bench-retry-step",
            "--retry-plan",
            config.retry_plan.display().to_string(),
            "--attempt-index",
            config.attempt_index.to_string(),
            "--task-cycle-input",
            config.task_cycle_input.display().to_string(),
            "--local-evaluation",
            local_evaluation_path.display().to_string()
        ],
        "apply_candidate_patch": config.apply_candidate_patch,
        "official_evaluator_manifest": config.official_evaluator_manifest.as_ref().map(|path| path.display().to_string()),
        "official_evaluator_manifest_inspection": config.official_evaluator_manifest_inspection.as_ref().map(|path| path.display().to_string()),
        "provider_invocations_started": false,
        "evaluator_invocations_started": false,
        "fitness_claim_allowed_before_evidence": false,
        "github_solution_search_allowed": false,
        "note": "deterministic retry-attempt planning only: this command starts no providers/evaluators, writes no attempt files, and is not fitness evidence",
    }))
}

fn read_artifact_to_string(path: &Path) -> Result<String, String> {
    if path == Path::new("-") {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|error| format!("failed to read artifact from stdin: {error}"))?;
        Ok(input)
    } else {
        fs::read_to_string(path)
            .map_err(|error| format!("failed to read artifact {}: {error}", path.display()))
    }
}

fn run_senior_swe_bench_retry_attempt_extract_patch(args: &[String]) {
    if args.len() != 1 {
        eprintln!(
            "Usage: a2d senior-swe-bench-retry-attempt-extract-patch <retry-attempt-plan.json|->"
        );
        std::process::exit(1);
    }
    let plan = read_artifact_or_exit(&args[0]);
    let extraction =
        build_senior_swe_bench_retry_attempt_extraction(&plan).unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench retry attempt extraction error: {error}");
            std::process::exit(1);
        });
    println!(
        "{}",
        serde_json::to_string_pretty(&extraction)
            .expect("Senior SWE-Bench retry attempt extraction must serialize")
    );
}

fn build_senior_swe_bench_retry_attempt_extraction(plan: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(plan)
        .map_err(|error| format!("invalid Senior SWE-Bench retry attempt plan JSON: {error}"))?;
    let schema = value
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry attempt plan missing schema_version".to_string())?;
    if schema != "a2d.senior-swe-bench-retry-attempt-plan.v1" {
        return Err(format!(
            "expected a2d.senior-swe-bench-retry-attempt-plan.v1, got {schema}"
        ));
    }
    let decision = value
        .get("decision")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry attempt plan missing decision".to_string())?;
    if decision != "extract_and_evaluate_candidate_patch" {
        return Err(format!(
            "Senior SWE-Bench retry attempt plan decision must be extract_and_evaluate_candidate_patch, got {decision}"
        ));
    }
    if value
        .get("provider_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt plan must not have started provider invocations"
                .to_string(),
        );
    }
    if value
        .get("evaluator_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt plan must not have started evaluator invocations"
                .to_string(),
        );
    }
    if value
        .get("fitness_claim_allowed_before_evidence")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt plan must forbid fitness claims before evidence"
                .to_string(),
        );
    }

    let selected = value.get("selected_artifact").ok_or_else(|| {
        "Senior SWE-Bench retry attempt plan missing selected_artifact".to_string()
    })?;
    let selected_path = required_plan_string(selected, "path")?;
    let selected_hash = required_plan_string(selected, "git_object_hash")?;
    validate_git_object_hash(&selected_hash).map_err(|error| {
        format!("Senior SWE-Bench retry attempt selected artifact git_object_hash {error}: {selected_hash}")
    })?;
    let selected_bytes = selected
        .get("bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt selected artifact missing bytes".to_string()
        })?;
    let artifact_bytes = fs::read(&selected_path).map_err(|error| {
        format!("failed to read selected candidate artifact {selected_path}: {error}")
    })?;
    if artifact_bytes.len() as u64 != selected_bytes {
        return Err(format!(
            "selected candidate artifact byte count mismatch for {selected_path}: plan {selected_bytes}, actual {}",
            artifact_bytes.len()
        ));
    }
    let actual_artifact_hash = git_hash_object_bytes(&artifact_bytes)?;
    if actual_artifact_hash != selected_hash {
        return Err(format!(
            "selected candidate artifact hash mismatch for {selected_path}: plan {selected_hash}, actual {actual_artifact_hash}"
        ));
    }
    let artifact = String::from_utf8(artifact_bytes).map_err(|error| {
        format!("selected candidate artifact {selected_path} is not UTF-8 text: {error}")
    })?;
    let candidate_patch = extract_senior_swe_bench_candidate_patch(&artifact)?;
    let actual_patch_hash = git_hash_object_bytes(candidate_patch.as_bytes())?;
    let planned_patch_hash = required_plan_string(&value, "candidate_patch_hash")?;
    validate_git_object_hash(&planned_patch_hash).map_err(|error| {
        format!("Senior SWE-Bench retry attempt candidate_patch_hash {error}: {planned_patch_hash}")
    })?;
    if actual_patch_hash != planned_patch_hash {
        return Err(format!(
            "candidate patch hash mismatch: plan {planned_patch_hash}, actual {actual_patch_hash}"
        ));
    }
    let planned_outputs = value
        .get("planned_outputs")
        .ok_or_else(|| "Senior SWE-Bench retry attempt plan missing planned_outputs".to_string())?;
    let candidate_patch_path = required_plan_string(planned_outputs, "candidate_patch")?;
    let candidate_patch_path = PathBuf::from(candidate_patch_path);
    write_candidate_patch_idempotently(&candidate_patch_path, candidate_patch.as_bytes())?;

    Ok(json!({
        "schema_version": "a2d.senior-swe-bench-retry-attempt-extraction.v1",
        "task_id": value.get("task_id").cloned().unwrap_or(Value::Null),
        "repo": value.get("repo").cloned().unwrap_or(Value::Null),
        "attempt_index": value.get("attempt_index").cloned().unwrap_or(Value::Null),
        "candidate_patch_path": candidate_patch_path,
        "candidate_patch_hash": actual_patch_hash,
        "selected_artifact": selected.clone(),
        "evaluate_args": value.get("evaluate_args").cloned().unwrap_or_else(|| json!([])),
        "retry_step_args": value.get("retry_step_args").cloned().unwrap_or_else(|| json!([])),
        "provider_invocations_started": false,
        "evaluator_invocations_started": false,
        "fitness_claim_allowed_before_evidence": false,
        "github_solution_search_allowed": false,
        "next_step": "run the emitted senior-swe-bench-evaluate command, then run the emitted senior-swe-bench-retry-step command",
        "note": "deterministic retry-attempt extraction only: this command starts no providers/evaluators and is not fitness evidence",
    }))
}

fn run_senior_swe_bench_retry_attempt_evaluate(args: &[String]) {
    if args.len() != 1 {
        eprintln!(
            "Usage: a2d senior-swe-bench-retry-attempt-evaluate <retry-attempt-extraction.json|->"
        );
        std::process::exit(1);
    }
    let extraction = read_artifact_or_exit(&args[0]);
    let evaluation =
        build_senior_swe_bench_retry_attempt_evaluation(&extraction).unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench retry attempt evaluation error: {error}");
            std::process::exit(1);
        });
    println!(
        "{}",
        serde_json::to_string_pretty(&evaluation)
            .expect("Senior SWE-Bench retry attempt evaluation must serialize")
    );
}

fn build_senior_swe_bench_retry_attempt_evaluation(extraction: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(extraction).map_err(|error| {
        format!("invalid Senior SWE-Bench retry attempt extraction JSON: {error}")
    })?;
    let schema = value
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt extraction missing schema_version".to_string()
        })?;
    if schema != "a2d.senior-swe-bench-retry-attempt-extraction.v1" {
        return Err(format!(
            "expected a2d.senior-swe-bench-retry-attempt-extraction.v1, got {schema}"
        ));
    }
    if value
        .get("provider_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt extraction must not have started provider invocations"
                .to_string(),
        );
    }
    if value
        .get("evaluator_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt extraction must not have started evaluator invocations"
                .to_string(),
        );
    }
    if value
        .get("fitness_claim_allowed_before_evidence")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt extraction must forbid fitness claims before evidence"
                .to_string(),
        );
    }
    if value
        .get("github_solution_search_allowed")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt extraction must forbid public GitHub solution search"
                .to_string(),
        );
    }

    let candidate_patch_path = PathBuf::from(required_plan_string(&value, "candidate_patch_path")?);
    let planned_patch_hash = required_plan_string(&value, "candidate_patch_hash")?;
    validate_git_object_hash(&planned_patch_hash).map_err(|error| {
        format!("Senior SWE-Bench retry attempt candidate_patch_hash {error}: {planned_patch_hash}")
    })?;
    let actual_patch_hash = file_content_hash(&candidate_patch_path)?;
    if actual_patch_hash != planned_patch_hash {
        return Err(format!(
            "candidate patch hash mismatch: extraction {planned_patch_hash}, actual {actual_patch_hash}"
        ));
    }

    let selected = value.get("selected_artifact").ok_or_else(|| {
        "Senior SWE-Bench retry attempt extraction missing selected_artifact".to_string()
    })?;
    let selected_path = required_plan_string(selected, "path")?;
    let selected_hash = required_plan_string(selected, "git_object_hash")?;
    validate_git_object_hash(&selected_hash).map_err(|error| {
        format!("Senior SWE-Bench retry attempt selected artifact git_object_hash {error}: {selected_hash}")
    })?;
    let selected_bytes = selected
        .get("bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt selected artifact missing bytes".to_string()
        })?;
    let artifact_bytes = fs::read(&selected_path).map_err(|error| {
        format!("failed to read selected candidate artifact {selected_path}: {error}")
    })?;
    if artifact_bytes.len() as u64 != selected_bytes {
        return Err(format!(
            "selected candidate artifact byte count mismatch for {selected_path}: extraction {selected_bytes}, actual {}",
            artifact_bytes.len()
        ));
    }
    let actual_artifact_hash = git_hash_object_bytes(&artifact_bytes)?;
    if actual_artifact_hash != selected_hash {
        return Err(format!(
            "selected candidate artifact hash mismatch for {selected_path}: extraction {selected_hash}, actual {actual_artifact_hash}"
        ));
    }
    let artifact = String::from_utf8(artifact_bytes).map_err(|error| {
        format!("selected candidate artifact {selected_path} is not UTF-8 text: {error}")
    })?;
    let extracted_patch = extract_senior_swe_bench_candidate_patch(&artifact)?;
    let extracted_hash = git_hash_object_bytes(extracted_patch.as_bytes())?;
    if extracted_hash != planned_patch_hash {
        return Err(format!(
            "candidate patch hash mismatch after re-extraction: extraction {planned_patch_hash}, actual {extracted_hash}"
        ));
    }

    let evaluate_args = value
        .get("evaluate_args")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt extraction missing evaluate_args".to_string()
        })?
        .iter()
        .map(|arg| {
            arg.as_str().map(ToString::to_string).ok_or_else(|| {
                "Senior SWE-Bench retry attempt evaluate_args contains non-string".to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let evaluate_config = validate_retry_attempt_evaluate_args(
        &evaluate_args,
        &selected_path,
        &candidate_patch_path,
    )?;
    let local_evaluation_path = evaluate_config
        .output
        .as_ref()
        .expect("retry-attempt evaluate validation requires output")
        .to_string_lossy()
        .to_string();
    let retry_step_args = value
        .get("retry_step_args")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt extraction missing retry_step_args".to_string()
        })?
        .iter()
        .map(|arg| {
            arg.as_str().map(ToString::to_string).ok_or_else(|| {
                "Senior SWE-Bench retry attempt retry_step_args contains non-string".to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_retry_attempt_retry_step_args(
        &retry_step_args,
        &value,
        &evaluate_config,
        &local_evaluation_path,
    )?;

    let current_exe = env::current_exe()
        .map_err(|error| format!("failed to resolve current a2d executable: {error}"))?;
    let output = Command::new(current_exe)
        .args(&evaluate_args)
        .output()
        .map_err(|error| {
            format!("failed to run planned senior-swe-bench-evaluate command: {error}")
        })?;
    let exit_code = output.status.code();
    if exit_code != Some(0) && exit_code != Some(2) {
        return Err(format!(
            "planned senior-swe-bench-evaluate failed with exit {:?}: stdout={} stderr={}",
            exit_code,
            preview_text_lossy(&output.stdout),
            preview_text_lossy(&output.stderr)
        ));
    }

    let local_evaluation = validate_retry_attempt_local_evaluation(
        &PathBuf::from(&local_evaluation_path),
        &value,
        &candidate_patch_path,
        &planned_patch_hash,
        &evaluate_config,
        exit_code,
    )?;
    let local_status = local_evaluation
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("<invalid>")
        .to_string();
    let fitness_evidence_path = local_evaluation
        .get("fitness_evidence_path")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    let mut result = json!({
        "schema_version": "a2d.senior-swe-bench-retry-attempt-evaluation.v1",
        "task_id": value.get("task_id").cloned().unwrap_or(Value::Null),
        "repo": value.get("repo").cloned().unwrap_or(Value::Null),
        "attempt_index": value.get("attempt_index").cloned().unwrap_or(Value::Null),
        "candidate_patch_path": candidate_patch_path,
        "candidate_patch_hash": planned_patch_hash,
        "selected_artifact": selected.clone(),
        "evaluate_args": evaluate_args,
        "retry_step_args": retry_step_args,
        "evaluate_exit_code": exit_code,
        "local_evaluation_path": local_evaluation_path,
        "local_evaluation_status": local_status,
        "provider_invocations_started": false,
        "evaluator_invocations_started": true,
        "retry_step_started": false,
        "fitness_evidence_inspection_started": false,
        "fitness_claim_allowed_before_evidence": false,
        "github_solution_search_allowed": false,
        "next_step": "run the emitted senior-swe-bench-retry-step command; inspect fitness evidence only if retry-step says to",
        "note": "deterministic retry-attempt evaluation only: this command runs exactly one planned evaluator wrapper command and is not a fitness claim",
    });
    if let Some(path) = fitness_evidence_path {
        result["fitness_evidence_path"] = Value::String(path);
    }
    Ok(result)
}

fn run_senior_swe_bench_retry_attempt_step(args: &[String]) {
    if args.len() != 1 {
        eprintln!(
            "Usage: a2d senior-swe-bench-retry-attempt-step <retry-attempt-evaluation.json|->"
        );
        std::process::exit(1);
    }
    let evaluation = read_artifact_or_exit(&args[0]);
    let step_execution = build_senior_swe_bench_retry_attempt_step_execution(&evaluation)
        .unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench retry attempt step error: {error}");
            std::process::exit(1);
        });
    println!(
        "{}",
        serde_json::to_string_pretty(&step_execution)
            .expect("Senior SWE-Bench retry attempt step execution must serialize")
    );
}

fn build_senior_swe_bench_retry_attempt_step_execution(evaluation: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(evaluation).map_err(|error| {
        format!("invalid Senior SWE-Bench retry attempt evaluation JSON: {error}")
    })?;
    let schema = value
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt evaluation missing schema_version".to_string()
        })?;
    if schema != "a2d.senior-swe-bench-retry-attempt-evaluation.v1" {
        return Err(format!(
            "expected a2d.senior-swe-bench-retry-attempt-evaluation.v1, got {schema}"
        ));
    }
    if value
        .get("provider_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt evaluation must not have started provider invocations"
                .to_string(),
        );
    }
    if value
        .get("evaluator_invocations_started")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(
            "Senior SWE-Bench retry attempt evaluation must have started exactly one evaluator invocation"
                .to_string(),
        );
    }
    if value.get("retry_step_started").and_then(Value::as_bool) != Some(false) {
        return Err(
            "Senior SWE-Bench retry attempt evaluation must not have started retry-step"
                .to_string(),
        );
    }
    if value
        .get("fitness_evidence_inspection_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt evaluation must not have inspected fitness evidence"
                .to_string(),
        );
    }
    if value
        .get("fitness_claim_allowed_before_evidence")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt evaluation must forbid fitness claims before evidence"
                .to_string(),
        );
    }
    if value
        .get("github_solution_search_allowed")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt evaluation must forbid public GitHub solution search"
                .to_string(),
        );
    }

    let candidate_patch_path = PathBuf::from(required_plan_string(&value, "candidate_patch_path")?);
    let planned_patch_hash = required_plan_string(&value, "candidate_patch_hash")?;
    validate_git_object_hash(&planned_patch_hash).map_err(|error| {
        format!("Senior SWE-Bench retry attempt candidate_patch_hash {error}: {planned_patch_hash}")
    })?;
    let actual_patch_hash = file_content_hash(&candidate_patch_path)?;
    if actual_patch_hash != planned_patch_hash {
        return Err(format!(
            "candidate patch hash mismatch: evaluation {planned_patch_hash}, actual {actual_patch_hash}"
        ));
    }

    let selected = value.get("selected_artifact").ok_or_else(|| {
        "Senior SWE-Bench retry attempt evaluation missing selected_artifact".to_string()
    })?;
    let selected_path = required_plan_string(selected, "path")?;
    let selected_hash = required_plan_string(selected, "git_object_hash")?;
    validate_git_object_hash(&selected_hash).map_err(|error| {
        format!("Senior SWE-Bench retry attempt selected artifact git_object_hash {error}: {selected_hash}")
    })?;
    let selected_bytes = selected
        .get("bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt selected artifact missing bytes".to_string()
        })?;
    let artifact_bytes = fs::read(&selected_path).map_err(|error| {
        format!("failed to read selected candidate artifact {selected_path}: {error}")
    })?;
    if artifact_bytes.len() as u64 != selected_bytes {
        return Err(format!(
            "selected candidate artifact byte count mismatch for {selected_path}: evaluation {selected_bytes}, actual {}",
            artifact_bytes.len()
        ));
    }
    let actual_artifact_hash = git_hash_object_bytes(&artifact_bytes)?;
    if actual_artifact_hash != selected_hash {
        return Err(format!(
            "selected candidate artifact hash mismatch for {selected_path}: evaluation {selected_hash}, actual {actual_artifact_hash}"
        ));
    }
    let artifact = String::from_utf8(artifact_bytes).map_err(|error| {
        format!("selected candidate artifact {selected_path} is not UTF-8 text: {error}")
    })?;
    let extracted_patch = extract_senior_swe_bench_candidate_patch(&artifact)?;
    let extracted_hash = git_hash_object_bytes(extracted_patch.as_bytes())?;
    if extracted_hash != planned_patch_hash {
        return Err(format!(
            "candidate patch hash mismatch after re-extraction: evaluation {planned_patch_hash}, actual {extracted_hash}"
        ));
    }

    let evaluate_args = value
        .get("evaluate_args")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt evaluation missing evaluate_args".to_string()
        })?
        .iter()
        .map(|arg| {
            arg.as_str().map(ToString::to_string).ok_or_else(|| {
                "Senior SWE-Bench retry attempt evaluate_args contains non-string".to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let evaluate_config = validate_retry_attempt_evaluate_args(
        &evaluate_args,
        &selected_path,
        &candidate_patch_path,
    )?;
    let local_evaluation_path = evaluate_config
        .output
        .as_ref()
        .expect("retry-attempt evaluate validation requires output")
        .to_string_lossy()
        .to_string();
    if value.get("local_evaluation_path").and_then(Value::as_str)
        != Some(local_evaluation_path.as_str())
    {
        return Err(
            "Senior SWE-Bench retry attempt evaluation local_evaluation_path does not match evaluate_args"
                .to_string(),
        );
    }
    let retry_step_args = value
        .get("retry_step_args")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt evaluation missing retry_step_args".to_string()
        })?
        .iter()
        .map(|arg| {
            arg.as_str().map(ToString::to_string).ok_or_else(|| {
                "Senior SWE-Bench retry attempt retry_step_args contains non-string".to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_retry_attempt_retry_step_args(
        &retry_step_args,
        &value,
        &evaluate_config,
        &local_evaluation_path,
    )?;

    let evaluate_exit_code = value
        .get("evaluate_exit_code")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt evaluation missing evaluate_exit_code".to_string()
        })?;
    if evaluate_exit_code != 0 && evaluate_exit_code != 2 {
        return Err(format!(
            "Senior SWE-Bench retry attempt evaluation has unsupported evaluate_exit_code {evaluate_exit_code}"
        ));
    }
    let local_evaluation = validate_retry_attempt_local_evaluation(
        &PathBuf::from(&local_evaluation_path),
        &value,
        &candidate_patch_path,
        &planned_patch_hash,
        &evaluate_config,
        Some(evaluate_exit_code as i32),
    )?;
    let local_status = local_evaluation
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("<invalid>")
        .to_string();
    if value.get("local_evaluation_status").and_then(Value::as_str) != Some(local_status.as_str()) {
        return Err("Senior SWE-Bench retry attempt evaluation local_evaluation_status does not match local evaluation".to_string());
    }

    let current_exe = env::current_exe()
        .map_err(|error| format!("failed to resolve current a2d executable: {error}"))?;
    let output = Command::new(current_exe)
        .args(&retry_step_args)
        .output()
        .map_err(|error| {
            format!("failed to run planned senior-swe-bench-retry-step command: {error}")
        })?;
    if !output.status.success() {
        return Err(format!(
            "planned senior-swe-bench-retry-step failed with exit {:?}: stdout={} stderr={}",
            output.status.code(),
            preview_text_lossy(&output.stdout),
            preview_text_lossy(&output.stderr)
        ));
    }
    let retry_step: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "planned senior-swe-bench-retry-step wrote invalid JSON: {error}; stdout={}",
            preview_text_lossy(&output.stdout)
        )
    })?;
    validate_retry_attempt_step_output(&retry_step, &value, &local_evaluation, &local_status)?;

    Ok(json!({
        "schema_version": "a2d.senior-swe-bench-retry-attempt-step-execution.v1",
        "task_id": value.get("task_id").cloned().unwrap_or(Value::Null),
        "repo": value.get("repo").cloned().unwrap_or(Value::Null),
        "attempt_index": value.get("attempt_index").cloned().unwrap_or(Value::Null),
        "candidate_patch_path": candidate_patch_path,
        "candidate_patch_hash": planned_patch_hash,
        "selected_artifact": selected.clone(),
        "evaluate_args": evaluate_args,
        "retry_step_args": retry_step_args,
        "evaluate_exit_code": evaluate_exit_code,
        "local_evaluation_path": local_evaluation_path,
        "local_evaluation_status": local_status,
        "retry_step": retry_step,
        "provider_invocations_started": false,
        "evaluator_invocations_started": false,
        "prior_evaluator_invocations_started": true,
        "retry_step_started": true,
        "fitness_evidence_inspection_started": false,
        "fitness_claim_allowed_before_evidence": false,
        "github_solution_search_allowed": false,
        "next_step": "follow the embedded retry_step decision; run fitness-evidence-inspect only if retry_step says to",
        "note": "deterministic retry-attempt step execution only: this command runs exactly one planned retry-step command and does not inspect fitness evidence or claim fitness",
    }))
}

fn run_senior_swe_bench_retry_attempt_step_evidence(args: &[String]) {
    if args.len() != 1 {
        eprintln!(
            "Usage: a2d senior-swe-bench-retry-attempt-step-evidence <retry-attempt-step-execution.json|->"
        );
        std::process::exit(1);
    }
    let step_execution = read_artifact_or_exit(&args[0]);
    let evidence_execution =
        build_senior_swe_bench_retry_attempt_step_evidence_execution(&step_execution)
            .unwrap_or_else(|error| {
                eprintln!("Senior SWE-Bench retry attempt step evidence error: {error}");
                std::process::exit(1);
            });
    println!(
        "{}",
        serde_json::to_string_pretty(&evidence_execution)
            .expect("Senior SWE-Bench retry attempt evidence execution must serialize")
    );
}

fn build_senior_swe_bench_retry_attempt_step_evidence_execution(
    step_execution: &str,
) -> Result<Value, String> {
    let value: Value = serde_json::from_str(step_execution).map_err(|error| {
        format!("invalid Senior SWE-Bench retry attempt step execution JSON: {error}")
    })?;
    let schema = value
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt step execution missing schema_version".to_string()
        })?;
    if schema != "a2d.senior-swe-bench-retry-attempt-step-execution.v1" {
        return Err(format!(
            "expected a2d.senior-swe-bench-retry-attempt-step-execution.v1, got {schema}"
        ));
    }
    if value
        .get("provider_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt step execution must not have started providers"
                .to_string(),
        );
    }
    if value
        .get("evaluator_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt step execution must not start evaluators".to_string(),
        );
    }
    if value
        .get("prior_evaluator_invocations_started")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(
            "Senior SWE-Bench retry attempt step execution must record prior evaluator execution"
                .to_string(),
        );
    }
    if value.get("retry_step_started").and_then(Value::as_bool) != Some(true) {
        return Err(
            "Senior SWE-Bench retry attempt step execution must have started retry-step"
                .to_string(),
        );
    }
    if value
        .get("fitness_evidence_inspection_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry attempt step execution must not have inspected fitness evidence"
                .to_string(),
        );
    }
    if value
        .get("fitness_claim_allowed_before_evidence")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err("Senior SWE-Bench retry attempt step execution must forbid fitness claims before evidence".to_string());
    }
    if value
        .get("github_solution_search_allowed")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err("Senior SWE-Bench retry attempt step execution must forbid public GitHub solution search".to_string());
    }

    let candidate_patch_path = PathBuf::from(required_plan_string(&value, "candidate_patch_path")?);
    let candidate_patch_hash = required_plan_string(&value, "candidate_patch_hash")?;
    validate_git_object_hash(&candidate_patch_hash).map_err(|error| {
        format!(
            "Senior SWE-Bench retry attempt candidate_patch_hash {error}: {candidate_patch_hash}"
        )
    })?;
    let actual_patch_hash = file_content_hash(&candidate_patch_path)?;
    if actual_patch_hash != candidate_patch_hash {
        return Err(format!(
            "candidate patch hash mismatch: step execution {candidate_patch_hash}, actual {actual_patch_hash}"
        ));
    }

    let selected = value.get("selected_artifact").ok_or_else(|| {
        "Senior SWE-Bench retry attempt step execution missing selected_artifact".to_string()
    })?;
    let selected_path = required_plan_string(selected, "path")?;
    let selected_hash = required_plan_string(selected, "git_object_hash")?;
    validate_git_object_hash(&selected_hash).map_err(|error| {
        format!(
            "Senior SWE-Bench retry attempt selected artifact git_object_hash {error}: {selected_hash}"
        )
    })?;
    let selected_bytes = selected
        .get("bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt selected artifact missing bytes".to_string()
        })?;
    let artifact_bytes = fs::read(&selected_path).map_err(|error| {
        format!("failed to read selected candidate artifact {selected_path}: {error}")
    })?;
    if artifact_bytes.len() as u64 != selected_bytes {
        return Err(format!(
            "selected candidate artifact byte count mismatch for {selected_path}: step execution {selected_bytes}, actual {}",
            artifact_bytes.len()
        ));
    }
    let actual_artifact_hash = git_hash_object_bytes(&artifact_bytes)?;
    if actual_artifact_hash != selected_hash {
        return Err(format!(
            "selected candidate artifact hash mismatch for {selected_path}: step execution {selected_hash}, actual {actual_artifact_hash}"
        ));
    }
    let artifact = String::from_utf8(artifact_bytes).map_err(|error| {
        format!("selected candidate artifact {selected_path} is not UTF-8 text: {error}")
    })?;
    let extracted_patch = extract_senior_swe_bench_candidate_patch(&artifact)?;
    let extracted_hash = git_hash_object_bytes(extracted_patch.as_bytes())?;
    if extracted_hash != candidate_patch_hash {
        return Err(format!(
            "candidate patch hash mismatch after re-extraction: step execution {candidate_patch_hash}, actual {extracted_hash}"
        ));
    }

    let evaluate_args = value
        .get("evaluate_args")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt step execution missing evaluate_args".to_string()
        })?
        .iter()
        .map(|arg| {
            arg.as_str().map(ToString::to_string).ok_or_else(|| {
                "Senior SWE-Bench retry attempt step execution evaluate_args contains non-string"
                    .to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let evaluate_config = validate_retry_attempt_evaluate_args(
        &evaluate_args,
        &selected_path,
        &candidate_patch_path,
    )?;
    let local_evaluation_path = evaluate_config
        .output
        .as_ref()
        .expect("retry-attempt evaluate validation requires output")
        .to_string_lossy()
        .to_string();
    if value.get("local_evaluation_path").and_then(Value::as_str)
        != Some(local_evaluation_path.as_str())
    {
        return Err(
            "Senior SWE-Bench retry attempt step execution local_evaluation_path does not match evaluate_args"
                .to_string(),
        );
    }
    let retry_step_args = value
        .get("retry_step_args")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt step execution missing retry_step_args".to_string()
        })?
        .iter()
        .map(|arg| {
            arg.as_str().map(ToString::to_string).ok_or_else(|| {
                "Senior SWE-Bench retry attempt step execution retry_step_args contains non-string"
                    .to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_retry_attempt_retry_step_args(
        &retry_step_args,
        &value,
        &evaluate_config,
        &local_evaluation_path,
    )?;

    let evaluate_exit_code = value
        .get("evaluate_exit_code")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt step execution missing evaluate_exit_code".to_string()
        })?;
    if evaluate_exit_code != 0 && evaluate_exit_code != 2 {
        return Err(format!(
            "Senior SWE-Bench retry attempt step execution has unsupported evaluate_exit_code {evaluate_exit_code}"
        ));
    }
    let local_evaluation = validate_retry_attempt_local_evaluation(
        &PathBuf::from(&local_evaluation_path),
        &value,
        &candidate_patch_path,
        &candidate_patch_hash,
        &evaluate_config,
        Some(evaluate_exit_code as i32),
    )?;
    let local_status = local_evaluation
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("<invalid>")
        .to_string();
    if value.get("local_evaluation_status").and_then(Value::as_str) != Some(local_status.as_str()) {
        return Err("Senior SWE-Bench retry attempt step execution local_evaluation_status does not match local evaluation".to_string());
    }
    if evaluate_exit_code != 0 || local_status != "passed" {
        return Err(
            "Senior SWE-Bench retry attempt step evidence inspection requires a passed local evaluation"
                .to_string(),
        );
    }

    let retry_step = value.get("retry_step").ok_or_else(|| {
        "Senior SWE-Bench retry attempt step execution missing retry_step".to_string()
    })?;
    validate_retry_attempt_step_output(retry_step, &value, &local_evaluation, &local_status)?;
    validate_retry_attempt_step_evidence_retry_step(retry_step, &value)?;
    let fitness_evidence_path = retry_step
        .get("fitness_evidence_path")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt step evidence missing fitness_evidence_path".to_string()
        })?;
    let fitness_evidence_inspect_args = retry_step
        .get("fitness_evidence_inspect_args")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt step evidence missing fitness_evidence_inspect_args"
                .to_string()
        })?
        .iter()
        .map(|arg| {
            arg.as_str().map(ToString::to_string).ok_or_else(|| {
                "Senior SWE-Bench retry attempt step evidence inspection args contain non-string"
                    .to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if fitness_evidence_inspect_args.len() != 3
        || fitness_evidence_inspect_args[0] != "fitness-evidence-inspect"
        || fitness_evidence_inspect_args[1] != fitness_evidence_path
        || fitness_evidence_inspect_args[2] != "--require-all-tests-pass"
    {
        return Err("Senior SWE-Bench retry attempt step evidence has invalid planned fitness-evidence-inspect args".to_string());
    }

    let resolved_fitness_evidence_path =
        resolve_retry_artifact_path(Path::new(fitness_evidence_path));
    let mut resolved_fitness_evidence_inspect_args = fitness_evidence_inspect_args.clone();
    resolved_fitness_evidence_inspect_args[1] =
        resolved_fitness_evidence_path.to_string_lossy().to_string();
    let current_exe = env::current_exe()
        .map_err(|error| format!("failed to resolve current a2d executable: {error}"))?;
    let output = Command::new(current_exe)
        .args(&resolved_fitness_evidence_inspect_args)
        .output()
        .map_err(|error| {
            format!("failed to run planned fitness-evidence-inspect command: {error}")
        })?;
    if !output.status.success() {
        return Err(format!(
            "planned fitness-evidence-inspect failed with exit {:?}: stdout={} stderr={}",
            output.status.code(),
            preview_text_lossy(&output.stdout),
            preview_text_lossy(&output.stderr)
        ));
    }

    let evidence_bytes = fs::read(&resolved_fitness_evidence_path).map_err(|error| {
        format!("failed to read inspected fitness evidence {fitness_evidence_path}: {error}")
    })?;
    let evidence: Value = serde_json::from_slice(&evidence_bytes).map_err(|error| {
        format!("inspected fitness evidence {fitness_evidence_path} is not JSON: {error}")
    })?;
    inspect_fitness_evidence_value(&evidence, true).map_err(|error| {
        format!(
            "inspected fitness evidence {fitness_evidence_path} is invalid after command: {error}"
        )
    })?;
    let evidence_candidate_patch_hash = evidence
        .get("candidate_patch_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "inspected Senior SWE-Bench fitness evidence missing candidate_patch_hash".to_string()
        })?;
    if evidence_candidate_patch_hash != candidate_patch_hash {
        return Err(format!(
            "inspected Senior SWE-Bench fitness evidence candidate_patch_hash {evidence_candidate_patch_hash} does not match retry-attempt candidate patch hash {candidate_patch_hash}"
        ));
    }
    let evidence_candidate_patch_path = evidence
        .get("candidate_patch_path")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "inspected Senior SWE-Bench fitness evidence missing candidate_patch_path".to_string()
        })?;
    if evidence_candidate_patch_path != candidate_patch_path.to_string_lossy() {
        return Err(format!(
            "inspected Senior SWE-Bench fitness evidence candidate_patch_path {evidence_candidate_patch_path} does not match retry-attempt candidate patch path {}",
            candidate_patch_path.display()
        ));
    }
    let evidence_artifact_path = evidence
        .get("candidate_patch_artifact_path")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "inspected Senior SWE-Bench fitness evidence missing candidate_patch_artifact_path"
                .to_string()
        })?;
    if evidence_artifact_path != selected_path {
        return Err(format!(
            "inspected Senior SWE-Bench fitness evidence candidate_patch_artifact_path {evidence_artifact_path} does not match selected artifact {selected_path}"
        ));
    }
    let evidence_artifact_hash = evidence
        .get("candidate_patch_artifact_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "inspected Senior SWE-Bench fitness evidence missing candidate_patch_artifact_hash"
                .to_string()
        })?;
    if evidence_artifact_hash != selected_hash {
        return Err(format!(
            "inspected Senior SWE-Bench fitness evidence candidate_patch_artifact_hash {evidence_artifact_hash} does not match selected artifact hash {selected_hash}"
        ));
    }
    let evidence_evaluator_kind = evidence
        .get("evaluator_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "inspected Senior SWE-Bench fitness evidence missing evaluator_kind".to_string()
        })?;
    if !matches!(
        evidence_evaluator_kind,
        "provided_local_command" | "official_senior_swe_bench"
    ) {
        return Err(format!(
            "inspected Senior SWE-Bench fitness evidence has unreviewed evaluator_kind {evidence_evaluator_kind}"
        ));
    }

    let fitness_evidence_summary = json!({
        "schema_version": evidence.get("schema_version").cloned().unwrap_or(Value::Null),
        "actual_tests_evaluated": evidence.get("actual_tests_evaluated").cloned().unwrap_or(Value::Null),
        "non_regressing": evidence.get("non_regressing").cloned().unwrap_or(Value::Null),
        "fitness": evidence.get("fitness").cloned().unwrap_or(Value::Null),
        "passed": evidence.get("passed").cloned().unwrap_or(Value::Null),
        "failed": evidence.get("failed").cloned().unwrap_or(Value::Null),
        "total": evidence.get("total").cloned().unwrap_or(Value::Null),
        "source_revision": evidence.get("source_revision").cloned().unwrap_or(Value::Null),
        "source_tree_dirty": evidence.get("source_tree_dirty").cloned().unwrap_or(Value::Null),
        "source_diff_hash": evidence.get("source_diff_hash").cloned().unwrap_or(Value::Null),
        "candidate_patch_hash": evidence.get("candidate_patch_hash").cloned().unwrap_or(Value::Null),
        "candidate_patch_path": evidence.get("candidate_patch_path").cloned().unwrap_or(Value::Null),
        "candidate_patch_artifact_path": evidence.get("candidate_patch_artifact_path").cloned().unwrap_or(Value::Null),
        "candidate_patch_artifact_hash": evidence.get("candidate_patch_artifact_hash").cloned().unwrap_or(Value::Null),
        "evaluator_kind": evidence.get("evaluator_kind").cloned().unwrap_or(Value::Null),
    });

    Ok(json!({
        "schema_version": "a2d.senior-swe-bench-retry-attempt-step-evidence-execution.v1",
        "task_id": value.get("task_id").cloned().unwrap_or(Value::Null),
        "repo": value.get("repo").cloned().unwrap_or(Value::Null),
        "attempt_index": value.get("attempt_index").cloned().unwrap_or(Value::Null),
        "candidate_patch_path": candidate_patch_path,
        "candidate_patch_hash": candidate_patch_hash,
        "selected_artifact": selected.clone(),
        "evaluate_args": evaluate_args,
        "retry_step_args": retry_step_args,
        "evaluate_exit_code": evaluate_exit_code,
        "local_evaluation_path": local_evaluation_path,
        "local_evaluation_status": local_status,
        "retry_step": retry_step.clone(),
        "fitness_evidence_path": fitness_evidence_path,
        "fitness_evidence_inspect_args": fitness_evidence_inspect_args,
        "fitness_evidence_inspect_exit_code": output.status.code().unwrap_or(0),
        "fitness_evidence_inspect_stdout_preview": preview_text_lossy(&output.stdout),
        "provider_invocations_started": false,
        "evaluator_invocations_started": false,
        "retry_step_started": false,
        "prior_evaluator_invocations_started": true,
        "prior_retry_step_started": true,
        "fitness_evidence_inspection_started": true,
        "fitness_evidence_inspection_passed": true,
        "fitness_claim_allowed_before_evidence": false,
        "fitness_claim_allowed_after_evidence_inspection": true,
        "github_solution_search_allowed": false,
        "fitness_evidence_summary": fitness_evidence_summary,
        "next_step": "the planned a2d.fitness-evidence.v1 inspection gate passed; any benchmark success claim must still state evaluator_kind/provenance and must not overclaim local-wrapper evidence as official Senior SWE-Bench mastery",
        "note": "deterministic retry-attempt evidence inspection only: this command runs exactly one planned fitness-evidence-inspect command after prior retry-step execution and starts no providers or evaluators",
    }))
}

fn run_senior_swe_bench_retry_run_result(args: &[String]) {
    if args.len() != 1 {
        eprintln!(
            "Usage: a2d senior-swe-bench-retry-run-result <retry-attempt-step-evidence-execution.json|->"
        );
        std::process::exit(1);
    }
    let step_evidence_execution = read_artifact_or_exit(&args[0]);
    let run_result = build_senior_swe_bench_retry_run_result(&step_evidence_execution)
        .unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench retry run result error: {error}");
            std::process::exit(1);
        });
    println!(
        "{}",
        serde_json::to_string_pretty(&run_result)
            .expect("Senior SWE-Bench retry run result must serialize")
    );
}

#[derive(Debug, Clone)]
struct SeniorSweBenchRetryResumeAttemptPlanConfig {
    retry_execution: PathBuf,
    retry_plan: PathBuf,
    cycle_output_manifest: PathBuf,
    next_cycle_execution: Option<PathBuf>,
    apply_candidate_patch: bool,
    official_evaluator_manifest: Option<PathBuf>,
    official_evaluator_manifest_inspection: Option<PathBuf>,
    evaluator_command: Vec<String>,
}

#[derive(Debug, Clone)]
struct SeniorSweBenchRetryNextCycleBoundary {
    task_id: String,
    repo: String,
    next_cycle_command: Value,
    argv: Vec<String>,
    task_cycle_input: PathBuf,
    checkout: PathBuf,
    output_artifacts_dir: PathBuf,
    expected_manifest: PathBuf,
    attempt_index: usize,
    attempt_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct SeniorSweBenchRetryRunNextCycleConfig {
    retry_execution: PathBuf,
}

#[derive(Debug, Clone)]
enum SeniorSweBenchRetryRunNextGateConfig {
    FromRetryExecution(SeniorSweBenchRetryRunNextCycleConfig),
    FromNextCycleExecution(SeniorSweBenchRetryResumeAttemptPlanConfig),
    FromResumeAttemptPlan { retry_attempt_plan: PathBuf },
}

fn run_senior_swe_bench_retry_resume_attempt_plan(args: &[String]) {
    let config = parse_senior_swe_bench_retry_resume_attempt_plan_args(args).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench retry resume attempt plan error: {error}");
        eprintln!("Usage: a2d senior-swe-bench-retry-resume-attempt-plan (--retry-execution <retry-execution.json> --cycle-output-manifest <manifest.json> | --next-cycle-execution <retry-next-cycle-execution.json>) --retry-plan <retry-plan.json> [--apply-candidate-patch] [--official-evaluator-manifest <json> --official-evaluator-manifest-inspection <json>] -- <evaluator> [args...]");
        std::process::exit(1);
    });
    let plan = build_senior_swe_bench_retry_resume_attempt_plan(&config).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench retry resume attempt plan error: {error}");
        std::process::exit(1);
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&plan)
            .expect("Senior SWE-Bench retry resume attempt plan must serialize")
    );
}

fn run_senior_swe_bench_retry_resume_attempt_execute(args: &[String]) {
    if args.len() != 1 {
        eprintln!(
            "Usage: a2d senior-swe-bench-retry-resume-attempt-execute <retry-attempt-plan.json|->"
        );
        std::process::exit(1);
    }
    let plan = read_artifact_or_exit(&args[0]);
    let execution =
        build_senior_swe_bench_retry_resume_attempt_execution(&plan).unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench retry resume attempt execute error: {error}");
            std::process::exit(1);
        });
    let success = execution.get("status").and_then(Value::as_str) == Some("success");
    println!(
        "{}",
        serde_json::to_string_pretty(&execution)
            .expect("Senior SWE-Bench retry resume attempt execution must serialize")
    );
    if !success {
        std::process::exit(2);
    }
}

fn run_senior_swe_bench_retry_run_next_cycle(args: &[String]) {
    let config = parse_senior_swe_bench_retry_run_next_cycle_args(args).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench retry run next cycle error: {error}");
        eprintln!("Usage: a2d senior-swe-bench-retry-run-next-cycle --retry-execution <retry-execution.json>");
        std::process::exit(1);
    });
    let execution =
        build_senior_swe_bench_retry_next_cycle_execution(&config).unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench retry run next cycle error: {error}");
            std::process::exit(1);
        });
    let success = execution.get("status").and_then(Value::as_str) == Some("success");
    println!(
        "{}",
        serde_json::to_string_pretty(&execution)
            .expect("Senior SWE-Bench retry next-cycle execution must serialize")
    );
    if !success {
        std::process::exit(2);
    }
}

fn run_senior_swe_bench_retry_run_next_gate(args: &[String]) {
    let config = parse_senior_swe_bench_retry_run_next_gate_args(args).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench retry run next gate error: {error}");
        eprintln!("Usage: a2d senior-swe-bench-retry-run-next-gate (--retry-execution <retry-execution.json> | --next-cycle-execution <retry-next-cycle-execution.json> --retry-plan <retry-plan.json> [--apply-candidate-patch] [--official-evaluator-manifest <json> --official-evaluator-manifest-inspection <json>] -- <evaluator> [args...] | --retry-attempt-plan <retry-attempt-plan.json>)");
        std::process::exit(1);
    });
    let execution =
        build_senior_swe_bench_retry_next_gate_execution(&config).unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench retry run next gate error: {error}");
            std::process::exit(1);
        });
    let success = execution.get("status").and_then(Value::as_str) == Some("success");
    println!(
        "{}",
        serde_json::to_string_pretty(&execution)
            .expect("Senior SWE-Bench retry next-gate execution must serialize")
    );
    if !success {
        std::process::exit(2);
    }
}

fn parse_senior_swe_bench_retry_run_next_cycle_args(
    args: &[String],
) -> Result<SeniorSweBenchRetryRunNextCycleConfig, String> {
    let mut retry_execution = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--retry-execution" => {
                index += 1;
                retry_execution =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--retry-execution requires a path".to_string()
                    })?));
            }
            other => {
                return Err(format!(
                    "unknown senior-swe-bench-retry-run-next-cycle argument: {other}"
                ));
            }
        }
        index += 1;
    }
    validate_retry_run_next_cycle_config(SeniorSweBenchRetryRunNextCycleConfig {
        retry_execution: retry_execution.ok_or_else(|| "missing --retry-execution".to_string())?,
    })
}

fn validate_retry_run_next_cycle_config(
    config: SeniorSweBenchRetryRunNextCycleConfig,
) -> Result<SeniorSweBenchRetryRunNextCycleConfig, String> {
    if config.retry_execution == Path::new("-") {
        return Err(
            "Senior SWE-Bench retry run next cycle requires retry-execution file path".to_string(),
        );
    }
    let retry_execution = resolve_retry_artifact_path(&config.retry_execution);
    if !retry_execution.is_file() {
        return Err(format!(
            "Senior SWE-Bench retry execution not found: {}",
            config.retry_execution.display()
        ));
    }
    Ok(SeniorSweBenchRetryRunNextCycleConfig { retry_execution })
}

fn parse_senior_swe_bench_retry_run_next_gate_args(
    args: &[String],
) -> Result<SeniorSweBenchRetryRunNextGateConfig, String> {
    if args.is_empty() {
        return Err("missing Senior SWE-Bench retry next-gate selector".to_string());
    }
    match args[0].as_str() {
        "--retry-execution" => {
            if args.len() != 2 {
                return Err("--retry-execution next-gate mode accepts exactly one path".to_string());
            }
            let config =
                validate_retry_run_next_cycle_config(SeniorSweBenchRetryRunNextCycleConfig {
                    retry_execution: PathBuf::from(&args[1]),
                })?;
            Ok(SeniorSweBenchRetryRunNextGateConfig::FromRetryExecution(
                config,
            ))
        }
        "--next-cycle-execution" => {
            let next_cycle_execution = args
                .get(1)
                .ok_or_else(|| "--next-cycle-execution requires a path".to_string())?;
            let mut resume_args = vec![
                "--next-cycle-execution".to_string(),
                next_cycle_execution.clone(),
            ];
            resume_args.extend_from_slice(&args[2..]);
            let config = parse_senior_swe_bench_retry_resume_attempt_plan_args(&resume_args)?;
            Ok(SeniorSweBenchRetryRunNextGateConfig::FromNextCycleExecution(config))
        }
        "--retry-attempt-plan" => {
            if args.len() != 2 {
                return Err(
                    "--retry-attempt-plan next-gate mode accepts exactly one path".to_string(),
                );
            }
            let supplied_retry_attempt_plan = PathBuf::from(&args[1]);
            if supplied_retry_attempt_plan == Path::new("-") {
                return Err(
                    "Senior SWE-Bench retry next gate requires retry-attempt-plan file path"
                        .to_string(),
                );
            }
            let retry_attempt_plan = resolve_retry_artifact_path(&supplied_retry_attempt_plan);
            if !retry_attempt_plan.is_file() {
                return Err(format!(
                    "Senior SWE-Bench retry attempt plan not found: {}",
                    supplied_retry_attempt_plan.display()
                ));
            }
            Ok(SeniorSweBenchRetryRunNextGateConfig::FromResumeAttemptPlan { retry_attempt_plan })
        }
        other => Err(format!(
            "unknown senior-swe-bench-retry-run-next-gate argument: {other}"
        )),
    }
}

fn parse_senior_swe_bench_retry_resume_attempt_plan_args(
    args: &[String],
) -> Result<SeniorSweBenchRetryResumeAttemptPlanConfig, String> {
    let mut retry_execution = None;
    let mut retry_plan: Option<PathBuf> = None;
    let mut cycle_output_manifest: Option<PathBuf> = None;
    let mut next_cycle_execution: Option<PathBuf> = None;
    let mut apply_candidate_patch = false;
    let mut official_evaluator_manifest: Option<PathBuf> = None;
    let mut official_evaluator_manifest_inspection: Option<PathBuf> = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--" => {
                let evaluator_command = args[index + 1..].to_vec();
                if evaluator_command.is_empty() {
                    return Err(
                        "Senior SWE-Bench retry resume attempt evaluator command is empty"
                            .to_string(),
                    );
                }
                let retry_plan = resolve_retry_artifact_path(
                    &retry_plan.ok_or_else(|| "missing --retry-plan".to_string())?,
                );
                let (retry_execution, cycle_output_manifest) =
                    resolve_retry_resume_attempt_boundary_paths(
                        retry_execution,
                        cycle_output_manifest,
                        next_cycle_execution.clone(),
                    )?;
                let config = SeniorSweBenchRetryResumeAttemptPlanConfig {
                    retry_execution,
                    retry_plan,
                    cycle_output_manifest,
                    next_cycle_execution: next_cycle_execution
                        .as_ref()
                        .map(|path| resolve_retry_artifact_path(path)),
                    apply_candidate_patch,
                    official_evaluator_manifest: official_evaluator_manifest
                        .as_ref()
                        .map(|path| resolve_retry_artifact_path(path)),
                    official_evaluator_manifest_inspection: official_evaluator_manifest_inspection
                        .as_ref()
                        .map(|path| resolve_retry_artifact_path(path)),
                    evaluator_command,
                };
                validate_retry_resume_attempt_plan_config(&config)?;
                return Ok(config);
            }
            "--retry-execution" => {
                index += 1;
                retry_execution =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--retry-execution requires a path".to_string()
                    })?));
            }
            "--retry-plan" => {
                index += 1;
                retry_plan = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--retry-plan requires a path".to_string())?,
                ));
            }
            "--cycle-output-manifest" => {
                index += 1;
                cycle_output_manifest =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--cycle-output-manifest requires a path".to_string()
                    })?));
            }
            "--next-cycle-execution" => {
                index += 1;
                next_cycle_execution =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--next-cycle-execution requires a path".to_string()
                    })?));
            }
            "--apply-candidate-patch" => apply_candidate_patch = true,
            "--official-evaluator-manifest" => {
                index += 1;
                official_evaluator_manifest =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--official-evaluator-manifest requires a path".to_string()
                    })?));
            }
            "--official-evaluator-manifest-inspection" => {
                index += 1;
                official_evaluator_manifest_inspection =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--official-evaluator-manifest-inspection requires a path".to_string()
                    })?));
            }
            other => {
                return Err(format!(
                    "unknown senior-swe-bench-retry-resume-attempt-plan argument: {other}"
                ));
            }
        }
        index += 1;
    }
    Err("missing -- <evaluator> command".to_string())
}

fn resolve_retry_resume_attempt_boundary_paths(
    retry_execution: Option<PathBuf>,
    cycle_output_manifest: Option<PathBuf>,
    next_cycle_execution: Option<PathBuf>,
) -> Result<(PathBuf, PathBuf), String> {
    if let Some(next_cycle_execution) = next_cycle_execution {
        if retry_execution.is_some() || cycle_output_manifest.is_some() {
            return Err(
                "use either --next-cycle-execution or the --retry-execution/--cycle-output-manifest pair, not both"
                    .to_string(),
            );
        }
        return load_senior_swe_bench_retry_next_cycle_execution_paths(&next_cycle_execution);
    }
    Ok((
        resolve_retry_artifact_path(
            &retry_execution.ok_or_else(|| "missing --retry-execution".to_string())?,
        ),
        resolve_retry_artifact_path(
            &cycle_output_manifest.ok_or_else(|| "missing --cycle-output-manifest".to_string())?,
        ),
    ))
}

fn validate_retry_resume_attempt_plan_config(
    config: &SeniorSweBenchRetryResumeAttemptPlanConfig,
) -> Result<(), String> {
    for (name, path) in [
        ("retry-execution", &config.retry_execution),
        ("retry-plan", &config.retry_plan),
        ("cycle-output-manifest", &config.cycle_output_manifest),
    ] {
        if path == Path::new("-") {
            return Err(format!(
                "Senior SWE-Bench retry resume attempt plan requires {name} file paths"
            ));
        }
        if !path.is_file() {
            return Err(format!(
                "Senior SWE-Bench retry resume attempt plan {name} not found: {}",
                path.display()
            ));
        }
    }
    if let Some(next_cycle_execution) = &config.next_cycle_execution
        && !next_cycle_execution.is_file()
    {
        return Err(format!(
            "Senior SWE-Bench retry next-cycle execution not found: {}",
            next_cycle_execution.display()
        ));
    }
    match (
        &config.official_evaluator_manifest,
        &config.official_evaluator_manifest_inspection,
    ) {
        (Some(manifest), Some(inspection)) => {
            if !manifest.is_file() {
                return Err(format!(
                    "Senior SWE-Bench official evaluator manifest not found: {}",
                    manifest.display()
                ));
            }
            if !inspection.is_file() {
                return Err(format!(
                    "Senior SWE-Bench official evaluator manifest inspection not found: {}",
                    inspection.display()
                ));
            }
        }
        (Some(_), None) => {
            return Err("Senior SWE-Bench retry resume attempt plan requires --official-evaluator-manifest-inspection when --official-evaluator-manifest is supplied; run senior-swe-bench-official-evaluator-manifest-inspect first".to_string());
        }
        (None, Some(_)) => {
            return Err("Senior SWE-Bench retry resume attempt plan --official-evaluator-manifest-inspection requires --official-evaluator-manifest".to_string());
        }
        (None, None) => {}
    }
    Ok(())
}

fn load_senior_swe_bench_retry_next_cycle_execution_paths(
    next_cycle_execution_path: &Path,
) -> Result<(PathBuf, PathBuf), String> {
    if next_cycle_execution_path == Path::new("-") {
        return Err(
            "Senior SWE-Bench retry resume attempt plan requires next-cycle execution file paths"
                .to_string(),
        );
    }
    let next_cycle_execution_path = resolve_retry_artifact_path(next_cycle_execution_path);
    if !next_cycle_execution_path.is_file() {
        return Err(format!(
            "Senior SWE-Bench retry next-cycle execution not found: {}",
            next_cycle_execution_path.display()
        ));
    }
    let text = read_artifact_to_string(&next_cycle_execution_path)?;
    let execution: Value = serde_json::from_str(&text).map_err(|error| {
        format!("invalid Senior SWE-Bench retry next-cycle execution JSON: {error}")
    })?;
    let schema = execution
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "Senior SWE-Bench retry next-cycle execution missing schema_version".to_string()
        })?;
    if schema != "a2d.senior-swe-bench-retry-next-cycle-execution.v1" {
        return Err(format!(
            "expected a2d.senior-swe-bench-retry-next-cycle-execution.v1, got {schema}"
        ));
    }
    if execution.get("status").and_then(Value::as_str) != Some("success") {
        return Err(
            "Senior SWE-Bench retry resume requires successful retry next-cycle execution"
                .to_string(),
        );
    }
    if execution.get("stop_reason").and_then(Value::as_str) != Some("cycle_output_manifest_ready") {
        return Err(
            "Senior SWE-Bench retry next-cycle execution must have cycle_output_manifest_ready stop_reason"
                .to_string(),
        );
    }
    for field in [
        "task_id",
        "repo",
        "retry_execution_path",
        "cycle_output_manifest",
    ] {
        if execution.get(field).and_then(Value::as_str).is_none() {
            return Err(format!(
                "Senior SWE-Bench retry next-cycle execution missing {field}"
            ));
        }
    }
    if execution
        .get("attempt_index")
        .and_then(Value::as_u64)
        .is_none()
    {
        return Err(
            "Senior SWE-Bench retry next-cycle execution missing attempt_index".to_string(),
        );
    }
    let artifact_count = execution
        .get("cycle_output_artifact_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "Senior SWE-Bench retry next-cycle execution missing cycle_output_artifact_count"
                .to_string()
        })?;
    if artifact_count == 0 {
        return Err(
            "Senior SWE-Bench retry next-cycle execution recorded no output artifacts".to_string(),
        );
    }
    for forbidden in [
        "fitness",
        "fitness_delta",
        "fitness_evidence",
        "fitness_evidence_path",
        "terminal_run_result",
        "official_senior_swe_bench_mastery",
    ] {
        if execution.get(forbidden).is_some() {
            return Err(format!(
                "Senior SWE-Bench retry next-cycle execution must not contain pre-evidence fitness claim field {forbidden}"
            ));
        }
    }
    for (field, expected) in [
        ("cycle_input_command_started", true),
        ("cycle_input_command_spawned", true),
        ("cycle_input_command_timed_out", false),
        ("provider_invocations_started_by_this_command", true),
        ("evaluator_invocations_started", false),
        ("fitness_evidence_inspection_started", false),
        ("fitness_claim_allowed_before_evidence", false),
        ("fitness_claim_allowed_after_cycle", false),
        ("github_solution_search_allowed", false),
    ] {
        if execution.get(field).and_then(Value::as_bool) != Some(expected) {
            return Err(format!(
                "Senior SWE-Bench retry next-cycle execution {field} must be {expected}"
            ));
        }
    }
    if execution
        .get("cycle_input_exit_code")
        .and_then(Value::as_i64)
        != Some(0)
    {
        return Err(
            "Senior SWE-Bench retry next-cycle execution must record cycle_input_exit_code 0"
                .to_string(),
        );
    }
    let retry_execution = resolve_retry_artifact_path(Path::new(
        execution
            .get("retry_execution_path")
            .and_then(Value::as_str)
            .expect("validated retry_execution_path"),
    ));
    if !retry_execution.is_file() {
        return Err(format!(
            "Senior SWE-Bench prior retry execution not found: {}",
            retry_execution.display()
        ));
    }
    let boundary = load_senior_swe_bench_retry_next_cycle_boundary(&retry_execution)?;
    let task_id = execution
        .get("task_id")
        .and_then(Value::as_str)
        .expect("validated task_id");
    let repo = execution
        .get("repo")
        .and_then(Value::as_str)
        .expect("validated repo");
    let attempt_index = execution
        .get("attempt_index")
        .and_then(Value::as_u64)
        .expect("validated attempt_index") as usize;
    if task_id != boundary.task_id
        || repo != boundary.repo
        || attempt_index != boundary.attempt_index
        || execution.get("next_cycle_command") != Some(&boundary.next_cycle_command)
    {
        return Err(
            "Senior SWE-Bench retry next-cycle execution metadata does not match prior retry boundary"
                .to_string(),
        );
    }
    let expected_summary_path = boundary.attempt_dir.join("retry-next-cycle-execution.json");
    if !paths_equivalent(&next_cycle_execution_path, &expected_summary_path) {
        return Err(format!(
            "Senior SWE-Bench retry next-cycle execution path {} does not match expected boundary summary {}",
            next_cycle_execution_path.display(),
            expected_summary_path.display()
        ));
    }
    let cycle_output_manifest = resolve_retry_artifact_path(Path::new(
        execution
            .get("cycle_output_manifest")
            .and_then(Value::as_str)
            .expect("validated cycle_output_manifest"),
    ));
    if !paths_equivalent(&cycle_output_manifest, &boundary.expected_manifest) {
        return Err(format!(
            "Senior SWE-Bench retry next-cycle execution manifest {} does not match expected boundary manifest {}",
            cycle_output_manifest.display(),
            boundary.expected_manifest.display()
        ));
    }
    for (field, path) in [
        ("task_cycle_input", &boundary.task_cycle_input),
        ("checkout", &boundary.checkout),
        ("output_artifacts_dir", &boundary.output_artifacts_dir),
    ] {
        let recorded = execution
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!("Senior SWE-Bench retry next-cycle execution missing {field}")
            })?;
        if !paths_equivalent(&resolve_retry_artifact_path(Path::new(recorded)), path) {
            return Err(format!(
                "Senior SWE-Bench retry next-cycle execution {field} {recorded} does not match boundary {}",
                path.display()
            ));
        }
    }
    let manifest_hash = execution
        .get("cycle_output_manifest_git_object_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "Senior SWE-Bench retry next-cycle execution missing cycle_output_manifest_git_object_hash"
                .to_string()
        })?;
    validate_git_object_hash(manifest_hash).map_err(|error| {
        format!(
            "Senior SWE-Bench retry next-cycle execution cycle_output_manifest_git_object_hash {error}: {manifest_hash}"
        )
    })?;
    let current_manifest_hash = file_content_hash(&cycle_output_manifest)?;
    if current_manifest_hash != manifest_hash {
        return Err(format!(
            "Senior SWE-Bench retry next-cycle execution manifest hash {manifest_hash} does not match current manifest hash {current_manifest_hash}"
        ));
    }
    let validated_count = validate_retry_next_cycle_manifest(&cycle_output_manifest)?;
    if validated_count as u64 != artifact_count {
        return Err(format!(
            "Senior SWE-Bench retry next-cycle execution artifact count {artifact_count} does not match manifest count {validated_count}"
        ));
    }
    Ok((retry_execution, cycle_output_manifest))
}

fn load_senior_swe_bench_retry_next_cycle_boundary(
    retry_execution_path: &Path,
) -> Result<SeniorSweBenchRetryNextCycleBoundary, String> {
    let retry_execution_path = resolve_retry_artifact_path(retry_execution_path);
    let retry_execution_text = read_artifact_to_string(&retry_execution_path)?;
    let retry_execution: Value = serde_json::from_str(&retry_execution_text)
        .map_err(|error| format!("invalid Senior SWE-Bench retry execution JSON: {error}"))?;
    let schema = retry_execution
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry execution missing schema_version".to_string())?;
    if schema != "a2d.senior-swe-bench-retry-execution.v1" {
        return Err(format!(
            "expected a2d.senior-swe-bench-retry-execution.v1, got {schema}"
        ));
    }
    if retry_execution.get("status").and_then(Value::as_str) != Some("failed") {
        return Err(
            "Senior SWE-Bench retry next-cycle boundary requires failed retry execution status"
                .to_string(),
        );
    }
    if retry_execution.get("stop_reason").and_then(Value::as_str)
        != Some("precomputed_attempt_manifests_exhausted")
    {
        return Err(
            "Senior SWE-Bench retry next-cycle boundary requires precomputed manifest exhaustion"
                .to_string(),
        );
    }
    if retry_execution
        .get("provider_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err("Senior SWE-Bench retry execution must not have started providers".to_string());
    }
    if retry_execution
        .get("fitness_claim_allowed_after_evidence_inspection")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry resume requires a non-success retry execution".to_string(),
        );
    }
    let next_cycle_command = retry_execution
        .get("next_cycle_command")
        .cloned()
        .ok_or_else(|| "Senior SWE-Bench retry execution missing next_cycle_command".to_string())?;
    if next_cycle_command
        .get("provider_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err("Senior SWE-Bench next_cycle_command must not pre-start providers".to_string());
    }
    for field in [
        "evaluator_invocations_started",
        "fitness_evidence_inspection_started",
        "github_solution_search_allowed",
    ] {
        if let Some(value) = next_cycle_command.get(field)
            && value.as_bool() != Some(false)
        {
            return Err(format!(
                "Senior SWE-Bench next_cycle_command {field} must be false"
            ));
        }
    }
    if next_cycle_command
        .get("fitness_claim_allowed_before_evidence")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench next_cycle_command must forbid pre-evidence fitness claims"
                .to_string(),
        );
    }
    if next_cycle_command.get("command").and_then(Value::as_str) != Some("a2d") {
        return Err("Senior SWE-Bench next_cycle_command command must be a2d".to_string());
    }
    let argv = next_cycle_command
        .get("argv")
        .and_then(Value::as_array)
        .ok_or_else(|| "Senior SWE-Bench next_cycle_command missing argv".to_string())?;
    let argv = argv
        .iter()
        .map(|arg| {
            arg.as_str().map(ToString::to_string).ok_or_else(|| {
                "Senior SWE-Bench next_cycle_command argv contains non-string".to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if argv.len() != 7
        || argv[0] != "cycle-input"
        || argv[2] != "1"
        || argv[3] != "--checkout"
        || argv[5] != "--output-artifacts"
    {
        return Err(
            "Senior SWE-Bench next_cycle_command has invalid cycle-input argv order".to_string(),
        );
    }
    let task_cycle_input = resolve_retry_artifact_path(Path::new(&argv[1]));
    let checkout = resolve_retry_artifact_path(Path::new(&argv[4]));
    let output_artifacts_dir = resolve_retry_artifact_path(Path::new(&argv[6]));
    if !task_cycle_input.is_file() {
        return Err(format!(
            "Senior SWE-Bench next cycle input not found: {}",
            task_cycle_input.display()
        ));
    }
    if !checkout.is_dir() {
        return Err(format!(
            "Senior SWE-Bench next cycle checkout not found: {}",
            checkout.display()
        ));
    }
    let expected_manifest = resolve_retry_artifact_path(Path::new(
        next_cycle_command
            .get("expected_manifest_path")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "Senior SWE-Bench next_cycle_command missing expected_manifest_path".to_string()
            })?,
    ));
    let command_manifest = output_artifacts_dir.join("manifest.json");
    if !paths_equivalent(&expected_manifest, &command_manifest) {
        return Err(format!(
            "Senior SWE-Bench next_cycle_command expected manifest {} does not match output-artifacts manifest {}",
            expected_manifest.display(),
            command_manifest.display()
        ));
    }
    let task_id = retry_execution
        .get("task_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry execution missing task_id".to_string())?
        .to_string();
    let repo = retry_execution
        .get("repo")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry execution missing repo".to_string())?
        .to_string();
    let max_attempts = retry_execution
        .get("max_attempts")
        .and_then(Value::as_u64)
        .ok_or_else(|| "Senior SWE-Bench retry execution missing max_attempts".to_string())?
        as usize;
    let attempts_executed = retry_execution
        .get("attempts_executed")
        .and_then(Value::as_u64)
        .ok_or_else(|| "Senior SWE-Bench retry execution missing attempts_executed".to_string())?
        as usize;
    if attempts_executed == 0 {
        return Err(
            "Senior SWE-Bench retry resume requires at least one prior attempt".to_string(),
        );
    }
    if attempts_executed >= max_attempts {
        return Err(format!(
            "Senior SWE-Bench retry next-cycle boundary attempts_executed {attempts_executed} must be below max_attempts {max_attempts}"
        ));
    }
    let attempts = retry_execution
        .get("attempts")
        .and_then(Value::as_array)
        .ok_or_else(|| "Senior SWE-Bench retry execution missing attempts".to_string())?;
    if attempts.len() != attempts_executed {
        return Err(format!(
            "Senior SWE-Bench retry execution attempts_executed {attempts_executed} does not match attempts length {}",
            attempts.len()
        ));
    }
    let last_attempt = attempts
        .last()
        .ok_or_else(|| "Senior SWE-Bench retry execution missing last attempt".to_string())?;
    if last_attempt.get("next_cycle_command") != Some(&next_cycle_command) {
        return Err(
            "Senior SWE-Bench retry execution next_cycle_command does not match last attempt"
                .to_string(),
        );
    }
    if last_attempt.get("attempt_index").and_then(Value::as_u64)
        != Some((attempts_executed - 1) as u64)
    {
        return Err(
            "Senior SWE-Bench retry execution last attempt index is inconsistent".to_string(),
        );
    }
    if last_attempt
        .get("retry_step_decision")
        .and_then(Value::as_str)
        != Some("build_next_cycle_input")
    {
        return Err(
            "Senior SWE-Bench retry next-cycle boundary requires last attempt to build next cycle input"
                .to_string(),
        );
    }
    if let Some(recorded_next_input) = last_attempt
        .get("next_cycle_input_path")
        .and_then(Value::as_str)
        && !paths_equivalent(
            &resolve_retry_artifact_path(Path::new(recorded_next_input)),
            &task_cycle_input,
        )
    {
        return Err(
            "Senior SWE-Bench retry execution next_cycle_input_path does not match next command"
                .to_string(),
        );
    }
    let retry_execution_work_dir = retry_execution_path
        .parent()
        .ok_or_else(|| "Senior SWE-Bench retry execution path has no parent".to_string())?;
    let expected_attempt_dir =
        retry_execution_work_dir.join(format!("attempt-{attempts_executed}"));
    let attempt_dir = output_artifacts_dir
        .parent()
        .ok_or_else(|| "Senior SWE-Bench next cycle output path has no attempt dir".to_string())?
        .to_path_buf();
    if !paths_equivalent(&attempt_dir, &expected_attempt_dir) {
        return Err(format!(
            "Senior SWE-Bench next cycle attempt dir {} does not match expected {}",
            attempt_dir.display(),
            expected_attempt_dir.display()
        ));
    }
    Ok(SeniorSweBenchRetryNextCycleBoundary {
        task_id,
        repo,
        next_cycle_command,
        argv,
        task_cycle_input,
        checkout,
        output_artifacts_dir,
        expected_manifest,
        attempt_index: attempts_executed,
        attempt_dir,
    })
}

fn build_senior_swe_bench_retry_resume_attempt_plan(
    config: &SeniorSweBenchRetryResumeAttemptPlanConfig,
) -> Result<Value, String> {
    let boundary = load_senior_swe_bench_retry_next_cycle_boundary(&config.retry_execution)?;
    if !paths_equivalent(&config.cycle_output_manifest, &boundary.expected_manifest) {
        return Err(format!(
            "Senior SWE-Bench supplied cycle-output manifest {} does not match expected manifest path {}",
            config.cycle_output_manifest.display(),
            boundary.expected_manifest.display()
        ));
    }
    let manifest_hash = file_content_hash(&config.cycle_output_manifest)?;
    let attempt_config = SeniorSweBenchRetryAttemptPlanConfig {
        retry_plan: config.retry_plan.clone(),
        attempt_index: boundary.attempt_index,
        task_cycle_input: boundary.task_cycle_input.clone(),
        cycle_output_manifest: config.cycle_output_manifest.clone(),
        checkout: boundary.checkout.clone(),
        attempt_dir: boundary.attempt_dir.clone(),
        apply_candidate_patch: config.apply_candidate_patch,
        official_evaluator_manifest: config.official_evaluator_manifest.clone(),
        official_evaluator_manifest_inspection: config
            .official_evaluator_manifest_inspection
            .clone(),
        evaluator_command: config.evaluator_command.clone(),
    };
    let mut plan = build_senior_swe_bench_retry_attempt_plan(&attempt_config)?;
    plan["resume_boundary"] = json!({
        "schema_version": "a2d.senior-swe-bench-retry-resume-boundary.v1",
        "retry_execution_path": retry_artifact_path_string(&config.retry_execution),
        "next_cycle_execution_path": config.next_cycle_execution.as_ref().map(|path| retry_artifact_path_string(path)),
        "task_id": boundary.task_id,
        "repo": boundary.repo,
        "attempt_index": boundary.attempt_index,
        "attempt_dir": retry_artifact_path_string(&boundary.attempt_dir),
        "task_cycle_input": retry_artifact_path_string(&boundary.task_cycle_input),
        "checkout": retry_artifact_path_string(&boundary.checkout),
        "output_artifacts_dir": retry_artifact_path_string(&boundary.output_artifacts_dir),
        "cycle_output_manifest": retry_artifact_path_string(&config.cycle_output_manifest),
        "cycle_output_manifest_git_object_hash": manifest_hash,
        "next_cycle_command": boundary.next_cycle_command,
        "provider_invocations_started": false,
        "evaluator_invocations_started": false,
        "fitness_claim_allowed_before_evidence": false,
    });
    Ok(plan)
}

fn build_senior_swe_bench_retry_resume_attempt_execution(plan: &str) -> Result<Value, String> {
    let attempt_plan: Value = serde_json::from_str(plan).map_err(|error| {
        format!("invalid Senior SWE-Bench retry resume attempt plan JSON: {error}")
    })?;
    if attempt_plan.get("schema_version").and_then(Value::as_str)
        != Some("a2d.senior-swe-bench-retry-attempt-plan.v1")
    {
        return Err(
            "Senior SWE-Bench retry resume attempt execute expected a retry-attempt plan"
                .to_string(),
        );
    }
    if attempt_plan
        .get("provider_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
        || attempt_plan
            .get("evaluator_invocations_started")
            .and_then(Value::as_bool)
            != Some(false)
        || attempt_plan
            .get("fitness_claim_allowed_before_evidence")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry resume attempt plan must start no providers/evaluators and forbid pre-evidence fitness claims"
                .to_string(),
        );
    }
    let attempt_index = attempt_plan
        .get("attempt_index")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "Senior SWE-Bench retry resume attempt plan missing attempt_index".to_string()
        })? as usize;
    let attempt_dir = resolve_retry_artifact_path(Path::new(&required_plan_string(
        &attempt_plan,
        "attempt_dir",
    )?));
    let work_dir = attempt_dir
        .parent()
        .ok_or_else(|| "Senior SWE-Bench retry resume attempt dir has no parent".to_string())?
        .to_path_buf();
    let retry_plan_path = retry_attempt_arg_value(
        attempt_plan
            .get("retry_step_args")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                "Senior SWE-Bench retry resume attempt plan missing retry_step_args".to_string()
            })?
            .iter()
            .map(|arg| {
                arg.as_str().map(ToString::to_string).ok_or_else(|| {
                    "Senior SWE-Bench retry resume attempt retry_step_args contains non-string"
                        .to_string()
                })
            })
            .collect::<Result<Vec<_>, _>>()?
            .as_slice(),
        "--retry-plan",
    )?;
    let retry_plan_text =
        read_artifact_to_string(&resolve_retry_artifact_path(Path::new(&retry_plan_path)))?;
    let retry_plan: Value = serde_json::from_str(&retry_plan_text)
        .map_err(|error| format!("invalid Senior SWE-Bench retry plan JSON: {error}"))?;
    let prior_retry_execution_path = work_dir.join("retry-execution.json");
    let boundary = load_senior_swe_bench_retry_next_cycle_boundary(&prior_retry_execution_path)?;
    let plan_task_cycle_input = resolve_retry_artifact_path(Path::new(&required_plan_string(
        &attempt_plan,
        "task_cycle_input",
    )?));
    let plan_manifest = resolve_retry_artifact_path(Path::new(&required_plan_string(
        &attempt_plan,
        "cycle_output_manifest",
    )?));
    if boundary.attempt_index != attempt_index
        || !paths_equivalent(&boundary.attempt_dir, &attempt_dir)
        || !paths_equivalent(&boundary.task_cycle_input, &plan_task_cycle_input)
        || !paths_equivalent(&boundary.expected_manifest, &plan_manifest)
    {
        return Err(
            "Senior SWE-Bench retry resume attempt plan does not match prior retry boundary"
                .to_string(),
        );
    }
    let prior_retry_execution: Value = serde_json::from_str(&read_artifact_to_string(
        &prior_retry_execution_path,
    )?)
    .map_err(|error| format!("invalid prior Senior SWE-Bench retry execution JSON: {error}"))?;
    validate_retry_resume_attempt_execution_boundary(
        &attempt_plan,
        &retry_plan,
        &prior_retry_execution,
        &prior_retry_execution_path,
        &boundary,
    )?;
    preflight_retry_resume_attempt_outputs(&attempt_plan, &attempt_dir)?;
    let mut attempts = prior_retry_execution
        .get("attempts")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "prior Senior SWE-Bench retry execution missing attempts".to_string())?;
    if attempts.len() != attempt_index {
        return Err(format!(
            "prior Senior SWE-Bench retry execution attempts length {} does not match resumed attempt index {attempt_index}",
            attempts.len()
        ));
    }
    let extraction = build_senior_swe_bench_retry_attempt_extraction(plan)?;
    write_json_artifact(
        &attempt_dir.join("retry-attempt-extraction.json"),
        &extraction,
    )?;
    let extraction_text = serde_json::to_string(&extraction)
        .map_err(|error| format!("failed to serialize retry attempt extraction: {error}"))?;
    let evaluation = build_senior_swe_bench_retry_attempt_evaluation(&extraction_text)?;
    write_json_artifact(
        &attempt_dir.join("retry-attempt-evaluation.json"),
        &evaluation,
    )?;
    let evaluation_text = serde_json::to_string(&evaluation)
        .map_err(|error| format!("failed to serialize retry attempt evaluation: {error}"))?;
    let step_execution = build_senior_swe_bench_retry_attempt_step_execution(&evaluation_text)?;
    write_json_artifact(
        &attempt_dir.join("retry-attempt-step-execution.json"),
        &step_execution,
    )?;
    let retry_step = step_execution
        .get("retry_step")
        .cloned()
        .ok_or_else(|| "Senior SWE-Bench retry resume attempt missing retry_step".to_string())?;
    let retry_decision = retry_step
        .get("decision")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "Senior SWE-Bench retry resume attempt retry_step missing decision".to_string()
        })?;
    let mut record = json!({
        "attempt_index": attempt_index,
        "cycle_output_manifest": attempt_plan.get("cycle_output_manifest").cloned().unwrap_or(Value::Null),
        "attempt_dir": attempt_dir,
        "candidate_patch_path": extraction.get("candidate_patch_path").cloned().unwrap_or(Value::Null),
        "candidate_patch_hash": extraction.get("candidate_patch_hash").cloned().unwrap_or(Value::Null),
        "local_evaluation_path": evaluation.get("local_evaluation_path").cloned().unwrap_or(Value::Null),
        "local_evaluation_status": evaluation.get("local_evaluation_status").cloned().unwrap_or(Value::Null),
        "evaluate_exit_code": evaluation.get("evaluate_exit_code").cloned().unwrap_or(Value::Null),
        "retry_step_decision": retry_decision,
        "provider_invocations_started": false,
    });
    match retry_decision {
        "inspect_fitness_evidence" => {
            let step_execution_text = serde_json::to_string(&step_execution).map_err(|error| {
                format!("failed to serialize retry attempt step execution: {error}")
            })?;
            let evidence_execution =
                build_senior_swe_bench_retry_attempt_step_evidence_execution(&step_execution_text)?;
            let evidence_execution_text =
                serde_json::to_string(&evidence_execution).map_err(|error| {
                    format!("failed to serialize retry attempt evidence execution: {error}")
                })?;
            write_json_artifact(
                &attempt_dir.join("retry-attempt-step-evidence-execution.json"),
                &evidence_execution,
            )?;
            let run_result = build_senior_swe_bench_retry_run_result(&evidence_execution_text)?;
            write_json_artifact(&attempt_dir.join("retry-run-result.json"), &run_result)?;
            record["fitness_evidence_path"] = evidence_execution
                .get("fitness_evidence_path")
                .cloned()
                .unwrap_or(Value::Null);
            record["fitness_evidence_inspection_passed"] = json!(true);
            attempts.push(record);
            let terminal = retry_execution_terminal_result(
                &retry_plan,
                attempts,
                "success",
                "fitness_evidence_inspection_passed",
                Some(run_result),
            );
            write_json_artifact(
                &work_dir.join("retry-resume-attempt-execution.json"),
                &terminal,
            )?;
            Ok(terminal)
        }
        "build_next_cycle_input" => {
            let next_cycle_input =
                retry_step.get("next_cycle_input").cloned().ok_or_else(|| {
                    "Senior SWE-Bench retry resume attempt retry_step missing next_cycle_input"
                        .to_string()
                })?;
            let next_cycle_input_path = attempt_dir.join("next-cycle-input.json");
            write_json_artifact(&next_cycle_input_path, &next_cycle_input)?;
            let evaluate_args = evaluation
                .get("evaluate_args")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    "Senior SWE-Bench retry resume attempt evaluation missing evaluate_args"
                        .to_string()
                })?
                .iter()
                .map(|arg| {
                    arg.as_str().map(ToString::to_string).ok_or_else(|| {
                        "Senior SWE-Bench retry resume attempt evaluate_args contains non-string"
                            .to_string()
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let checkout = PathBuf::from(retry_attempt_arg_value(&evaluate_args, "--checkout")?);
            let next_cycle_output_dir = work_dir.join(format!(
                "attempt-{}/cycle-output-artifacts",
                attempt_index + 1
            ));
            let next_cycle_command = retry_execute_next_cycle_command(
                &next_cycle_input_path,
                &checkout,
                &next_cycle_output_dir,
            );
            record["next_cycle_input_path"] = json!(next_cycle_input_path);
            record["next_cycle_command"] = next_cycle_command;
            record["fitness_evidence_inspection_passed"] = json!(false);
            attempts.push(record);
            let terminal = retry_execution_terminal_result(
                &retry_plan,
                attempts,
                "failed",
                "precomputed_attempt_manifests_exhausted",
                None,
            );
            write_json_artifact(
                &work_dir.join("retry-resume-attempt-execution.json"),
                &terminal,
            )?;
            Ok(terminal)
        }
        "stop" => {
            let stop_reason = retry_step
                .get("stop_reason")
                .and_then(Value::as_str)
                .unwrap_or("retry_step_stop");
            record["fitness_evidence_inspection_passed"] = json!(false);
            attempts.push(record);
            let terminal =
                retry_execution_terminal_result(&retry_plan, attempts, "failed", stop_reason, None);
            write_json_artifact(
                &work_dir.join("retry-resume-attempt-execution.json"),
                &terminal,
            )?;
            Ok(terminal)
        }
        other => Err(format!(
            "Senior SWE-Bench retry resume attempt execute unreviewed retry-step decision {other}"
        )),
    }
}

#[derive(Debug, Clone)]
struct RetryNextCycleCommandOutput {
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    spawn_error: Option<String>,
    timed_out: bool,
}

fn run_retry_next_cycle_command(argv: &[String]) -> Result<RetryNextCycleCommandOutput, String> {
    let current_exe = env::current_exe()
        .map_err(|error| format!("failed to resolve current a2d executable: {error}"))?;
    let stdout_path = unique_temp_path("a2d-retry-next-cycle-stdout", "txt");
    let stderr_path = unique_temp_path("a2d-retry-next-cycle-stderr", "txt");
    let stdout_file = fs::File::create(&stdout_path)
        .map_err(|error| format!("failed to create next-cycle stdout capture: {error}"))?;
    let stderr_file = fs::File::create(&stderr_path)
        .map_err(|error| format!("failed to create next-cycle stderr capture: {error}"))?;
    let mut child = Command::new(current_exe)
        .args(argv)
        .current_dir(a2d_project_root())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|error| format!("failed to run persisted next cycle command: {error}"))?;
    let timeout = env::var("A2D_SENIOR_SWE_BENCH_RETRY_NEXT_CYCLE_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(1800));
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = child.wait();
                let stdout = read_and_remove_capture(&stdout_path).into_bytes();
                let stderr = read_and_remove_capture(&stderr_path).into_bytes();
                return Ok(RetryNextCycleCommandOutput {
                    exit_code: status.code(),
                    stdout,
                    stderr,
                    spawn_error: None,
                    timed_out: false,
                });
            }
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let status = child.wait().map_err(|error| {
                    format!("failed to collect timed-out next cycle command: {error}")
                })?;
                let stdout = read_and_remove_capture(&stdout_path).into_bytes();
                let mut stderr = read_and_remove_capture(&stderr_path);
                stderr.push_str(&format!(
                    "\nSenior SWE-Bench retry next-cycle command timed out after {}s",
                    timeout.as_secs()
                ));
                return Ok(RetryNextCycleCommandOutput {
                    exit_code: status.code(),
                    stdout,
                    stderr: stderr.into_bytes(),
                    spawn_error: None,
                    timed_out: true,
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                return Err(format!(
                    "failed while waiting for next cycle command: {error}"
                ));
            }
        }
    }
}

fn validate_retry_resume_attempt_execution_boundary(
    attempt_plan: &Value,
    retry_plan: &Value,
    prior_retry_execution: &Value,
    prior_retry_execution_path: &Path,
    boundary: &SeniorSweBenchRetryNextCycleBoundary,
) -> Result<(), String> {
    for (field, expected) in [("task_id", &boundary.task_id), ("repo", &boundary.repo)] {
        let retry_value = retry_plan
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!("Senior SWE-Bench retry resume attempt retry plan missing {field}")
            })?;
        let prior_value = prior_retry_execution
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("prior Senior SWE-Bench retry execution missing {field}"))?;
        if retry_value != expected || prior_value != expected {
            return Err(format!(
                "Senior SWE-Bench retry resume attempt {field} metadata does not match prior retry boundary"
            ));
        }
    }
    let retry_max_attempts = retry_plan
        .get("max_attempts")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "Senior SWE-Bench retry resume attempt retry plan missing max_attempts".to_string()
        })?;
    let prior_max_attempts = prior_retry_execution
        .get("max_attempts")
        .and_then(Value::as_u64)
        .ok_or_else(|| "prior Senior SWE-Bench retry execution missing max_attempts".to_string())?;
    if retry_max_attempts != prior_max_attempts
        || retry_max_attempts as usize <= boundary.attempt_index
    {
        return Err(
            "Senior SWE-Bench retry resume attempt retry plan max_attempts does not match prior retry boundary"
                .to_string(),
        );
    }
    if retry_plan
        .get("github_solution_search_allowed")
        .and_then(Value::as_bool)
        != Some(false)
        || prior_retry_execution
            .get("github_solution_search_allowed")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry resume attempt requires no GitHub solution search policy"
                .to_string(),
        );
    }
    let resume_boundary = attempt_plan.get("resume_boundary").ok_or_else(|| {
        "Senior SWE-Bench retry resume attempt plan missing resume_boundary".to_string()
    })?;
    if resume_boundary
        .get("schema_version")
        .and_then(Value::as_str)
        != Some("a2d.senior-swe-bench-retry-resume-boundary.v1")
    {
        return Err(
            "Senior SWE-Bench retry resume attempt plan has invalid resume_boundary schema"
                .to_string(),
        );
    }
    let recorded_retry_execution = resolve_retry_artifact_path(Path::new(&required_plan_string(
        resume_boundary,
        "retry_execution_path",
    )?));
    if !paths_equivalent(&recorded_retry_execution, prior_retry_execution_path) {
        return Err(
            "Senior SWE-Bench retry resume attempt resume_boundary retry_execution_path does not match prior retry boundary"
                .to_string(),
        );
    }
    if let Some(next_cycle_execution_path) = resume_boundary
        .get("next_cycle_execution_path")
        .and_then(Value::as_str)
    {
        let (summary_retry_execution, summary_manifest) =
            load_senior_swe_bench_retry_next_cycle_execution_paths(Path::new(
                next_cycle_execution_path,
            ))?;
        if !paths_equivalent(&summary_retry_execution, prior_retry_execution_path)
            || !paths_equivalent(&summary_manifest, &boundary.expected_manifest)
        {
            return Err(
                "Senior SWE-Bench retry resume attempt next-cycle summary does not match prior retry boundary"
                    .to_string(),
            );
        }
    }
    for (field, expected) in [
        ("task_id", boundary.task_id.as_str()),
        ("repo", boundary.repo.as_str()),
    ] {
        if resume_boundary.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "Senior SWE-Bench retry resume attempt resume_boundary {field} does not match prior retry boundary"
            ));
        }
    }
    if resume_boundary.get("attempt_index").and_then(Value::as_u64)
        != Some(boundary.attempt_index as u64)
    {
        return Err(
            "Senior SWE-Bench retry resume attempt resume_boundary attempt_index does not match prior retry boundary"
                .to_string(),
        );
    }
    for (field, expected) in [
        ("attempt_dir", &boundary.attempt_dir),
        ("task_cycle_input", &boundary.task_cycle_input),
        ("checkout", &boundary.checkout),
        ("output_artifacts_dir", &boundary.output_artifacts_dir),
        ("cycle_output_manifest", &boundary.expected_manifest),
    ] {
        let recorded =
            resolve_retry_artifact_path(Path::new(&required_plan_string(resume_boundary, field)?));
        if !paths_equivalent(&recorded, expected) {
            return Err(format!(
                "Senior SWE-Bench retry resume attempt resume_boundary {field} does not match prior retry boundary"
            ));
        }
    }
    if resume_boundary.get("next_cycle_command") != Some(&boundary.next_cycle_command) {
        return Err(
            "Senior SWE-Bench retry resume attempt resume_boundary next_cycle_command does not match prior retry boundary"
                .to_string(),
        );
    }
    let recorded_manifest_hash =
        required_plan_string(resume_boundary, "cycle_output_manifest_git_object_hash")?;
    validate_git_object_hash(&recorded_manifest_hash).map_err(|error| {
        format!(
            "Senior SWE-Bench retry resume attempt resume_boundary manifest hash {error}: {recorded_manifest_hash}"
        )
    })?;
    let current_manifest_hash = file_content_hash(&boundary.expected_manifest)?;
    if current_manifest_hash != recorded_manifest_hash {
        return Err(format!(
            "Senior SWE-Bench retry resume attempt manifest hash {recorded_manifest_hash} does not match current manifest hash {current_manifest_hash}"
        ));
    }
    let current_selection = select_senior_swe_bench_candidate_artifact(&read_artifact_to_string(
        &boundary.expected_manifest,
    )?)?;
    if attempt_plan.get("selected_artifact") != current_selection.get("selected") {
        return Err(
            "Senior SWE-Bench retry resume attempt selected artifact does not match current manifest"
                .to_string(),
        );
    }
    Ok(())
}

fn preflight_retry_resume_attempt_outputs(
    attempt_plan: &Value,
    attempt_dir: &Path,
) -> Result<(), String> {
    let planned_outputs = attempt_plan.get("planned_outputs").ok_or_else(|| {
        "Senior SWE-Bench retry resume attempt plan missing planned_outputs".to_string()
    })?;
    let selected = attempt_plan.get("selected_artifact").ok_or_else(|| {
        "Senior SWE-Bench retry resume attempt plan missing selected_artifact".to_string()
    })?;
    let selected_path = required_plan_string(selected, "path")?;
    let candidate_patch_path =
        PathBuf::from(required_plan_string(planned_outputs, "candidate_patch")?);
    let planned_local_evaluation_path =
        PathBuf::from(required_plan_string(planned_outputs, "local_evaluation")?);
    ensure_retry_resume_output_under_attempt_dir(
        "candidate_patch",
        &candidate_patch_path,
        attempt_dir,
    )?;
    ensure_retry_resume_output_under_attempt_dir(
        "local_evaluation",
        &planned_local_evaluation_path,
        attempt_dir,
    )?;

    let evaluate_args = attempt_plan
        .get("evaluate_args")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "Senior SWE-Bench retry resume attempt plan missing evaluate_args".to_string()
        })?
        .iter()
        .map(|arg| {
            arg.as_str().map(ToString::to_string).ok_or_else(|| {
                "Senior SWE-Bench retry resume attempt evaluate_args contains non-string"
                    .to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let evaluate_config = validate_retry_attempt_evaluate_args(
        &evaluate_args,
        &selected_path,
        &candidate_patch_path,
    )?;
    let actual_local_evaluation_path = evaluate_config
        .output
        .as_ref()
        .expect("retry-attempt evaluate validation requires output")
        .to_path_buf();
    if !paths_equivalent(
        &actual_local_evaluation_path,
        &planned_local_evaluation_path,
    ) {
        return Err(
            "Senior SWE-Bench retry resume attempt planned local_evaluation does not match evaluate_args --output"
                .to_string(),
        );
    }
    let retry_step_args = attempt_plan
        .get("retry_step_args")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "Senior SWE-Bench retry resume attempt plan missing retry_step_args".to_string()
        })?
        .iter()
        .map(|arg| {
            arg.as_str().map(ToString::to_string).ok_or_else(|| {
                "Senior SWE-Bench retry resume attempt retry_step_args contains non-string"
                    .to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_retry_attempt_retry_step_args(
        &retry_step_args,
        attempt_plan,
        &evaluate_config,
        &actual_local_evaluation_path.to_string_lossy(),
    )?;

    let work_dir = attempt_dir.parent().ok_or_else(|| {
        "Senior SWE-Bench retry resume attempt dir has no parent for summary preflight".to_string()
    })?;
    let mut paths = vec![
        candidate_patch_path,
        planned_local_evaluation_path,
        actual_local_evaluation_path,
        attempt_dir.join("retry-attempt-extraction.json"),
        attempt_dir.join("retry-attempt-evaluation.json"),
        attempt_dir.join("retry-attempt-step-execution.json"),
        attempt_dir.join("retry-attempt-step-evidence-execution.json"),
        attempt_dir.join("retry-run-result.json"),
        attempt_dir.join("next-cycle-input.json"),
        work_dir.join("retry-resume-attempt-execution.json"),
    ];
    paths.sort();
    paths.dedup();
    for path in paths {
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "Senior SWE-Bench retry resume attempt planned output must not be a symlink: {}",
                    path.display()
                ));
            }
            Ok(_) => {
                return Err(format!(
                    "Senior SWE-Bench retry resume attempt planned output already exists: {}",
                    path.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "failed to inspect Senior SWE-Bench retry resume attempt planned output {}: {error}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn ensure_retry_resume_output_under_attempt_dir(
    field: &str,
    path: &Path,
    attempt_dir: &Path,
) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| {
        format!(
            "Senior SWE-Bench retry resume attempt planned output {field} has no parent: {}",
            path.display()
        )
    })?;
    let parent = parent.canonicalize().map_err(|error| {
        format!(
            "failed to canonicalize Senior SWE-Bench retry resume attempt planned output {field} parent {}: {error}",
            parent.display()
        )
    })?;
    let attempt_dir = attempt_dir.canonicalize().map_err(|error| {
        format!(
            "failed to canonicalize Senior SWE-Bench retry resume attempt dir {}: {error}",
            attempt_dir.display()
        )
    })?;
    if parent.starts_with(&attempt_dir) {
        return Ok(());
    }
    Err(format!(
        "Senior SWE-Bench retry resume attempt planned output {field} must be under attempt dir {}: {}",
        attempt_dir.display(),
        path.display()
    ))
}

fn build_senior_swe_bench_retry_next_cycle_execution(
    config: &SeniorSweBenchRetryRunNextCycleConfig,
) -> Result<Value, String> {
    build_senior_swe_bench_retry_next_cycle_execution_with_runner(
        config,
        run_retry_next_cycle_command,
    )
}

fn build_senior_swe_bench_retry_next_gate_execution(
    config: &SeniorSweBenchRetryRunNextGateConfig,
) -> Result<Value, String> {
    build_senior_swe_bench_retry_next_gate_execution_with_runner(
        config,
        run_retry_next_cycle_command,
    )
}

fn build_senior_swe_bench_retry_next_gate_execution_with_runner<F>(
    config: &SeniorSweBenchRetryRunNextGateConfig,
    mut next_cycle_runner: F,
) -> Result<Value, String>
where
    F: FnMut(&[String]) -> Result<RetryNextCycleCommandOutput, String>,
{
    match config {
        SeniorSweBenchRetryRunNextGateConfig::FromRetryExecution(next_cycle_config) => {
            let status_config = SeniorSweBenchRetryStatusConfig {
                retry_execution: next_cycle_config.retry_execution.clone(),
            };
            let before_status = build_senior_swe_bench_retry_status(&status_config)?;
            match before_status.get("next_action").and_then(Value::as_str) {
                Some("run_next_cycle") => {
                    let boundary = load_senior_swe_bench_retry_next_cycle_boundary(
                        &next_cycle_config.retry_execution,
                    )?;
                    let output_path = boundary
                        .attempt_dir
                        .join("retry-next-gate-run-next-cycle.json");
                    preflight_retry_next_gate_output(&output_path)?;
                    let child = build_senior_swe_bench_retry_next_cycle_execution_with_runner(
                        next_cycle_config,
                        &mut next_cycle_runner,
                    )?;
                    let attempt_dir = resolve_retry_artifact_path(Path::new(
                        child
                            .get("output_artifacts_dir")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                "Senior SWE-Bench retry next-cycle child missing output_artifacts_dir"
                                    .to_string()
                            })?,
                    ))
                    .parent()
                    .ok_or_else(|| {
                        "Senior SWE-Bench retry next-cycle child output path has no attempt dir"
                            .to_string()
                    })?
                    .to_path_buf();
                    if !paths_equivalent(&attempt_dir, &boundary.attempt_dir) {
                        return Err(
                            "Senior SWE-Bench retry next-gate child attempt dir changed during execution"
                                .to_string(),
                        );
                    }
                    let child_artifact = attempt_dir.join("retry-next-cycle-execution.json");
                    write_retry_next_gate_execution_artifact(
                        "retry_run_next_cycle",
                        &before_status,
                        &child,
                        &child_artifact,
                        &output_path,
                    )
                }
                Some("completed_success") | Some("stopped") => {
                    let retry_execution_path = &next_cycle_config.retry_execution;
                    let retry_execution_dir = retry_execution_path.parent().ok_or_else(|| {
                        "Senior SWE-Bench retry execution path has no parent".to_string()
                    })?;
                    let output_path =
                        retry_execution_dir.join("retry-next-gate-terminal-status.json");
                    preflight_retry_next_gate_output(&output_path)?;
                    let controller = json!({
                        "schema_version": "a2d.senior-swe-bench-retry-next-gate-execution.v1",
                        "status": "success",
                        "stop_reason": "terminal_retry_status_no_gate_executed",
                        "executed_gate": "none_terminal_status",
                        "before_status": before_status,
                        "child_schema": Value::Null,
                        "child_artifact_path": Value::Null,
                        "controller_artifact_path": output_path.display().to_string(),
                        "provider_invocations_started_by_this_command": false,
                        "cycle_input_command_started_by_this_command": false,
                        "cycle_input_command_spawned_by_this_command": false,
                        "cycle_input_failed_before_provider": false,
                        "provider_invocation_observation": "terminal retry status observed; no cycle-input subprocess boundary is started by this controller gate",
                        "evaluator_invocations_started_by_this_command": false,
                        "fitness_evidence_inspection_started_by_this_command": false,
                        "github_solution_search_allowed": false,
                        "fitness_claim_allowed_before_evidence": false,
                        "fitness_claim_allowed_after_gate": false,
                        "official_senior_swe_bench_mastery": false,
                        "note": "terminal retry status observed; no provider, evaluator, evidence, or retry gate was executed by this command",
                    });
                    write_json_artifact(&output_path, &controller)?;
                    Ok(controller)
                }
                other => Err(format!(
                    "Senior SWE-Bench retry next gate cannot execute unreviewed next_action {other:?}"
                )),
            }
        }
        SeniorSweBenchRetryRunNextGateConfig::FromNextCycleExecution(resume_config) => {
            let plan = build_senior_swe_bench_retry_resume_attempt_plan(resume_config)?;
            let attempt_dir = resolve_retry_artifact_path(Path::new(&required_plan_string(
                &plan,
                "attempt_dir",
            )?));
            let plan_path = attempt_dir.join("retry-attempt-plan.json");
            let output_path = attempt_dir.join("retry-next-gate-resume-plan.json");
            preflight_retry_next_gate_output(&output_path)?;
            write_json_artifact(&plan_path, &plan)?;
            let before_status = json!({
                "next_action": "plan_resume_attempt_from_next_cycle_execution",
                "next_cycle_execution_path": resume_config
                    .next_cycle_execution
                    .as_ref()
                    .map(|path| retry_artifact_path_string(path)),
                "provider_invocations_started": false,
                "evaluator_invocations_started": false,
                "fitness_evidence_inspection_started": false,
                "github_solution_search_allowed": false,
                "fitness_claim_allowed_before_evidence": false,
            });
            write_retry_next_gate_execution_artifact(
                "retry_resume_attempt_plan",
                &before_status,
                &plan,
                &plan_path,
                &output_path,
            )
        }
        SeniorSweBenchRetryRunNextGateConfig::FromResumeAttemptPlan { retry_attempt_plan } => {
            let plan_text = read_artifact_to_string(retry_attempt_plan)?;
            let plan: Value = serde_json::from_str(&plan_text).map_err(|error| {
                format!("invalid Senior SWE-Bench retry attempt plan JSON: {error}")
            })?;
            let attempt_dir = resolve_retry_artifact_path(Path::new(&required_plan_string(
                &plan,
                "attempt_dir",
            )?));
            let work_dir = attempt_dir.parent().ok_or_else(|| {
                "Senior SWE-Bench retry attempt plan attempt_dir has no parent".to_string()
            })?;
            let child_artifact = work_dir.join("retry-resume-attempt-execution.json");
            let output_path = attempt_dir.join("retry-next-gate-resume-execute.json");
            preflight_retry_next_gate_output(&output_path)?;
            let child = build_senior_swe_bench_retry_resume_attempt_execution(&plan_text)?;
            let before_status = json!({
                "next_action": "execute_resume_attempt_plan",
                "retry_attempt_plan": retry_attempt_plan.display().to_string(),
                "provider_invocations_started": false,
                "evaluator_invocations_started": false,
                "fitness_evidence_inspection_started": false,
                "github_solution_search_allowed": false,
                "fitness_claim_allowed_before_evidence": false,
            });
            write_retry_next_gate_execution_artifact(
                "retry_resume_attempt_execute",
                &before_status,
                &child,
                &child_artifact,
                &output_path,
            )
        }
    }
}

fn preflight_retry_next_gate_output(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "Senior SWE-Bench retry next-gate artifact must not be a symlink before child side effects: {}",
                path.display()
            ));
        }
        Ok(_) => {
            return Err(format!(
                "Senior SWE-Bench retry next-gate artifact already exists before child side effects: {}",
                path.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to inspect Senior SWE-Bench retry next-gate artifact {} before child side effects: {error}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn write_retry_next_gate_execution_artifact(
    executed_gate: &str,
    before_status: &Value,
    child: &Value,
    child_artifact_path: &Path,
    controller_artifact_path: &Path,
) -> Result<Value, String> {
    let controller = retry_next_gate_execution_value(
        executed_gate,
        before_status,
        child,
        child_artifact_path,
        controller_artifact_path,
    );
    write_json_artifact(controller_artifact_path, &controller)?;
    Ok(controller)
}

fn retry_next_gate_execution_value(
    executed_gate: &str,
    before_status: &Value,
    child: &Value,
    child_artifact_path: &Path,
    controller_artifact_path: &Path,
) -> Value {
    let child_status = child
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("success");
    let stop_reason =
        child
            .get("stop_reason")
            .and_then(Value::as_str)
            .unwrap_or(match executed_gate {
                "retry_resume_attempt_plan" => "resume_attempt_plan_ready",
                _ => "gate_executed",
            });
    let fitness_claim_allowed_after_gate = child
        .get("fitness_claim_allowed_after_evidence_inspection")
        .and_then(Value::as_bool)
        == Some(true);
    let official_mastery = child
        .get("official_senior_swe_bench_mastery")
        .and_then(Value::as_bool)
        == Some(true)
        && fitness_claim_allowed_after_gate;
    let github_solution_search_allowed = before_status
        .get("github_solution_search_allowed")
        .and_then(Value::as_bool)
        == Some(true)
        || child
            .get("github_solution_search_allowed")
            .and_then(Value::as_bool)
            == Some(true);
    json!({
        "schema_version": "a2d.senior-swe-bench-retry-next-gate-execution.v1",
        "status": child_status,
        "stop_reason": stop_reason,
        "executed_gate": executed_gate,
        "before_status": before_status,
        "child_schema": child.get("schema_version").cloned().unwrap_or(Value::Null),
        "child_artifact_path": child_artifact_path.display().to_string(),
        "controller_artifact_path": controller_artifact_path.display().to_string(),
        "child": child,
        "provider_invocations_started_by_this_command": child
            .get("provider_invocations_started_by_this_command")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "cycle_input_command_started_by_this_command": child
            .get("cycle_input_command_started")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "cycle_input_command_spawned_by_this_command": child
            .get("cycle_input_command_spawned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "cycle_input_failed_before_provider": child
            .get("cycle_input_failed_before_provider")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "provider_invocation_observation": child
            .get("provider_invocation_observation")
            .and_then(Value::as_str)
            .unwrap_or(match executed_gate {
                "retry_run_next_cycle" => "cycle-input subprocess boundary executed; provider activity inside that child is not separately instrumented by the controller",
                _ => "no cycle-input subprocess boundary is started by this controller gate",
            }),
        "evaluator_invocations_started_by_this_command": child
            .get("evaluator_invocations_started")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "fitness_evidence_inspection_started_by_this_command": child
            .get("fitness_evidence_inspection_started")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| fitness_claim_allowed_after_gate),
        "github_solution_search_allowed": github_solution_search_allowed,
        "fitness_claim_allowed_before_evidence": false,
        "fitness_claim_allowed_after_gate": fitness_claim_allowed_after_gate,
        "official_senior_swe_bench_mastery": official_mastery,
        "note": "single Senior SWE-Bench retry controller gate only; this command does not loop or execute the next emitted gate",
    })
}

fn build_senior_swe_bench_retry_next_cycle_execution_with_runner<F>(
    config: &SeniorSweBenchRetryRunNextCycleConfig,
    mut runner: F,
) -> Result<Value, String>
where
    F: FnMut(&[String]) -> Result<RetryNextCycleCommandOutput, String>,
{
    let boundary = load_senior_swe_bench_retry_next_cycle_boundary(&config.retry_execution)?;
    let summary_path = boundary.attempt_dir.join("retry-next-cycle-execution.json");
    if summary_path.exists() {
        return Err(format!(
            "Senior SWE-Bench retry next-cycle execution already exists: {}",
            summary_path.display()
        ));
    }
    if boundary.expected_manifest.exists() {
        return Err(format!(
            "Senior SWE-Bench next cycle manifest already exists before run: {}",
            boundary.expected_manifest.display()
        ));
    }
    let cycle_input_text = read_artifact_to_string(&boundary.task_cycle_input)?;
    validate_cycle_input_bundle(&cycle_input_text)?;
    let package = parse_senior_swe_bench_cycle_input(&cycle_input_text)?;
    if package.task_id != boundary.task_id || package.repo != boundary.repo {
        return Err(format!(
            "Senior SWE-Bench retry execution task {}/{} does not match next cycle input {}/{}",
            boundary.repo, boundary.task_id, package.repo, package.task_id
        ));
    }
    let command_output_result = runner(&boundary.argv);
    let command_output = match command_output_result {
        Ok(output) => output,
        Err(error) => RetryNextCycleCommandOutput {
            exit_code: None,
            stdout: Vec::new(),
            stderr: error.clone().into_bytes(),
            spawn_error: Some(error),
            timed_out: false,
        },
    };
    let command_spawned = command_output.spawn_error.is_none();
    let stdout_preview = preview_text_lossy(&command_output.stdout);
    let stderr_preview = preview_text_lossy(&command_output.stderr);
    let cycle_input_failed_before_provider = command_spawned
        && command_output.exit_code != Some(0)
        && cycle_input_failure_happened_before_provider(&stdout_preview, &stderr_preview);
    let provider_invocations_started_by_this_command =
        command_spawned && !cycle_input_failed_before_provider;
    let provider_invocation_observation = if cycle_input_failed_before_provider {
        "cycle-input subprocess failed during pre-provider validation"
    } else if !command_spawned {
        "cycle-input subprocess did not spawn"
    } else {
        "cycle-input subprocess spawned; provider activity inside that child is possible and not separately instrumented by the controller"
    };
    let manifest_validation = if command_output.exit_code == Some(0) {
        validate_retry_next_cycle_manifest(&boundary.expected_manifest)
    } else {
        Err(format!(
            "persisted next cycle command exited {:?}",
            command_output.exit_code
        ))
    };
    let (status, stop_reason, artifact_count) = match manifest_validation {
        Ok(count) => ("success", "cycle_output_manifest_ready", count),
        Err(_) if command_output.exit_code == Some(0) => {
            ("failed", "cycle_output_manifest_missing_or_invalid", 0)
        }
        Err(_) if command_output.spawn_error.is_some() => {
            ("failed", "cycle_input_command_spawn_failed", 0)
        }
        Err(_) if command_output.timed_out => ("failed", "cycle_input_command_timed_out", 0),
        Err(_) => ("failed", "cycle_input_command_failed", 0),
    };
    let cycle_output_manifest_git_object_hash = if status == "success" {
        Some(file_content_hash(&boundary.expected_manifest)?)
    } else {
        None
    };
    let execution = json!({
        "schema_version": "a2d.senior-swe-bench-retry-next-cycle-execution.v1",
        "status": status,
        "stop_reason": stop_reason,
        "task_id": package.task_id,
        "repo": package.repo,
        "attempt_index": boundary.attempt_index,
        "retry_execution_path": retry_artifact_path_string(&config.retry_execution),
        "next_cycle_command": boundary.next_cycle_command,
        "task_cycle_input": retry_artifact_path_string(&boundary.task_cycle_input),
        "checkout": retry_artifact_path_string(&boundary.checkout),
        "output_artifacts_dir": retry_artifact_path_string(&boundary.output_artifacts_dir),
        "cycle_output_manifest": retry_artifact_path_string(&boundary.expected_manifest),
        "cycle_output_manifest_git_object_hash": cycle_output_manifest_git_object_hash,
        "cycle_output_artifact_count": artifact_count,
        "cycle_input_command_started": command_spawned,
        "cycle_input_command_spawned": command_spawned,
        "cycle_input_command_timed_out": command_output.timed_out,
        "cycle_input_spawn_error": command_output.spawn_error,
        "cycle_input_exit_code": command_output.exit_code,
        "cycle_input_stdout_preview": stdout_preview,
        "cycle_input_stderr_preview": stderr_preview,
        "cycle_input_failed_before_provider": cycle_input_failed_before_provider,
        "provider_invocations_started_by_this_command": provider_invocations_started_by_this_command,
        "provider_invocation_observation": provider_invocation_observation,
        "evaluator_invocations_started": false,
        "fitness_evidence_inspection_started": false,
        "fitness_claim_allowed_before_evidence": false,
        "fitness_claim_allowed_after_cycle": false,
        "github_solution_search_allowed": false,
        "note": "bounded retry next-cycle execution only: runs exactly the persisted cycle-input boundary once and does not evaluate, inspect evidence, or claim fitness",
    });
    write_json_artifact(&summary_path, &execution)?;
    Ok(execution)
}

fn cycle_input_failure_happened_before_provider(
    stdout_preview: &str,
    stderr_preview: &str,
) -> bool {
    if stdout_preview.contains("A²D Catalytic Cycle") {
        return false;
    }
    let stderr = stderr_preview.trim_start();
    [
        "cycle-input --checkout found no bounded UTF-8 source/context files",
        "cycle-input --checkout must not be a symlink",
        "cycle-input --checkout must be a directory",
        "failed to read checkout ",
        "failed to canonicalize checkout ",
        "cycle-input cannot seed reserved runtime artifact",
        "cycle-input requires a JSON object artifact bundle",
    ]
    .iter()
    .any(|pattern| stderr.starts_with(pattern))
}

fn validate_retry_next_cycle_manifest(manifest_path: &Path) -> Result<usize, String> {
    let manifest = read_artifact_to_string(manifest_path)?;
    let value: Value = serde_json::from_str(&manifest)
        .map_err(|error| format!("invalid Senior SWE-Bench next cycle manifest JSON: {error}"))?;
    if value.get("schema_version").and_then(Value::as_str) != Some("a2d.cycle-output-artifacts.v1")
    {
        return Err("Senior SWE-Bench next cycle manifest has invalid schema".to_string());
    }
    let artifacts = value
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| "Senior SWE-Bench next cycle manifest missing artifacts".to_string())?;
    if artifacts.is_empty() {
        return Err("Senior SWE-Bench next cycle manifest has no artifacts".to_string());
    }
    for (index, artifact) in artifacts.iter().enumerate() {
        artifact.as_object().ok_or_else(|| {
            format!("Senior SWE-Bench next cycle manifest artifact {index} is not an object")
        })?;
        let path = required_manifest_string(artifact, "path")?;
        let manifest_hash = required_manifest_string(artifact, "git_object_hash")?;
        validate_git_object_hash(&manifest_hash).map_err(|error| {
            format!(
                "Senior SWE-Bench next cycle manifest artifact {index} git_object_hash {error}: {manifest_hash}"
            )
        })?;
        let bytes = artifact
            .get("bytes")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("Senior SWE-Bench next cycle manifest artifact {index} missing bytes")
            })?;
        let artifact_path = PathBuf::from(&path);
        let manifest_dir = manifest_path
            .parent()
            .ok_or_else(|| "Senior SWE-Bench next cycle manifest path has no parent".to_string())?;
        let artifact_canonical = artifact_path.canonicalize().map_err(|error| {
            format!(
                "failed to canonicalize Senior SWE-Bench next cycle artifact {index} at {path}: {error}"
            )
        })?;
        let manifest_dir_canonical = manifest_dir.canonicalize().map_err(|error| {
            format!(
                "failed to canonicalize Senior SWE-Bench next cycle manifest directory {}: {error}",
                manifest_dir.display()
            )
        })?;
        if !artifact_canonical.starts_with(&manifest_dir_canonical) {
            return Err(format!(
                "Senior SWE-Bench next cycle artifact {index} path {path} is outside output artifact directory {}",
                manifest_dir.display()
            ));
        }
        let artifact_bytes = fs::read(&artifact_path).map_err(|error| {
            format!(
                "failed to read Senior SWE-Bench next cycle artifact {index} at {path}: {error}"
            )
        })?;
        if artifact_bytes.len() as u64 != bytes {
            return Err(format!(
                "Senior SWE-Bench next cycle artifact {index} byte count mismatch for {path}: manifest {bytes}, actual {}",
                artifact_bytes.len()
            ));
        }
        let actual_hash = git_hash_object_bytes(&artifact_bytes)?;
        if actual_hash != manifest_hash {
            return Err(format!(
                "Senior SWE-Bench next cycle artifact {index} hash mismatch for {path}: manifest {manifest_hash}, actual {actual_hash}"
            ));
        }
    }
    Ok(artifacts.len())
}

#[derive(Debug, Clone)]
struct SeniorSweBenchRetryStatusConfig {
    retry_execution: PathBuf,
}

fn run_senior_swe_bench_retry_status(args: &[String]) {
    let config = parse_senior_swe_bench_retry_status_args(args).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench retry status error: {error}");
        eprintln!("Usage: a2d senior-swe-bench-retry-status <retry-execution.json>");
        std::process::exit(1);
    });
    let status = build_senior_swe_bench_retry_status(&config).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench retry status error: {error}");
        std::process::exit(1);
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&status)
            .expect("Senior SWE-Bench retry status must serialize")
    );
}

fn parse_senior_swe_bench_retry_status_args(
    args: &[String],
) -> Result<SeniorSweBenchRetryStatusConfig, String> {
    if args.len() != 1 {
        return Err(
            "Senior SWE-Bench retry status requires exactly one retry execution path".to_string(),
        );
    }
    let supplied_retry_execution = PathBuf::from(&args[0]);
    if supplied_retry_execution == Path::new("-") {
        return Err(
            "Senior SWE-Bench retry status requires a retry execution file path".to_string(),
        );
    }
    let retry_execution = resolve_retry_artifact_path(&supplied_retry_execution);
    if !retry_execution.is_file() {
        return Err(format!(
            "Senior SWE-Bench retry execution not found: {}",
            supplied_retry_execution.display()
        ));
    }
    Ok(SeniorSweBenchRetryStatusConfig { retry_execution })
}

fn build_senior_swe_bench_retry_status(
    config: &SeniorSweBenchRetryStatusConfig,
) -> Result<Value, String> {
    let retry_execution_text = read_artifact_to_string(&config.retry_execution)?;
    let retry_execution: Value = serde_json::from_str(&retry_execution_text)
        .map_err(|error| format!("invalid Senior SWE-Bench retry execution JSON: {error}"))?;
    let schema = retry_execution
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry execution missing schema_version".to_string())?;
    if schema != "a2d.senior-swe-bench-retry-execution.v1" {
        return Err(format!(
            "expected a2d.senior-swe-bench-retry-execution.v1, got {schema}"
        ));
    }
    if retry_execution
        .get("github_solution_search_allowed")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry execution must forbid GitHub solution search".to_string(),
        );
    }
    if retry_execution
        .get("fitness_claim_allowed_before_evidence")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry execution must forbid pre-evidence fitness claims".to_string(),
        );
    }
    let status = retry_execution
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry execution missing status".to_string())?;
    let stop_reason = retry_execution
        .get("stop_reason")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry execution missing stop_reason".to_string())?;
    let attempts_executed = retry_execution
        .get("attempts_executed")
        .and_then(Value::as_u64)
        .ok_or_else(|| "Senior SWE-Bench retry execution missing attempts_executed".to_string())?;
    let attempts = retry_execution
        .get("attempts")
        .and_then(Value::as_array)
        .ok_or_else(|| "Senior SWE-Bench retry execution missing attempts".to_string())?;
    if attempts.len() as u64 != attempts_executed {
        return Err(format!(
            "Senior SWE-Bench retry execution attempts_executed {attempts_executed} does not match attempts length {}",
            attempts.len()
        ));
    }
    for (index, attempt) in attempts.iter().enumerate() {
        if attempt.get("attempt_index").and_then(Value::as_u64) != Some(index as u64) {
            return Err(format!(
                "Senior SWE-Bench retry execution attempt {index} has inconsistent attempt_index"
            ));
        }
    }

    let mut result = json!({
        "schema_version": "a2d.senior-swe-bench-retry-status.v1",
        "retry_execution_path": retry_artifact_path_string(&config.retry_execution),
        "retry_execution_schema": schema,
        "status": status,
        "stop_reason": stop_reason,
        "task_id": retry_execution.get("task_id").cloned().unwrap_or(Value::Null),
        "repo": retry_execution.get("repo").cloned().unwrap_or(Value::Null),
        "attempts_executed": attempts_executed,
        "max_attempts": retry_execution.get("max_attempts").cloned().unwrap_or(Value::Null),
        "provider_invocations_started": retry_execution.get("provider_invocations_started").cloned().unwrap_or(Value::Null),
        "evaluator_invocations_started": retry_execution.get("evaluator_invocations_started").cloned().unwrap_or(Value::Null),
        "github_solution_search_allowed": false,
        "fitness_claim_allowed_before_evidence": false,
        "fitness_evidence_inspection_performed_by_status": false,
        "fitness_claim_allowed_by_status": false,
        "official_senior_swe_bench_mastery": false,
    });

    match status {
        "success" => {
            if retry_execution
                .get("fitness_claim_allowed_after_evidence_inspection")
                .and_then(Value::as_bool)
                != Some(true)
            {
                return Err(
                    "Senior SWE-Bench successful retry execution must be evidence-inspection gated"
                        .to_string(),
                );
            }
            let terminal_run_result =
                retry_execution.get("terminal_run_result").ok_or_else(|| {
                    "Senior SWE-Bench successful retry execution missing terminal_run_result"
                        .to_string()
                })?;
            let final_evidence_path = retry_execution
                .get("final_evidence_path")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "Senior SWE-Bench successful retry execution missing final_evidence_path"
                        .to_string()
                })?;
            let terminal_final_evidence_path = terminal_run_result
                .get("final_evidence_path")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "Senior SWE-Bench successful retry terminal result missing final_evidence_path"
                        .to_string()
                })?;
            if final_evidence_path != terminal_final_evidence_path {
                return Err(
                    "Senior SWE-Bench retry final_evidence_path does not match terminal_run_result"
                        .to_string(),
                );
            }
            for (field, expected) in [
                ("github_solution_search_allowed", false),
                ("fitness_claim_allowed_before_evidence", false),
                ("provider_invocations_started", false),
                ("evaluator_invocations_started", false),
                ("fitness_evidence_inspection_started", true),
                ("fitness_evidence_inspection_passed", true),
                ("fitness_claim_allowed_after_evidence_inspection", true),
            ] {
                if terminal_run_result.get(field).and_then(Value::as_bool) != Some(expected) {
                    return Err(format!(
                        "Senior SWE-Bench retry terminal_run_result.{field} must be {expected}"
                    ));
                }
            }
            let final_evidence_resolved =
                resolve_retry_artifact_path(Path::new(final_evidence_path));
            let evidence_bytes = fs::read(&final_evidence_resolved).map_err(|error| {
                format!(
                    "failed to read Senior SWE-Bench retry final evidence {final_evidence_path}: {error}"
                )
            })?;
            let evidence: Value = serde_json::from_slice(&evidence_bytes).map_err(|error| {
                format!("Senior SWE-Bench retry final evidence is not JSON: {error}")
            })?;
            inspect_fitness_evidence_value(&evidence, true)?;
            let summary = json!({
                "schema_version": evidence.get("schema_version").cloned().unwrap_or(Value::Null),
                "actual_tests_evaluated": evidence.get("actual_tests_evaluated").cloned().unwrap_or(Value::Null),
                "non_regressing": evidence.get("non_regressing").cloned().unwrap_or(Value::Null),
                "fitness": evidence.get("fitness").cloned().unwrap_or(Value::Null),
                "passed": evidence.get("passed").cloned().unwrap_or(Value::Null),
                "failed": evidence.get("failed").cloned().unwrap_or(Value::Null),
                "total": evidence.get("total").cloned().unwrap_or(Value::Null),
                "source_revision": evidence.get("source_revision").cloned().unwrap_or(Value::Null),
                "source_tree_dirty": evidence.get("source_tree_dirty").cloned().unwrap_or(Value::Null),
                "source_diff_hash": evidence.get("source_diff_hash").cloned().unwrap_or(Value::Null),
                "candidate_patch_hash": evidence.get("candidate_patch_hash").cloned().unwrap_or(Value::Null),
                "candidate_patch_path": evidence.get("candidate_patch_path").cloned().unwrap_or(Value::Null),
                "candidate_patch_artifact_path": evidence.get("candidate_patch_artifact_path").cloned().unwrap_or(Value::Null),
                "candidate_patch_artifact_hash": evidence.get("candidate_patch_artifact_hash").cloned().unwrap_or(Value::Null),
                "evaluator_kind": evidence.get("evaluator_kind").cloned().unwrap_or(Value::Null),
            });
            if terminal_run_result.get("fitness_evidence_summary") != Some(&summary) {
                return Err(
                    "Senior SWE-Bench retry status fitness_evidence_summary does not match inspected evidence"
                        .to_string(),
                );
            }
            let evaluator_kind = summary
                .get("evaluator_kind")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "Senior SWE-Bench retry final evidence missing evaluator_kind".to_string()
                })?;
            if !matches!(
                evaluator_kind,
                "provided_local_command" | "official_senior_swe_bench"
            ) {
                return Err(format!(
                    "Senior SWE-Bench retry final evidence has unreviewed evaluator kind: {evaluator_kind}"
                ));
            }
            for (field, value) in [
                (
                    "final_evaluator_kind",
                    retry_execution.get("final_evaluator_kind"),
                ),
                (
                    "terminal_run_result.final_evaluator_kind",
                    terminal_run_result.get("final_evaluator_kind"),
                ),
            ] {
                if value.and_then(Value::as_str) != Some(evaluator_kind) {
                    return Err(format!(
                        "Senior SWE-Bench retry {field} does not match inspected evidence evaluator_kind"
                    ));
                }
            }
            let official_mastery = evaluator_kind == "official_senior_swe_bench";
            for (field, value) in [
                (
                    "official_senior_swe_bench_mastery",
                    retry_execution.get("official_senior_swe_bench_mastery"),
                ),
                (
                    "terminal_run_result.official_senior_swe_bench_mastery",
                    terminal_run_result.get("official_senior_swe_bench_mastery"),
                ),
            ] {
                if value.and_then(Value::as_bool) != Some(official_mastery) {
                    return Err(format!(
                        "Senior SWE-Bench retry {field} does not match inspected evidence evaluator_kind"
                    ));
                }
            }
            result["next_action"] = json!("completed_success");
            result["final_evidence_path"] = json!(final_evidence_path);
            result["final_evaluator_kind"] = json!(evaluator_kind);
            result["fitness_evidence_inspection_performed_by_status"] = json!(true);
            result["fitness_evidence_all_tests_pass_required"] = json!(true);
            result["fitness_evidence_validated_by_status"] = json!(true);
            result["fitness_claim_allowed_by_status"] = json!(true);
            result["authoritative_evidence_gate"] =
                json!("fitness-evidence-inspect --require-all-tests-pass");
            result["official_senior_swe_bench_mastery"] = json!(official_mastery);
            Ok(result)
        }
        "failed" => {
            if retry_execution
                .get("fitness_claim_allowed_after_evidence_inspection")
                .and_then(Value::as_bool)
                != Some(false)
            {
                return Err(
                    "Senior SWE-Bench failed retry execution must not allow post-evidence fitness claims"
                        .to_string(),
                );
            }
            reject_failed_retry_status_fitness_claim_fields(&retry_execution, "retry_execution")?;
            if stop_reason == "precomputed_attempt_manifests_exhausted" {
                let boundary =
                    load_senior_swe_bench_retry_next_cycle_boundary(&config.retry_execution)?;
                let next_gate_command = json!({
                    "command": "a2d",
                    "argv": [
                        "senior-swe-bench-retry-run-next-cycle",
                        "--retry-execution",
                        retry_artifact_path_string(&config.retry_execution),
                    ],
                    "provider_invocations_started": false,
                    "evaluator_invocations_started": false,
                    "fitness_evidence_inspection_started": false,
                    "fitness_claim_allowed_before_evidence": false,
                    "github_solution_search_allowed": false,
                    "retry_execution_path_binding": "repo_relative_paths_resolve_against_a2d_project_root",
                    "note": "status handoff only; running this command may start exactly one bounded cycle-input provider boundary, but this status command has not started it",
                });
                result["next_action"] = json!("run_next_cycle");
                result["next_gate_command"] = next_gate_command;
                result["next_cycle_command"] = boundary.next_cycle_command;
                result["next_cycle_attempt_index"] = json!(boundary.attempt_index);
                result["next_cycle_task_input"] =
                    json!(retry_artifact_path_string(&boundary.task_cycle_input));
                result["next_cycle_expected_manifest"] =
                    json!(retry_artifact_path_string(&boundary.expected_manifest));
            } else {
                result["next_action"] = json!("stopped");
            }
            Ok(result)
        }
        other => Err(format!(
            "Senior SWE-Bench retry execution has unreviewed status: {other}"
        )),
    }
}

fn reject_failed_retry_status_fitness_claim_fields(
    value: &Value,
    location: &str,
) -> Result<(), String> {
    const FORBIDDEN_FIELDS: &[&str] = &[
        "fitness",
        "fitness_delta",
        "fitness_evidence",
        "fitness_evidence_path",
        "final_evidence_path",
        "final_evaluator_kind",
        "fitness_evidence_summary",
        "terminal_run_result",
        "official_senior_swe_bench_mastery",
    ];
    match value {
        Value::Object(object) => {
            for (key, nested) in object {
                if FORBIDDEN_FIELDS.contains(&key.as_str()) {
                    return Err(format!(
                        "Senior SWE-Bench failed retry status must not contain pre-evidence fitness claim field {location}.{key}"
                    ));
                }
                reject_failed_retry_status_fitness_claim_fields(
                    nested,
                    &format!("{location}.{key}"),
                )?;
            }
        }
        Value::Array(items) => {
            for (index, nested) in items.iter().enumerate() {
                reject_failed_retry_status_fitness_claim_fields(
                    nested,
                    &format!("{location}[{index}]"),
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct SeniorSweBenchRetryExecuteConfig {
    retry_plan: PathBuf,
    task_cycle_input: PathBuf,
    checkout: PathBuf,
    work_dir: PathBuf,
    attempt_output_manifests: Vec<PathBuf>,
    apply_candidate_patch: bool,
    official_evaluator_manifest: Option<PathBuf>,
    official_evaluator_manifest_inspection: Option<PathBuf>,
    evaluator_command: Vec<String>,
}

fn run_senior_swe_bench_retry_execute(args: &[String]) {
    let config = parse_senior_swe_bench_retry_execute_args(args).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench retry execute error: {error}");
        eprintln!("Usage: a2d senior-swe-bench-retry-execute --retry-plan <json> --task-cycle-input <json> --checkout <dir> --work-dir <dir> --attempt-output-manifest <manifest.json> [--attempt-output-manifest <manifest.json> ...] [--apply-candidate-patch] [--official-evaluator-manifest <json> --official-evaluator-manifest-inspection <json>] -- <evaluator> [args...]");
        std::process::exit(1);
    });
    let execution = build_senior_swe_bench_retry_execution(&config).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench retry execute error: {error}");
        std::process::exit(1);
    });
    let success = execution.get("status").and_then(Value::as_str) == Some("success");
    println!(
        "{}",
        serde_json::to_string_pretty(&execution)
            .expect("Senior SWE-Bench retry execution must serialize")
    );
    if !success {
        std::process::exit(2);
    }
}

fn parse_senior_swe_bench_retry_execute_args(
    args: &[String],
) -> Result<SeniorSweBenchRetryExecuteConfig, String> {
    let mut retry_plan = None;
    let mut task_cycle_input = None;
    let mut checkout = None;
    let mut work_dir = None;
    let mut attempt_output_manifests = Vec::new();
    let mut apply_candidate_patch = false;
    let mut official_evaluator_manifest = None;
    let mut official_evaluator_manifest_inspection = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--" => {
                let evaluator_command = args[index + 1..].to_vec();
                if evaluator_command.is_empty() {
                    return Err(
                        "Senior SWE-Bench retry execute evaluator command is empty".to_string()
                    );
                }
                let config = SeniorSweBenchRetryExecuteConfig {
                    retry_plan: retry_plan.ok_or_else(|| "missing --retry-plan".to_string())?,
                    task_cycle_input: task_cycle_input
                        .ok_or_else(|| "missing --task-cycle-input".to_string())?,
                    checkout: checkout.ok_or_else(|| "missing --checkout".to_string())?,
                    work_dir: work_dir.ok_or_else(|| "missing --work-dir".to_string())?,
                    attempt_output_manifests,
                    apply_candidate_patch,
                    official_evaluator_manifest,
                    official_evaluator_manifest_inspection,
                    evaluator_command,
                };
                validate_retry_execute_config(&config)?;
                return Ok(config);
            }
            "--retry-plan" => {
                index += 1;
                retry_plan = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--retry-plan requires a path".to_string())?,
                ));
            }
            "--task-cycle-input" => {
                index += 1;
                task_cycle_input =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--task-cycle-input requires a path".to_string()
                    })?));
            }
            "--checkout" => {
                index += 1;
                checkout = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--checkout requires a path".to_string())?,
                ));
            }
            "--work-dir" => {
                index += 1;
                work_dir =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--work-dir requires a directory".to_string()
                    })?));
            }
            "--attempt-output-manifest" => {
                index += 1;
                attempt_output_manifests
                    .push(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--attempt-output-manifest requires a path".to_string()
                    })?));
            }
            "--apply-candidate-patch" => apply_candidate_patch = true,
            "--official-evaluator-manifest" => {
                index += 1;
                official_evaluator_manifest =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--official-evaluator-manifest requires a path".to_string()
                    })?));
            }
            "--official-evaluator-manifest-inspection" => {
                index += 1;
                official_evaluator_manifest_inspection =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--official-evaluator-manifest-inspection requires a path".to_string()
                    })?));
            }
            other => {
                return Err(format!(
                    "unknown senior-swe-bench-retry-execute argument: {other}"
                ));
            }
        }
        index += 1;
    }
    Err("missing -- <evaluator> command".to_string())
}

fn validate_retry_execute_config(config: &SeniorSweBenchRetryExecuteConfig) -> Result<(), String> {
    if config.retry_plan == Path::new("-") || config.task_cycle_input == Path::new("-") {
        return Err(
            "Senior SWE-Bench retry execute requires retry-plan and task-cycle-input file paths"
                .to_string(),
        );
    }
    if config.attempt_output_manifests.is_empty() {
        return Err(
            "Senior SWE-Bench retry execute requires at least one --attempt-output-manifest"
                .to_string(),
        );
    }
    if config
        .attempt_output_manifests
        .iter()
        .any(|path| path == Path::new("-"))
    {
        return Err(
            "Senior SWE-Bench retry execute attempt manifests must be file paths".to_string(),
        );
    }
    if !config.checkout.is_dir() {
        return Err(format!(
            "Senior SWE-Bench retry execute checkout directory not found: {}",
            config.checkout.display()
        ));
    }
    fs::create_dir_all(&config.work_dir).map_err(|error| {
        format!(
            "failed to create Senior SWE-Bench retry execute work dir {}: {error}",
            config.work_dir.display()
        )
    })?;
    match (
        &config.official_evaluator_manifest,
        &config.official_evaluator_manifest_inspection,
    ) {
        (Some(manifest), Some(inspection)) => {
            if !manifest.is_file() {
                return Err(format!(
                    "Senior SWE-Bench official evaluator manifest not found: {}",
                    manifest.display()
                ));
            }
            if !inspection.is_file() {
                return Err(format!(
                    "Senior SWE-Bench official evaluator manifest inspection not found: {}",
                    inspection.display()
                ));
            }
        }
        (Some(_), None) => {
            return Err("Senior SWE-Bench retry execute requires --official-evaluator-manifest-inspection when --official-evaluator-manifest is supplied; run senior-swe-bench-official-evaluator-manifest-inspect first".to_string());
        }
        (None, Some(_)) => {
            return Err("Senior SWE-Bench retry execute --official-evaluator-manifest-inspection requires --official-evaluator-manifest".to_string());
        }
        (None, None) => {}
    }
    Ok(())
}

fn build_senior_swe_bench_retry_execution(
    config: &SeniorSweBenchRetryExecuteConfig,
) -> Result<Value, String> {
    let retry_plan_text = read_artifact_to_string(&config.retry_plan)?;
    let cycle_input_text = read_artifact_to_string(&config.task_cycle_input)?;
    let (retry_plan_value, package) =
        validate_senior_swe_bench_retry_plan_and_cycle_input_for_attempt(
            &retry_plan_text,
            0,
            &cycle_input_text,
        )?;
    let official_manifest_inspection_path =
        persist_retry_execute_official_manifest_inspection(config, &package)?;
    let max_attempts = retry_plan_value
        .get("max_attempts")
        .and_then(Value::as_u64)
        .ok_or_else(|| "Senior SWE-Bench retry plan missing max_attempts".to_string())?
        as usize;
    if config.attempt_output_manifests.len() > max_attempts {
        return Err(format!(
            "Senior SWE-Bench retry execute received more precomputed attempt manifests ({}) than bounded max_attempts ({max_attempts})",
            config.attempt_output_manifests.len()
        ));
    }

    let mut current_cycle_input = config.task_cycle_input.clone();
    let mut attempt_records = Vec::new();
    for (attempt_index, manifest) in config.attempt_output_manifests.iter().enumerate() {
        let attempt_dir = config.work_dir.join(format!("attempt-{attempt_index}"));
        fs::create_dir_all(&attempt_dir).map_err(|error| {
            format!(
                "failed to create Senior SWE-Bench retry attempt dir {}: {error}",
                attempt_dir.display()
            )
        })?;
        let attempt_plan_config = SeniorSweBenchRetryAttemptPlanConfig {
            retry_plan: config.retry_plan.clone(),
            attempt_index,
            task_cycle_input: current_cycle_input.clone(),
            cycle_output_manifest: manifest.clone(),
            checkout: config.checkout.clone(),
            attempt_dir: attempt_dir.clone(),
            apply_candidate_patch: config.apply_candidate_patch,
            official_evaluator_manifest: config.official_evaluator_manifest.clone(),
            official_evaluator_manifest_inspection: official_manifest_inspection_path.clone(),
            evaluator_command: config.evaluator_command.clone(),
        };
        let attempt_plan = build_senior_swe_bench_retry_attempt_plan(&attempt_plan_config)?;
        write_json_artifact(&attempt_dir.join("retry-attempt-plan.json"), &attempt_plan)?;
        let decision = attempt_plan
            .get("decision")
            .and_then(Value::as_str)
            .unwrap_or("<missing>");
        if decision != "extract_and_evaluate_candidate_patch" {
            let record = json!({
                "attempt_index": attempt_index,
                "cycle_output_manifest": manifest,
                "attempt_plan": attempt_plan,
                "decision": "stop",
                "stop_reason": attempt_plan.get("stop_reason").cloned().unwrap_or_else(|| json!("candidate_patch_extraction_failed")),
            });
            attempt_records.push(record);
            let terminal = retry_execute_attach_official_manifest_inspection(
                retry_execution_terminal_result(
                    &retry_plan_value,
                    attempt_records,
                    "failed",
                    "candidate_patch_extraction_failed",
                    None,
                ),
                official_manifest_inspection_path.as_deref(),
            );
            write_json_artifact(&config.work_dir.join("retry-execution.json"), &terminal)?;
            return Ok(terminal);
        }
        let attempt_plan_text = serde_json::to_string(&attempt_plan)
            .map_err(|error| format!("failed to serialize retry attempt plan: {error}"))?;
        let extraction = build_senior_swe_bench_retry_attempt_extraction(&attempt_plan_text)?;
        write_json_artifact(
            &attempt_dir.join("retry-attempt-extraction.json"),
            &extraction,
        )?;
        let extraction_text = serde_json::to_string(&extraction)
            .map_err(|error| format!("failed to serialize retry attempt extraction: {error}"))?;
        let evaluation = build_senior_swe_bench_retry_attempt_evaluation(&extraction_text)?;
        write_json_artifact(
            &attempt_dir.join("retry-attempt-evaluation.json"),
            &evaluation,
        )?;
        let evaluation_text = serde_json::to_string(&evaluation)
            .map_err(|error| format!("failed to serialize retry attempt evaluation: {error}"))?;
        let step_execution = build_senior_swe_bench_retry_attempt_step_execution(&evaluation_text)?;
        write_json_artifact(
            &attempt_dir.join("retry-attempt-step-execution.json"),
            &step_execution,
        )?;
        let retry_step = step_execution
            .get("retry_step")
            .cloned()
            .ok_or_else(|| "Senior SWE-Bench retry execute missing retry_step".to_string())?;
        let retry_decision = retry_step
            .get("decision")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "Senior SWE-Bench retry execute retry_step missing decision".to_string()
            })?
            .to_string();
        let mut record = json!({
            "attempt_index": attempt_index,
            "cycle_output_manifest": manifest,
            "attempt_dir": attempt_dir,
            "candidate_patch_path": extraction.get("candidate_patch_path").cloned().unwrap_or(Value::Null),
            "candidate_patch_hash": extraction.get("candidate_patch_hash").cloned().unwrap_or(Value::Null),
            "local_evaluation_path": evaluation.get("local_evaluation_path").cloned().unwrap_or(Value::Null),
            "local_evaluation_status": evaluation.get("local_evaluation_status").cloned().unwrap_or(Value::Null),
            "evaluate_exit_code": evaluation.get("evaluate_exit_code").cloned().unwrap_or(Value::Null),
            "retry_step_decision": retry_decision,
            "provider_invocations_started": false,
        });
        match retry_decision.as_str() {
            "inspect_fitness_evidence" => {
                let step_execution_text =
                    serde_json::to_string(&step_execution).map_err(|error| {
                        format!("failed to serialize retry attempt step execution: {error}")
                    })?;
                let evidence_execution =
                    build_senior_swe_bench_retry_attempt_step_evidence_execution(
                        &step_execution_text,
                    )?;
                let evidence_execution_text =
                    serde_json::to_string(&evidence_execution).map_err(|error| {
                        format!("failed to serialize retry attempt evidence execution: {error}")
                    })?;
                write_json_artifact(
                    &attempt_dir.join("retry-attempt-step-evidence-execution.json"),
                    &evidence_execution,
                )?;
                let run_result = build_senior_swe_bench_retry_run_result(&evidence_execution_text)?;
                write_json_artifact(&attempt_dir.join("retry-run-result.json"), &run_result)?;
                record["fitness_evidence_path"] = evidence_execution
                    .get("fitness_evidence_path")
                    .cloned()
                    .unwrap_or(Value::Null);
                record["fitness_evidence_inspection_passed"] = json!(true);
                attempt_records.push(record);
                let terminal = retry_execute_attach_official_manifest_inspection(
                    retry_execution_terminal_result(
                        &retry_plan_value,
                        attempt_records,
                        "success",
                        "fitness_evidence_inspection_passed",
                        Some(run_result),
                    ),
                    official_manifest_inspection_path.as_deref(),
                );
                write_json_artifact(&config.work_dir.join("retry-execution.json"), &terminal)?;
                return Ok(terminal);
            }
            "build_next_cycle_input" => {
                let next_cycle_input =
                    retry_step.get("next_cycle_input").cloned().ok_or_else(|| {
                        "Senior SWE-Bench retry execute retry_step missing next_cycle_input"
                            .to_string()
                    })?;
                let next_cycle_input_path = attempt_dir.join("next-cycle-input.json");
                write_json_artifact(&next_cycle_input_path, &next_cycle_input)?;
                let next_cycle_output_dir = config.work_dir.join(format!(
                    "attempt-{}/cycle-output-artifacts",
                    attempt_index + 1
                ));
                let next_cycle_command = retry_execute_next_cycle_command(
                    &next_cycle_input_path,
                    &config.checkout,
                    &next_cycle_output_dir,
                );
                record["next_cycle_input_path"] = json!(next_cycle_input_path);
                record["next_cycle_command"] = next_cycle_command;
                record["fitness_evidence_inspection_passed"] = json!(false);
                attempt_records.push(record);
                current_cycle_input = next_cycle_input_path;
            }
            "stop" => {
                let stop_reason = retry_step
                    .get("stop_reason")
                    .and_then(Value::as_str)
                    .unwrap_or("retry_step_stop");
                record["fitness_evidence_inspection_passed"] = json!(false);
                attempt_records.push(record);
                let terminal = retry_execute_attach_official_manifest_inspection(
                    retry_execution_terminal_result(
                        &retry_plan_value,
                        attempt_records,
                        "failed",
                        stop_reason,
                        None,
                    ),
                    official_manifest_inspection_path.as_deref(),
                );
                write_json_artifact(&config.work_dir.join("retry-execution.json"), &terminal)?;
                return Ok(terminal);
            }
            other => {
                return Err(format!(
                    "Senior SWE-Bench retry execute unreviewed retry-step decision {other}"
                ));
            }
        }
    }

    let stop_reason = if attempt_records.len() >= max_attempts {
        "max_attempts_exhausted"
    } else {
        "precomputed_attempt_manifests_exhausted"
    };
    let terminal = retry_execute_attach_official_manifest_inspection(
        retry_execution_terminal_result(
            &retry_plan_value,
            attempt_records,
            "failed",
            stop_reason,
            None,
        ),
        official_manifest_inspection_path.as_deref(),
    );
    write_json_artifact(&config.work_dir.join("retry-execution.json"), &terminal)?;
    Ok(terminal)
}

fn persist_retry_execute_official_manifest_inspection(
    config: &SeniorSweBenchRetryExecuteConfig,
    package: &SeniorSweBenchTaskPackageSummary,
) -> Result<Option<PathBuf>, String> {
    let Some(inspection_path) = &config.official_evaluator_manifest_inspection else {
        return Ok(None);
    };
    let manifest_path = config.official_evaluator_manifest.as_ref().ok_or_else(|| {
        "Senior SWE-Bench retry execute official inspection requires an official evaluator manifest"
            .to_string()
    })?;
    let inspection_text = read_artifact_to_string(inspection_path)?;
    let inspection: Value = serde_json::from_str(&inspection_text).map_err(|error| {
        format!("invalid Senior SWE-Bench official evaluator manifest inspection JSON: {error}")
    })?;
    let canonical_inspection = validate_retry_execute_official_manifest_inspection(
        &inspection,
        package,
        manifest_path,
        &config.evaluator_command,
    )?;
    let persisted = config
        .work_dir
        .join("official-evaluator-manifest-inspection.json");
    write_json_artifact(&persisted, &canonical_inspection)?;
    Ok(Some(persisted))
}

fn validate_retry_execute_official_manifest_inspection(
    inspection: &Value,
    package: &SeniorSweBenchTaskPackageSummary,
    manifest_path: &Path,
    evaluator_command: &[String],
) -> Result<Value, String> {
    let schema = inspection
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "Senior SWE-Bench official evaluator manifest inspection missing schema_version"
                .to_string()
        })?;
    if schema != "a2d.senior-swe-bench-official-evaluator-manifest-inspection.v1" {
        return Err(format!(
            "expected a2d.senior-swe-bench-official-evaluator-manifest-inspection.v1, got {schema}"
        ));
    }
    if inspection.get("task_id").and_then(Value::as_str) != Some(package.task_id.as_str())
        || inspection.get("repo").and_then(Value::as_str) != Some(package.repo.as_str())
    {
        return Err(
            "Senior SWE-Bench official evaluator manifest inspection task/repo mismatch"
                .to_string(),
        );
    }
    let recorded_manifest_path =
        required_plan_string(inspection, "official_evaluator_manifest_path")?;
    if !paths_equivalent(Path::new(&recorded_manifest_path), manifest_path) {
        return Err(
            "Senior SWE-Bench official evaluator manifest inspection does not match manifest path"
                .to_string(),
        );
    }
    let recorded_hash = required_plan_string(inspection, "official_evaluator_manifest_hash")?;
    let current_hash = file_content_hash(manifest_path)?;
    if recorded_hash != current_hash {
        return Err(format!(
            "Senior SWE-Bench official evaluator manifest inspection hash {recorded_hash} does not match current manifest hash {current_hash}"
        ));
    }
    let manifest_text = read_artifact_to_string(manifest_path)?;
    let manifest = parse_senior_swe_bench_official_evaluator_manifest(
        &manifest_text,
        package,
        evaluator_command,
    )?;
    if inspection
        .get("official_benchmark_url")
        .and_then(Value::as_str)
        != Some(manifest.benchmark_url.as_str())
    {
        return Err(
            "Senior SWE-Bench official evaluator manifest inspection benchmark URL mismatch"
                .to_string(),
        );
    }
    let expected_command = evaluator_command
        .iter()
        .map(|part| Value::String(part.clone()))
        .collect::<Vec<_>>();
    if inspection
        .get("official_benchmark_provided_command")
        .and_then(Value::as_array)
        != Some(&expected_command)
    {
        return Err(
            "Senior SWE-Bench official evaluator manifest inspection command mismatch".to_string(),
        );
    }
    let canonical_inspection = build_senior_swe_bench_official_evaluator_manifest_inspection_value(
        package,
        manifest_path,
        evaluator_command,
    )?;
    for field in [
        "schema_version",
        "task_id",
        "repo",
        "official_evaluator_manifest_hash",
        "official_benchmark_url",
        "official_benchmark_provided_command",
        "note",
    ] {
        if inspection.get(field) != canonical_inspection.get(field) {
            return Err(format!(
                "Senior SWE-Bench official evaluator manifest inspection {field} does not match the current canonical inspection"
            ));
        }
    }
    for field in [
        "official_hidden_holdouts",
        "official_github_solution_search_allowed",
        "provider_invocations_started",
        "evaluator_invocations_started",
        "fitness_evidence_inspection_started",
        "github_solution_search_allowed",
        "fitness_claim_allowed_before_evidence",
        "official_senior_swe_bench_mastery",
    ] {
        if !inspection.get(field).is_some_and(Value::is_boolean) {
            return Err(format!(
                "Senior SWE-Bench official evaluator manifest inspection {field} must be boolean"
            ));
        }
    }
    if inspection
        .get("official_hidden_holdouts")
        .and_then(Value::as_bool)
        != Some(true)
        || inspection
            .get("official_github_solution_search_allowed")
            .and_then(Value::as_bool)
            != Some(false)
        || inspection
            .get("provider_invocations_started")
            .and_then(Value::as_bool)
            != Some(false)
        || inspection
            .get("evaluator_invocations_started")
            .and_then(Value::as_bool)
            != Some(false)
        || inspection
            .get("fitness_evidence_inspection_started")
            .and_then(Value::as_bool)
            != Some(false)
        || inspection
            .get("github_solution_search_allowed")
            .and_then(Value::as_bool)
            != Some(false)
        || inspection
            .get("fitness_claim_allowed_before_evidence")
            .and_then(Value::as_bool)
            != Some(false)
        || inspection
            .get("official_senior_swe_bench_mastery")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err(
            "Senior SWE-Bench official evaluator manifest inspection side-effect/policy flags are unsafe"
                .to_string(),
        );
    }
    Ok(canonical_inspection)
}

fn retry_execute_attach_official_manifest_inspection(
    mut terminal: Value,
    inspection_path: Option<&Path>,
) -> Value {
    if let Some(path) = inspection_path {
        terminal["official_evaluator_manifest_inspection_path"] =
            json!(path.to_string_lossy().to_string());
        terminal["official_evaluator_manifest_inspection_validated"] = json!(true);
    }
    terminal
}

fn retry_execute_next_cycle_command(
    next_cycle_input_path: &Path,
    checkout: &Path,
    output_artifacts_dir: &Path,
) -> Value {
    json!({
        "command": "a2d",
        "argv": [
            "cycle-input",
            retry_artifact_path_string(next_cycle_input_path),
            "1",
            "--checkout",
            retry_artifact_path_string(checkout),
            "--output-artifacts",
            retry_artifact_path_string(output_artifacts_dir),
        ],
        "expected_manifest_path": retry_artifact_path_string(&output_artifacts_dir.join("manifest.json")),
        "provider_invocations_started": false,
        "evaluator_invocations_started": false,
        "fitness_evidence_inspection_started": false,
        "github_solution_search_allowed": false,
        "fitness_claim_allowed_before_evidence": false,
    })
}

fn a2d_project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn normalize_retry_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn retry_absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        normalize_retry_path(path.to_path_buf())
    } else {
        normalize_retry_path(
            env::current_dir()
                .unwrap_or_else(|_| a2d_project_root())
                .join(path),
        )
    }
}

fn retry_artifact_path_string(path: &Path) -> String {
    let absolute = retry_absolute_path(path);
    let project_root = a2d_project_root();
    if let Ok(relative) = absolute.strip_prefix(&project_root) {
        relative.to_string_lossy().replace('\\', "/")
    } else {
        absolute.to_string_lossy().replace('\\', "/")
    }
}

fn resolve_retry_artifact_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return normalize_retry_path(path.to_path_buf());
    }
    let project_candidate = normalize_retry_path(a2d_project_root().join(path));
    if project_candidate.exists() {
        return project_candidate;
    }
    let cwd_candidate = retry_absolute_path(path);
    if cwd_candidate.exists() {
        return cwd_candidate;
    }
    project_candidate
}

fn write_json_artifact(path: &Path, value: &Value) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "Senior SWE-Bench retry execute artifact already exists: {}",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize {}: {error}", path.display()))?;
    let temp = path.with_extension("json.tmp");
    if temp.exists() {
        return Err(format!(
            "Senior SWE-Bench retry execute temp artifact already exists: {}",
            temp.display()
        ));
    }
    fs::write(&temp, bytes)
        .map_err(|error| format!("failed to write {}: {error}", temp.display()))?;
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("failed to finalize {}: {error}", path.display())
    })
}

fn retry_execution_terminal_result(
    retry_plan: &Value,
    attempts: Vec<Value>,
    status: &str,
    stop_reason: &str,
    terminal_run_result: Option<Value>,
) -> Value {
    let evaluator_invocations_started = attempts.iter().any(|attempt| {
        attempt.get("evaluate_exit_code").is_some()
            || attempt.get("local_evaluation_path").is_some()
            || attempt.get("local_evaluation_status").is_some()
    });
    let next_cycle_command = attempts
        .last()
        .and_then(|attempt| attempt.get("next_cycle_command"))
        .cloned();
    let mut result = json!({
        "schema_version": "a2d.senior-swe-bench-retry-execution.v1",
        "status": status,
        "stop_reason": stop_reason,
        "task_id": retry_plan.get("task_id").cloned().unwrap_or(Value::Null),
        "repo": retry_plan.get("repo").cloned().unwrap_or(Value::Null),
        "max_attempts": retry_plan.get("max_attempts").cloned().unwrap_or(Value::Null),
        "attempts_executed": attempts.len(),
        "attempts": attempts,
        "provider_invocations_started": false,
        "evaluator_invocations_started": evaluator_invocations_started,
        "github_solution_search_allowed": false,
        "fitness_claim_allowed_before_evidence": false,
        "fitness_claim_allowed_after_evidence_inspection": status == "success",
        "note": "bounded retry executor summary over precomputed cycle-output manifests; cycle/provider execution remains outside this command, and the underlying a2d.fitness-evidence.v1 remains the authoritative evidence gate",
    });
    if let Some(next_cycle_command) = next_cycle_command {
        result["next_cycle_command"] = next_cycle_command;
    }
    if let Some(run_result) = terminal_run_result {
        result["final_evidence_path"] = run_result
            .get("final_evidence_path")
            .cloned()
            .unwrap_or(Value::Null);
        result["final_evaluator_kind"] = run_result
            .get("final_evaluator_kind")
            .cloned()
            .unwrap_or(Value::Null);
        result["official_senior_swe_bench_mastery"] = run_result
            .get("official_senior_swe_bench_mastery")
            .cloned()
            .unwrap_or_else(|| json!(false));
        result["terminal_run_result"] = run_result;
    }
    result
}

fn build_senior_swe_bench_retry_run_result(step_evidence_execution: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(step_evidence_execution).map_err(|error| {
        format!("invalid Senior SWE-Bench retry attempt step evidence JSON: {error}")
    })?;
    let schema = value
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt step evidence missing schema_version".to_string()
        })?;
    if schema != "a2d.senior-swe-bench-retry-attempt-step-evidence-execution.v1" {
        return Err(format!(
            "expected a2d.senior-swe-bench-retry-attempt-step-evidence-execution.v1, got {schema}"
        ));
    }
    if value
        .get("provider_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err("Senior SWE-Bench retry run result input must not start providers".to_string());
    }
    if value
        .get("evaluator_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry run result input must not start evaluators".to_string(),
        );
    }
    if value
        .get("prior_evaluator_invocations_started")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(
            "Senior SWE-Bench retry run result input must record prior evaluator execution"
                .to_string(),
        );
    }
    if value
        .get("prior_retry_step_started")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(
            "Senior SWE-Bench retry run result input must record prior retry-step execution"
                .to_string(),
        );
    }
    if value
        .get("fitness_evidence_inspection_started")
        .and_then(Value::as_bool)
        != Some(true)
        || value
            .get("fitness_evidence_inspection_passed")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return Err(
            "Senior SWE-Bench retry run result requires passed fitness evidence inspection"
                .to_string(),
        );
    }
    if value
        .get("fitness_claim_allowed_before_evidence")
        .and_then(Value::as_bool)
        != Some(false)
        || value
            .get("fitness_claim_allowed_after_evidence_inspection")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return Err(
            "Senior SWE-Bench retry run result requires the before/after evidence claim boundary"
                .to_string(),
        );
    }
    if value
        .get("github_solution_search_allowed")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry run result input must forbid public GitHub solution search"
                .to_string(),
        );
    }

    let fitness_evidence_path = required_plan_string(&value, "fitness_evidence_path")?;
    let resolved_fitness_evidence_path =
        resolve_retry_artifact_path(Path::new(&fitness_evidence_path));
    let evidence_bytes = fs::read(&resolved_fitness_evidence_path).map_err(|error| {
        format!("failed to read inspected fitness evidence {fitness_evidence_path}: {error}")
    })?;
    let evidence: Value = serde_json::from_slice(&evidence_bytes).map_err(|error| {
        format!("inspected fitness evidence {fitness_evidence_path} is not JSON: {error}")
    })?;
    inspect_fitness_evidence_value(&evidence, true).map_err(|error| {
        format!("inspected fitness evidence {fitness_evidence_path} is invalid: {error}")
    })?;

    let supplied_summary = value.get("fitness_evidence_summary").ok_or_else(|| {
        "Senior SWE-Bench retry run result input missing fitness_evidence_summary".to_string()
    })?;
    let summary = json!({
        "schema_version": evidence.get("schema_version").cloned().unwrap_or(Value::Null),
        "actual_tests_evaluated": evidence.get("actual_tests_evaluated").cloned().unwrap_or(Value::Null),
        "non_regressing": evidence.get("non_regressing").cloned().unwrap_or(Value::Null),
        "fitness": evidence.get("fitness").cloned().unwrap_or(Value::Null),
        "passed": evidence.get("passed").cloned().unwrap_or(Value::Null),
        "failed": evidence.get("failed").cloned().unwrap_or(Value::Null),
        "total": evidence.get("total").cloned().unwrap_or(Value::Null),
        "source_revision": evidence.get("source_revision").cloned().unwrap_or(Value::Null),
        "source_tree_dirty": evidence.get("source_tree_dirty").cloned().unwrap_or(Value::Null),
        "source_diff_hash": evidence.get("source_diff_hash").cloned().unwrap_or(Value::Null),
        "candidate_patch_hash": evidence.get("candidate_patch_hash").cloned().unwrap_or(Value::Null),
        "candidate_patch_path": evidence.get("candidate_patch_path").cloned().unwrap_or(Value::Null),
        "candidate_patch_artifact_path": evidence.get("candidate_patch_artifact_path").cloned().unwrap_or(Value::Null),
        "candidate_patch_artifact_hash": evidence.get("candidate_patch_artifact_hash").cloned().unwrap_or(Value::Null),
        "evaluator_kind": evidence.get("evaluator_kind").cloned().unwrap_or(Value::Null),
    });
    if supplied_summary != &summary {
        return Err(
            "Senior SWE-Bench retry run result fitness_evidence_summary does not match inspected evidence"
                .to_string(),
        );
    }
    let evaluator_kind = summary
        .get("evaluator_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry run result missing evaluator_kind".to_string())?;
    if !matches!(
        evaluator_kind,
        "provided_local_command" | "official_senior_swe_bench"
    ) {
        return Err(format!(
            "Senior SWE-Bench retry run result has unreviewed evaluator_kind {evaluator_kind}"
        ));
    }

    let official_senior_swe_bench_mastery = evaluator_kind == "official_senior_swe_bench";
    let fitness_claim_boundary = if official_senior_swe_bench_mastery {
        "official Senior SWE-Bench success claim is allowed only for the inspected task/evaluator manifest provenance; this is not top-level A²D goal completion"
    } else {
        "provided-local-command evidence passed; do not claim official Senior SWE-Bench mastery"
    };

    Ok(json!({
        "schema_version": "a2d.senior-swe-bench-retry-run-result.v1",
        "status": "success",
        "stop_reason": "fitness_evidence_inspection_passed",
        "task_id": value.get("task_id").cloned().unwrap_or(Value::Null),
        "repo": value.get("repo").cloned().unwrap_or(Value::Null),
        "attempt_index": value.get("attempt_index").cloned().unwrap_or(Value::Null),
        "final_evidence_path": fitness_evidence_path,
        "final_evaluator_kind": evaluator_kind,
        "official_senior_swe_bench_mastery": official_senior_swe_bench_mastery,
        "fitness_claim_allowed_before_evidence": false,
        "fitness_claim_allowed_after_evidence_inspection": true,
        "github_solution_search_allowed": false,
        "provider_invocations_started": false,
        "evaluator_invocations_started": false,
        "prior_evaluator_invocations_started": true,
        "prior_retry_step_started": true,
        "fitness_evidence_inspection_started": true,
        "fitness_evidence_inspection_passed": true,
        "fitness_evidence_summary": summary.clone(),
        "fitness_claim_boundary": fitness_claim_boundary,
        "note": "bounded retry-run result summary only: the underlying inspected a2d.fitness-evidence.v1 remains the authoritative evidence gate, and this is not top-level A²D goal completion",
    }))
}

fn validate_retry_attempt_step_evidence_retry_step(
    retry_step: &Value,
    step_execution: &Value,
) -> Result<(), String> {
    let schema = retry_step
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry-step output missing schema_version".to_string())?;
    if schema != "a2d.senior-swe-bench-cycle-retry-step.v1" {
        return Err(format!(
            "expected a2d.senior-swe-bench-cycle-retry-step.v1 retry-step output, got {schema}"
        ));
    }
    for field in ["task_id", "repo", "attempt_index"] {
        if retry_step.get(field) != step_execution.get(field) {
            return Err(format!(
                "Senior SWE-Bench retry-step output {field} does not match retry attempt step execution"
            ));
        }
    }
    if retry_step
        .get("provider_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err("Senior SWE-Bench retry-step output must not start providers".to_string());
    }
    if retry_step
        .get("evaluator_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err("Senior SWE-Bench retry-step output must not start evaluators".to_string());
    }
    if retry_step
        .get("fitness_claim_allowed_before_evidence")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry-step output must forbid fitness claims before evidence"
                .to_string(),
        );
    }
    if retry_step
        .get("github_solution_search_allowed")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry-step output must forbid public GitHub solution search"
                .to_string(),
        );
    }
    if retry_step.get("decision").and_then(Value::as_str) != Some("inspect_fitness_evidence") {
        return Err("Senior SWE-Bench retry attempt evidence inspection requires retry-step decision inspect_fitness_evidence".to_string());
    }
    Ok(())
}

fn validate_retry_attempt_step_output(
    retry_step: &Value,
    evaluation: &Value,
    local_evaluation: &Value,
    local_status: &str,
) -> Result<(), String> {
    let schema = retry_step
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry-step output missing schema_version".to_string())?;
    if schema != "a2d.senior-swe-bench-cycle-retry-step.v1" {
        return Err(format!(
            "expected a2d.senior-swe-bench-cycle-retry-step.v1 retry-step output, got {schema}"
        ));
    }
    for field in ["task_id", "repo", "attempt_index"] {
        if retry_step.get(field) != evaluation.get(field) {
            return Err(format!(
                "Senior SWE-Bench retry-step output {field} does not match retry attempt evaluation"
            ));
        }
    }
    if retry_step.get("evaluation_status").and_then(Value::as_str) != Some(local_status) {
        return Err(
            "Senior SWE-Bench retry-step output evaluation_status does not match local evaluation"
                .to_string(),
        );
    }
    if retry_step
        .get("provider_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err("Senior SWE-Bench retry-step output must not start providers".to_string());
    }
    if retry_step
        .get("evaluator_invocations_started")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err("Senior SWE-Bench retry-step output must not start evaluators".to_string());
    }
    if retry_step
        .get("fitness_claim_allowed_before_evidence")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry-step output must forbid fitness claims before evidence"
                .to_string(),
        );
    }
    if retry_step
        .get("github_solution_search_allowed")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench retry-step output must forbid public GitHub solution search"
                .to_string(),
        );
    }
    let decision = retry_step
        .get("decision")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench retry-step output missing decision".to_string())?;
    match decision {
        "build_next_cycle_input" => {
            if retry_step.get("next_cycle_input").is_none() {
                return Err(
                    "Senior SWE-Bench retry-step output build_next_cycle_input missing next_cycle_input"
                        .to_string(),
                );
            }
        }
        "inspect_fitness_evidence" => {
            let args = retry_step
                .get("fitness_evidence_inspect_args")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    "Senior SWE-Bench retry-step output inspect decision missing fitness_evidence_inspect_args"
                        .to_string()
                })?;
            let Some(step_path) = retry_step
                .get("fitness_evidence_path")
                .and_then(Value::as_str)
            else {
                return Err("Senior SWE-Bench retry-step output inspect decision missing fitness_evidence_path".to_string());
            };
            let local_path = local_evaluation
                .get("fitness_evidence_path")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "Senior SWE-Bench retry-step output inspect decision but local evaluation missing fitness_evidence_path".to_string()
                })?;
            if step_path != local_path {
                return Err("Senior SWE-Bench retry-step output fitness_evidence_path does not match local evaluation".to_string());
            }
            if args.len() != 3
                || args[0].as_str() != Some("fitness-evidence-inspect")
                || args[1].as_str() != Some(step_path)
                || args[2].as_str() != Some("--require-all-tests-pass")
            {
                return Err("Senior SWE-Bench retry-step output has invalid fitness evidence inspection args".to_string());
            }
        }
        "stop" => {
            required_non_empty_json_string(retry_step, "stop_reason")?;
        }
        other => {
            return Err(format!(
                "Senior SWE-Bench retry-step output contains unknown decision {other}"
            ));
        }
    }
    Ok(())
}

fn validate_retry_attempt_evaluate_args(
    evaluate_args: &[String],
    selected_path: &str,
    candidate_patch_path: &Path,
) -> Result<SeniorSweBenchEvaluateConfig, String> {
    if evaluate_args.first().map(String::as_str) != Some("senior-swe-bench-evaluate") {
        return Err(
            "Senior SWE-Bench retry attempt evaluate_args must start with senior-swe-bench-evaluate"
                .to_string(),
        );
    }
    reject_duplicate_retry_attempt_flags(
        "evaluate_args",
        evaluate_args,
        &[
            "--task-package",
            "--task-cycle-input",
            "--candidate-patch",
            "--candidate-patch-artifact",
            "--extracted-candidate-patch",
            "--checkout",
            "--output",
            "--apply-candidate-patch",
            "--official-evaluator-manifest",
            "--official-evaluator-manifest-inspection",
        ],
    )?;
    if !evaluate_args.iter().any(|arg| arg == "--") {
        return Err(
            "Senior SWE-Bench retry attempt evaluate_args missing evaluator separator".to_string(),
        );
    }
    let config = parse_senior_swe_bench_evaluate_args(&evaluate_args[1..]).map_err(|error| {
        format!(
            "Senior SWE-Bench retry attempt evaluate_args are not valid evaluator args: {error}"
        )
    })?;
    if config.candidate_patch_artifact.as_ref() != Some(&PathBuf::from(selected_path)) {
        return Err("Senior SWE-Bench retry attempt evaluate_args candidate artifact does not match extraction".to_string());
    }
    if config.extracted_candidate_patch.as_ref() != Some(&candidate_patch_path.to_path_buf()) {
        return Err("Senior SWE-Bench retry attempt evaluate_args extracted candidate patch does not match extraction".to_string());
    }
    let Some(output) = &config.output else {
        return Err(
            "Senior SWE-Bench retry attempt evaluate_args --output must be a file path".to_string(),
        );
    };
    if output.as_os_str() == "-" {
        return Err(
            "Senior SWE-Bench retry attempt evaluate_args --output must be a file path".to_string(),
        );
    }
    if config.task_cycle_input.is_none() {
        return Err(
            "Senior SWE-Bench retry attempt evaluate_args must use --task-cycle-input".to_string(),
        );
    }
    Ok(config)
}

fn validate_retry_attempt_retry_step_args(
    retry_step_args: &[String],
    extraction: &Value,
    evaluate_config: &SeniorSweBenchEvaluateConfig,
    local_evaluation_path: &str,
) -> Result<(), String> {
    if retry_step_args.first().map(String::as_str) != Some("senior-swe-bench-retry-step") {
        return Err(
            "Senior SWE-Bench retry attempt retry_step_args must start with senior-swe-bench-retry-step"
                .to_string(),
        );
    }
    reject_duplicate_retry_attempt_flags(
        "retry_step_args",
        retry_step_args,
        &[
            "--retry-plan",
            "--attempt-index",
            "--task-cycle-input",
            "--local-evaluation",
        ],
    )?;
    if retry_step_args.iter().any(|arg| arg == "--") {
        return Err(
            "Senior SWE-Bench retry attempt retry_step_args must not contain a command separator"
                .to_string(),
        );
    }
    let mut index = 1usize;
    while index < retry_step_args.len() {
        match retry_step_args[index].as_str() {
            "--retry-plan" | "--attempt-index" | "--task-cycle-input" | "--local-evaluation" => {
                index += 1;
                if index >= retry_step_args.len() || retry_step_args[index].starts_with("--") {
                    return Err(format!(
                        "Senior SWE-Bench retry attempt retry_step_args {} requires a value",
                        retry_step_args[index - 1]
                    ));
                }
            }
            other => {
                return Err(format!(
                    "Senior SWE-Bench retry attempt retry_step_args contains unknown argument {other}"
                ));
            }
        }
        index += 1;
    }
    let retry_plan = retry_attempt_arg_value(retry_step_args, "--retry-plan")?;
    if retry_plan == "-" {
        return Err(
            "Senior SWE-Bench retry attempt retry_step_args --retry-plan must be a file path"
                .to_string(),
        );
    }
    let attempt_index = retry_attempt_arg_value(retry_step_args, "--attempt-index")?;
    let planned_attempt_index = extraction
        .get("attempt_index")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt extraction missing attempt_index".to_string()
        })?;
    if attempt_index.parse::<u64>().ok() != Some(planned_attempt_index) {
        return Err("Senior SWE-Bench retry attempt retry_step_args attempt index does not match extraction".to_string());
    }
    let task_cycle_input = retry_attempt_arg_value(retry_step_args, "--task-cycle-input")?;
    let expected_cycle_input = evaluate_config
        .task_cycle_input
        .as_ref()
        .ok_or_else(|| {
            "Senior SWE-Bench retry attempt evaluate_args missing task-cycle-input".to_string()
        })?
        .to_string_lossy()
        .to_string();
    if task_cycle_input != expected_cycle_input {
        return Err("Senior SWE-Bench retry attempt retry_step_args task-cycle-input does not match evaluate_args".to_string());
    }
    if retry_attempt_arg_value(retry_step_args, "--local-evaluation")? != local_evaluation_path {
        return Err("Senior SWE-Bench retry attempt retry_step_args local-evaluation does not match evaluate output".to_string());
    }
    Ok(())
}

fn reject_duplicate_retry_attempt_flags(
    label: &str,
    args: &[String],
    flags: &[&str],
) -> Result<(), String> {
    let pre_separator = args
        .iter()
        .position(|arg| arg == "--")
        .map(|index| &args[..index])
        .unwrap_or(args);
    for flag in flags {
        let count = pre_separator
            .iter()
            .filter(|arg| arg.as_str() == *flag)
            .count();
        if count > 1 {
            return Err(format!(
                "Senior SWE-Bench retry attempt {label} contains duplicate {flag}"
            ));
        }
    }
    Ok(())
}

fn retry_attempt_arg_value(args: &[String], flag: &str) -> Result<String, String> {
    args.windows(2)
        .find_map(|window| {
            if window[0] == flag {
                Some(window[1].clone())
            } else {
                None
            }
        })
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Senior SWE-Bench retry attempt args missing {flag}"))
}

fn validate_retry_attempt_local_evaluation(
    path: &Path,
    extraction: &Value,
    candidate_patch_path: &Path,
    candidate_patch_hash: &str,
    evaluate_config: &SeniorSweBenchEvaluateConfig,
    wrapper_exit_code: Option<i32>,
) -> Result<Value, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "planned senior-swe-bench-evaluate did not write local evaluation {}: {error}",
            path.display()
        )
    })?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "planned senior-swe-bench-evaluate wrote invalid local evaluation JSON {}: {error}",
            path.display()
        )
    })?;
    let schema = value
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench local evaluation missing schema_version".to_string())?;
    if schema != "a2d.senior-swe-bench-local-evaluation.v1" {
        return Err(format!(
            "expected a2d.senior-swe-bench-local-evaluation.v1, got {schema}"
        ));
    }
    for field in ["task_id", "repo"] {
        if extraction.get(field).and_then(Value::as_str) != value.get(field).and_then(Value::as_str)
        {
            return Err(format!(
                "Senior SWE-Bench local evaluation {field} does not match retry attempt extraction"
            ));
        }
    }
    let expected_evaluator = if evaluate_config.official_evaluator_manifest.is_some() {
        "official_senior_swe_bench"
    } else {
        "provided_local_command"
    };
    if value.get("evaluator").and_then(Value::as_str) != Some(expected_evaluator) {
        return Err(
            "Senior SWE-Bench local evaluation evaluator does not match evaluate_args".to_string(),
        );
    }
    validate_retry_attempt_local_official_provenance(&value, evaluate_config, extraction)?;
    if value
        .get("github_solution_search_allowed")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "Senior SWE-Bench local evaluation must forbid public GitHub solution search"
                .to_string(),
        );
    }
    if value.get("candidate_patch").and_then(Value::as_str)
        != Some(candidate_patch_path.to_string_lossy().as_ref())
    {
        return Err(
            "Senior SWE-Bench local evaluation candidate_patch does not match extraction"
                .to_string(),
        );
    }
    if value.get("candidate_patch_hash").and_then(Value::as_str) != Some(candidate_patch_hash) {
        return Err(
            "Senior SWE-Bench local evaluation candidate_patch_hash does not match extraction"
                .to_string(),
        );
    }
    let expected_checkout_mode = if evaluate_config.apply_candidate_patch {
        "isolated_copy"
    } else {
        "supplied_checkout"
    };
    if value
        .get("candidate_patch_applied")
        .and_then(Value::as_bool)
        != Some(evaluate_config.apply_candidate_patch)
    {
        return Err("Senior SWE-Bench local evaluation candidate_patch_applied does not match evaluate_args".to_string());
    }
    if value.get("evaluator_checkout_mode").and_then(Value::as_str) != Some(expected_checkout_mode)
    {
        return Err("Senior SWE-Bench local evaluation evaluator_checkout_mode does not match evaluate_args".to_string());
    }
    if value
        .get("original_checkout_mutated")
        .and_then(Value::as_bool)
        .is_none()
    {
        return Err(
            "Senior SWE-Bench local evaluation missing original_checkout_mutated".to_string(),
        );
    }
    if evaluate_config.apply_candidate_patch
        && value
            .get("original_checkout_mutated")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err(
            "Senior SWE-Bench local evaluation mutated original checkout in isolated mode"
                .to_string(),
        );
    }
    if value
        .get("candidate_patch_preflight_checked")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(
            "Senior SWE-Bench local evaluation missing passed candidate patch preflight"
                .to_string(),
        );
    }
    if value
        .get("candidate_patch_preflight_status")
        .and_then(Value::as_str)
        != Some("passed")
    {
        return Err(
            "Senior SWE-Bench local evaluation candidate patch preflight did not pass".to_string(),
        );
    }
    if !value
        .get("candidate_patch_preflight_command")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .contains("git apply --check")
    {
        return Err(
            "Senior SWE-Bench local evaluation candidate patch preflight command is missing"
                .to_string(),
        );
    }
    if value.get("checkout").and_then(Value::as_str)
        != Some(evaluate_config.checkout.to_string_lossy().as_ref())
    {
        return Err(
            "Senior SWE-Bench local evaluation checkout does not match evaluate_args".to_string(),
        );
    }
    let evaluator_command = value
        .get("evaluator_command")
        .and_then(Value::as_array)
        .ok_or_else(|| "Senior SWE-Bench local evaluation missing evaluator_command".to_string())?
        .iter()
        .map(|arg| {
            arg.as_str().map(ToString::to_string).ok_or_else(|| {
                "Senior SWE-Bench local evaluation evaluator_command contains non-string"
                    .to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if evaluator_command != evaluate_config.command {
        return Err(
            "Senior SWE-Bench local evaluation evaluator_command does not match evaluate_args"
                .to_string(),
        );
    }
    validate_retry_attempt_local_source_provenance(&value)?;
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| "Senior SWE-Bench local evaluation missing status".to_string())?;
    match (wrapper_exit_code, status) {
        (Some(0), "passed") | (Some(2), "failed") => Ok(value),
        _ => Err(format!(
            "Senior SWE-Bench local evaluation status {status} does not match wrapper exit {:?}",
            wrapper_exit_code
        )),
    }
}

fn validate_retry_attempt_local_official_provenance(
    value: &Value,
    evaluate_config: &SeniorSweBenchEvaluateConfig,
    extraction: &Value,
) -> Result<(), String> {
    let official_fields = [
        "official_evaluator_manifest_path",
        "official_evaluator_manifest_hash",
        "official_evaluator_manifest_inspection_path",
        "official_evaluator_manifest_inspection_hash",
        "official_evaluator_manifest_inspection_validated",
        "official_benchmark_url",
        "official_task_id",
        "official_repo",
        "official_hidden_holdouts",
        "official_github_solution_search_allowed",
        "official_benchmark_provided_command",
    ];
    let Some(manifest_path) = &evaluate_config.official_evaluator_manifest else {
        if official_fields
            .iter()
            .any(|field| value.get(field).is_some())
        {
            return Err(
                "Senior SWE-Bench local evaluation includes official provenance for non-official evaluator"
                    .to_string(),
            );
        }
        return Ok(());
    };

    for field in official_fields {
        if value.get(field).is_none() {
            return Err(format!(
                "official Senior SWE-Bench local evaluation missing {field}"
            ));
        }
    }
    let local_manifest_path =
        required_non_empty_json_string(value, "official_evaluator_manifest_path")?;
    if !paths_equivalent(Path::new(&local_manifest_path), manifest_path) {
        return Err(
            "official Senior SWE-Bench local evaluation manifest path does not match evaluate_args"
                .to_string(),
        );
    }
    let expected_manifest_hash = file_content_hash(manifest_path)?;
    if value
        .get("official_evaluator_manifest_hash")
        .and_then(Value::as_str)
        != Some(expected_manifest_hash.as_str())
    {
        return Err(
            "official Senior SWE-Bench local evaluation manifest hash does not match manifest file"
                .to_string(),
        );
    }
    let inspection_path = evaluate_config
        .official_evaluator_manifest_inspection
        .as_ref()
        .ok_or_else(|| {
            "official Senior SWE-Bench local evaluation evaluate_args missing inspection path"
                .to_string()
        })?;
    let local_inspection_path =
        required_non_empty_json_string(value, "official_evaluator_manifest_inspection_path")?;
    if !paths_equivalent(Path::new(&local_inspection_path), inspection_path) {
        return Err(
            "official Senior SWE-Bench local evaluation inspection path does not match evaluate_args"
                .to_string(),
        );
    }
    let expected_inspection_hash = file_content_hash(inspection_path)?;
    if value
        .get("official_evaluator_manifest_inspection_hash")
        .and_then(Value::as_str)
        != Some(expected_inspection_hash.as_str())
    {
        return Err(
            "official Senior SWE-Bench local evaluation inspection hash does not match inspection file"
                .to_string(),
        );
    }
    if value
        .get("official_evaluator_manifest_inspection_validated")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(
            "official Senior SWE-Bench local evaluation inspection was not validated".to_string(),
        );
    }
    let official_task_id = required_non_empty_json_string(value, "official_task_id")?;
    if extraction.get("task_id").and_then(Value::as_str) != Some(official_task_id.as_str()) {
        return Err(
            "official Senior SWE-Bench local evaluation task_id does not match retry attempt"
                .to_string(),
        );
    }
    let official_repo = required_non_empty_json_string(value, "official_repo")?;
    if extraction.get("repo").and_then(Value::as_str) != Some(official_repo.as_str()) {
        return Err(
            "official Senior SWE-Bench local evaluation repo does not match retry attempt"
                .to_string(),
        );
    }
    if value
        .get("official_hidden_holdouts")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(
            "official Senior SWE-Bench local evaluation must declare hidden holdouts".to_string(),
        );
    }
    if value
        .get("official_github_solution_search_allowed")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(
            "official Senior SWE-Bench local evaluation must forbid GitHub solution search"
                .to_string(),
        );
    }
    let command = value
        .get("official_benchmark_provided_command")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "official Senior SWE-Bench local evaluation benchmark command is not an array"
                .to_string()
        })?
        .iter()
        .map(|arg| {
            arg.as_str().map(ToString::to_string).ok_or_else(|| {
                "official Senior SWE-Bench local evaluation benchmark command contains non-string"
                    .to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if command != evaluate_config.command {
        return Err(
            "official Senior SWE-Bench local evaluation benchmark command does not match evaluate_args"
                .to_string(),
        );
    }
    required_non_empty_json_string(value, "official_benchmark_url")?;
    Ok(())
}

fn validate_retry_attempt_local_source_provenance(value: &Value) -> Result<(), String> {
    required_non_empty_json_string(value, "source_revision")?;
    if value
        .get("source_tree_dirty")
        .and_then(Value::as_bool)
        .is_none()
    {
        return Err("Senior SWE-Bench local evaluation missing source_tree_dirty".to_string());
    }
    if value.get("source_diff_scope").and_then(Value::as_str) != Some("crates") {
        return Err(
            "Senior SWE-Bench local evaluation source_diff_scope must be crates".to_string(),
        );
    }
    let source_diff_hash = required_non_empty_json_string(value, "source_diff_hash")?;
    validate_git_object_hash(&source_diff_hash).map_err(|error| {
        format!("Senior SWE-Bench local evaluation source_diff_hash {error}: {source_diff_hash}")
    })?;
    required_non_empty_json_string(value, "evidence_command")?;

    let current_revision = git_scope_revision("crates")?;
    if value.get("source_revision").and_then(Value::as_str) != Some(current_revision.as_str()) {
        return Err(format!(
            "Senior SWE-Bench local evaluation source_revision does not match current crates revision {current_revision}"
        ));
    }
    let current_status = git_status_for_scope("crates")?;
    reject_untracked_source_files("crates", &current_status)?;
    let current_dirty = !current_status.is_empty();
    if value.get("source_tree_dirty").and_then(Value::as_bool) != Some(current_dirty) {
        return Err(format!(
            "Senior SWE-Bench local evaluation source_tree_dirty does not match current dirty status {current_dirty}"
        ));
    }
    let current_diff_hash = git_diff_hash_for_scope("crates")?;
    if source_diff_hash != current_diff_hash {
        return Err(format!(
            "Senior SWE-Bench local evaluation source_diff_hash {source_diff_hash} does not match current crates diff hash {current_diff_hash}"
        ));
    }
    Ok(())
}

fn required_non_empty_json_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| format!("Senior SWE-Bench local evaluation missing {field}"))
}

fn preview_text_lossy(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).replace('\n', "\\n");
    let mut preview: String = text.chars().take(500).collect();
    if text.chars().count() > 500 {
        preview.push_str("...");
    }
    preview
}

fn required_plan_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| format!("Senior SWE-Bench retry attempt plan missing {field}"))
}

fn write_candidate_patch_idempotently(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.exists() {
        let existing = fs::read(path).map_err(|error| {
            format!(
                "failed to read existing candidate patch {}: {error}",
                path.display()
            )
        })?;
        if existing == bytes {
            return Ok(());
        }
        return Err(format!(
            "candidate patch {} already exists with different bytes",
            path.display()
        ));
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create candidate patch parent {}: {error}",
                parent.display()
            )
        })?;
    }
    let temp = path.with_extension(format!(
        "tmp-{}",
        UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&temp, bytes).map_err(|error| {
        format!(
            "failed to write temp candidate patch {}: {error}",
            temp.display()
        )
    })?;
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!(
            "failed to move temp candidate patch {} to {}: {error}",
            temp.display(),
            path.display()
        )
    })
}

fn run_senior_swe_bench_retry_step(args: &[String]) {
    let mut retry_plan_path = None;
    let mut attempt_index = None;
    let mut cycle_input_path = None;
    let mut evaluation_path = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--retry-plan" => {
                index += 1;
                retry_plan_path = args.get(index).map(String::as_str);
            }
            "--attempt-index" => {
                index += 1;
                let raw = args.get(index).unwrap_or_else(|| {
                    eprintln!("--attempt-index requires a value");
                    std::process::exit(1);
                });
                attempt_index = Some(raw.parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("--attempt-index must be an integer");
                    std::process::exit(1);
                }));
            }
            "--task-cycle-input" => {
                index += 1;
                cycle_input_path = args.get(index).map(String::as_str);
            }
            "--local-evaluation" => {
                index += 1;
                evaluation_path = args.get(index).map(String::as_str);
            }
            other => {
                eprintln!("unknown senior-swe-bench-retry-step argument: {other}");
                std::process::exit(1);
            }
        }
        index += 1;
    }
    let Some(retry_plan_path) = retry_plan_path else {
        eprintln!(
            "Usage: a2d senior-swe-bench-retry-step --retry-plan <json|-> --attempt-index <n> --task-cycle-input <json|-> --local-evaluation <json|->"
        );
        std::process::exit(1);
    };
    let Some(attempt_index) = attempt_index else {
        eprintln!(
            "Usage: a2d senior-swe-bench-retry-step --retry-plan <json|-> --attempt-index <n> --task-cycle-input <json|-> --local-evaluation <json|->"
        );
        std::process::exit(1);
    };
    let Some(cycle_input_path) = cycle_input_path else {
        eprintln!(
            "Usage: a2d senior-swe-bench-retry-step --retry-plan <json|-> --attempt-index <n> --task-cycle-input <json|-> --local-evaluation <json|->"
        );
        std::process::exit(1);
    };
    let Some(evaluation_path) = evaluation_path else {
        eprintln!(
            "Usage: a2d senior-swe-bench-retry-step --retry-plan <json|-> --attempt-index <n> --task-cycle-input <json|-> --local-evaluation <json|->"
        );
        std::process::exit(1);
    };
    let retry_plan = read_artifact_or_exit(retry_plan_path);
    let cycle_input = read_artifact_or_exit(cycle_input_path);
    let local_evaluation = read_artifact_or_exit(evaluation_path);
    let step = build_senior_swe_bench_cycle_retry_step(
        &retry_plan,
        attempt_index,
        &cycle_input,
        &local_evaluation,
    )
    .unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench retry step error: {error}");
        std::process::exit(1);
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&step).expect("Senior SWE-Bench retry step must serialize")
    );
}

fn select_senior_swe_bench_candidate_artifact(manifest: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(manifest)
        .map_err(|error| format!("invalid cycle output artifact manifest JSON: {error}"))?;
    let schema = value
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "cycle output artifact manifest missing schema_version".to_string())?;
    if schema != "a2d.cycle-output-artifacts.v1" {
        return Err(format!(
            "expected a2d.cycle-output-artifacts.v1 manifest, got {schema}"
        ));
    }
    let artifacts = value
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| "cycle output artifact manifest missing artifacts array".to_string())?;
    let candidates = artifacts
        .iter()
        .filter(|entry| {
            entry.get("enzyme_id").and_then(Value::as_str) == Some("coder")
                && entry.get("artifact_type").and_then(Value::as_str) == Some("code")
        })
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err(format!(
            "expected exactly one coder/code candidate artifact, found {}",
            candidates.len()
        ));
    }
    let candidate = candidates[0];
    let path = required_manifest_string(candidate, "path")?;
    let manifest_hash = required_manifest_string(candidate, "git_object_hash")?;
    validate_git_object_hash(&manifest_hash).map_err(|error| {
        format!("cycle output artifact manifest git_object_hash {error}: {manifest_hash}")
    })?;
    let bytes = candidate
        .get("bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| "cycle output artifact candidate missing bytes".to_string())?;
    let artifact_bytes = fs::read(&path)
        .map_err(|error| format!("failed to read candidate artifact {path}: {error}"))?;
    if artifact_bytes.len() as u64 != bytes {
        return Err(format!(
            "candidate artifact byte count mismatch for {path}: manifest {bytes}, actual {}",
            artifact_bytes.len()
        ));
    }
    let actual_hash = git_hash_object_bytes(&artifact_bytes)?;
    if actual_hash != manifest_hash {
        return Err(format!(
            "candidate artifact hash mismatch for {path}: manifest {manifest_hash}, actual {actual_hash}"
        ));
    }
    let artifact = String::from_utf8(artifact_bytes)
        .map_err(|error| format!("candidate artifact {path} is not UTF-8 text: {error}"))?;
    if contains_public_github_solution_reference(&artifact) {
        return Err(
            "candidate artifact contains public GitHub solution references and must be rejected"
                .to_string(),
        );
    }
    let diagnosis = diagnose_senior_swe_bench_candidate_patch_artifact(&artifact);

    Ok(json!({
        "schema_version": "a2d.senior-swe-bench-candidate-artifact-selection.v1",
        "selected": {
            "cycle": candidate.get("cycle").cloned().unwrap_or(Value::Null),
            "report_cycle": candidate.get("report_cycle").cloned().unwrap_or(Value::Null),
            "workcell_id": required_manifest_string(candidate, "workcell_id")?,
            "enzyme_id": "coder",
            "provider": required_manifest_string(candidate, "provider")?,
            "artifact_type": "code",
            "path": path,
            "git_object_hash": manifest_hash,
            "bytes": bytes,
        },
        "contains_unified_diff_candidate_patch": diagnosis["contains_unified_diff_candidate_patch"].clone(),
        "contains_public_github_solution_reference": diagnosis["contains_public_github_solution_reference"].clone(),
        "failure_kind": diagnosis["failure_kind"].clone(),
        "artifact_preview": diagnosis["artifact_preview"].clone(),
        "recommended_next_gate": diagnosis["recommended_next_gate"].clone(),
        "extract_patch_args": ["senior-swe-bench-extract-patch", path],
        "provider_invocations_started": false,
        "evaluator_invocations_started": false,
        "fitness_claim_allowed_before_evidence": false,
        "note": "selection only: this starts no providers/evaluators and is not fitness evidence",
    }))
}

fn required_manifest_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| format!("cycle output artifact candidate missing {field}"))
}

fn extract_senior_swe_bench_candidate_patch(artifact: &str) -> Result<String, String> {
    if contains_public_github_solution_reference(artifact) {
        return Err(
            "candidate patch artifact appears to contain public GitHub solution references"
                .to_string(),
        );
    }

    let trimmed = artifact.trim();
    let candidate = if looks_like_unified_diff(trimmed) {
        trimmed.to_string()
    } else {
        extract_fenced_unified_diff(trimmed).ok_or_else(|| {
            format!(
                "artifact does not contain a unified diff candidate patch; diagnosis: {}",
                senior_swe_bench_patch_artifact_failure_kind(artifact)
            )
        })?
    };

    Ok(ensure_trailing_newline(candidate))
}

fn diagnose_senior_swe_bench_candidate_patch_artifact(artifact: &str) -> Value {
    let trimmed = artifact.trim();
    let extracted_patch = if contains_public_github_solution_reference(artifact) {
        None
    } else if looks_like_unified_diff(trimmed) {
        Some(trimmed.to_string())
    } else {
        extract_fenced_unified_diff(trimmed)
    };
    let failure_kind = if extracted_patch.is_some() {
        "candidate_patch_extractable"
    } else {
        senior_swe_bench_patch_artifact_failure_kind(artifact)
    };
    let recommended_next_gate = match failure_kind {
        "candidate_patch_extractable" => {
            "run senior-swe-bench-evaluate against a benchmark checkout/evaluator before claiming task fitness"
        }
        "public_solution_reference" => {
            "reject the artifact; public GitHub solution references violate the Senior SWE-Bench agent policy"
        }
        "checkout_context_not_exercised" => {
            "verify the provider had usable local checkout/tool context before adding prompt-only enrichment"
        }
        _ => {
            "treat as output-contract failure; retry or improve the coder contract only after checkout access is known-good"
        }
    };

    let contains_public_solution_reference = contains_public_github_solution_reference(artifact);
    let artifact_preview = if contains_public_solution_reference {
        "<redacted: public GitHub solution reference>".to_string()
    } else {
        preview(artifact, 400)
    };

    json!({
        "schema_version": "a2d.senior-swe-bench-artifact-diagnosis.v1",
        "contains_unified_diff_candidate_patch": extracted_patch.is_some(),
        "contains_public_github_solution_reference": contains_public_solution_reference,
        "failure_kind": failure_kind,
        "artifact_bytes": artifact.len(),
        "artifact_preview": artifact_preview,
        "recommended_next_gate": recommended_next_gate,
        "note": "diagnostic only: this is not fitness evidence and cannot support a Senior SWE-Bench mastery claim",
    })
}

fn senior_swe_bench_patch_artifact_failure_kind(artifact: &str) -> &'static str {
    if contains_public_github_solution_reference(artifact) {
        "public_solution_reference"
    } else if artifact_defers_to_checkout_inspection(artifact) {
        "checkout_context_not_exercised"
    } else {
        "output_contract_not_followed"
    }
}

fn artifact_defers_to_checkout_inspection(artifact: &str) -> bool {
    let normalized = artifact.to_ascii_lowercase();
    [
        "i'll inspect",
        "i will inspect",
        "inspect the local checkout",
        "inspect the provided checkout",
        "exploring the repository",
        "explore the repository",
        "repository structure",
        "let me start by exploring",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

const MAX_PUBLIC_GITHUB_REFERENCE_PERCENT_DECODE_PASSES: usize = 8;

fn contains_public_github_solution_reference(artifact: &str) -> bool {
    let normalized = artifact.to_ascii_lowercase();
    if contains_public_github_solution_reference_normalized(&normalized) {
        return true;
    }

    let mut current = normalized;
    for _ in 0..MAX_PUBLIC_GITHUB_REFERENCE_PERCENT_DECODE_PASSES {
        let percent_decoded = percent_decode_ascii_sequences(&current).to_ascii_lowercase();
        if percent_decoded == current {
            return false;
        }
        if contains_public_github_solution_reference_normalized(&percent_decoded) {
            return true;
        }
        current = percent_decoded;
    }

    let next = percent_decode_ascii_sequences(&current).to_ascii_lowercase();
    next != current
        && (contains_public_github_solution_reference_normalized(&next)
            || current.contains("github")
            || current.contains("refs")
            || next.contains("github")
            || next.contains("refs"))
}

fn contains_public_github_solution_reference_normalized(normalized: &str) -> bool {
    normalized.contains("github.com")
        || normalized.contains("githubusercontent.com")
        || normalized.contains("github[.]com")
        || normalized.contains("github dot com")
        || normalized.contains("github . com")
        || normalized.contains("/pull/")
        || normalized.contains("/commit/")
        || normalized.contains("/issues/")
        || normalized.contains("refs/pull")
        || contains_github_cli_solution_search_command(normalized)
}

fn percent_decode_ascii_sequences(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut decoded = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' && i + 2 < chars.len() {
            if let (Some(high), Some(low)) = (hex_value(chars[i + 1]), hex_value(chars[i + 2])) {
                decoded.push(((high << 4) | low) as char);
                i += 3;
                continue;
            }
        }
        decoded.push(chars[i]);
        i += 1;
    }
    decoded
}

fn hex_value(c: char) -> Option<u8> {
    match c {
        '0'..='9' => Some(c as u8 - b'0'),
        'a'..='f' => Some(c as u8 - b'a' + 10),
        'A'..='F' => Some(c as u8 - b'A' + 10),
        _ => None,
    }
}

fn contains_github_cli_solution_search_command(normalized: &str) -> bool {
    let words: Vec<&str> = normalized
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|word| !word.is_empty())
        .collect();
    words.windows(2).any(|pair| match pair {
        ["gh", subcommand] => matches!(
            *subcommand,
            "api" | "pr" | "issue" | "repo" | "search" | "browse" | "clone"
        ),
        ["hub", subcommand] => matches!(
            *subcommand,
            "api" | "pr" | "issue" | "repo" | "search" | "browse" | "clone"
        ),
        _ => false,
    })
}

fn extract_fenced_unified_diff(input: &str) -> Option<String> {
    let mut in_fence = false;
    let mut buffer = String::new();
    for line in input.lines() {
        if line.trim_start().starts_with("```") {
            if in_fence {
                let candidate = buffer.trim();
                if looks_like_unified_diff(candidate) {
                    return Some(candidate.to_string());
                }
                buffer.clear();
                in_fence = false;
            } else {
                in_fence = true;
                buffer.clear();
            }
            continue;
        }
        if in_fence {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }

    if in_fence {
        let candidate = buffer.trim();
        if looks_like_unified_diff(candidate) {
            return Some(candidate.to_string());
        }
    }

    None
}

fn looks_like_unified_diff(candidate: &str) -> bool {
    let lines = candidate.lines().collect::<Vec<_>>();
    let first_content = lines
        .iter()
        .find(|line| !line.trim().is_empty())
        .copied()
        .unwrap_or_default();
    let starts_like_diff =
        first_content.starts_with("diff --git ") || first_content.starts_with("--- ");
    let has_old = lines.iter().any(|line| line.starts_with("--- "));
    let has_new = lines.iter().any(|line| line.starts_with("+++ "));
    let has_hunk = lines.iter().any(|line| line.starts_with("@@"));
    starts_like_diff && has_old && has_new && has_hunk
}

fn ensure_trailing_newline(mut value: String) -> String {
    if !value.ends_with('\n') {
        value.push('\n');
    }
    value
}

fn find_senior_swe_bench_task_variant<'a>(
    tasks: &'a [SeniorSweBenchTask],
    requested: &str,
) -> Option<(
    &'a SeniorSweBenchTask,
    &'static str,
    &'a SeniorSweBenchVariant,
)> {
    tasks.iter().find_map(|task| {
        if let Some(hard) = &task.hard
            && hard.task_id == requested
        {
            return Some((task, "hard", hard));
        }
        if let Some(guided) = &task.guided
            && guided.task_id == requested
        {
            return Some((task, "guided", guided));
        }
        None
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeniorSweBenchOfficialEvaluatorManifestInspectConfig {
    task_package: Option<PathBuf>,
    task_cycle_input: Option<PathBuf>,
    official_evaluator_manifest: PathBuf,
    command: Vec<String>,
}

fn run_senior_swe_bench_official_evaluator_manifest_inspect(args: &[String]) {
    let config = parse_senior_swe_bench_official_evaluator_manifest_inspect_args(args)
        .unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench official evaluator manifest inspect error: {error}");
            eprintln!("Usage: a2d senior-swe-bench-official-evaluator-manifest-inspect (--task-package <json>|--task-cycle-input <json>) --official-evaluator-manifest <json> -- <evaluator> [args...]");
            std::process::exit(1);
        });
    let inspection = build_senior_swe_bench_official_evaluator_manifest_inspection(&config)
        .unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench official evaluator manifest inspect error: {error}");
            std::process::exit(1);
        });
    println!(
        "{}",
        serde_json::to_string_pretty(&inspection)
            .expect("Senior SWE-Bench official evaluator manifest inspection must serialize")
    );
}

fn parse_senior_swe_bench_official_evaluator_manifest_inspect_args(
    args: &[String],
) -> Result<SeniorSweBenchOfficialEvaluatorManifestInspectConfig, String> {
    let mut task_package = None;
    let mut task_cycle_input = None;
    let mut official_evaluator_manifest = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--" => {
                let command = args[index + 1..].to_vec();
                if command.is_empty() {
                    return Err(
                        "Senior SWE-Bench official evaluator manifest inspect evaluator command is empty"
                            .to_string(),
                    );
                }
                validate_senior_swe_bench_task_input_args(
                    task_package.as_ref(),
                    task_cycle_input.as_ref(),
                )?;
                let config = SeniorSweBenchOfficialEvaluatorManifestInspectConfig {
                    task_package,
                    task_cycle_input,
                    official_evaluator_manifest: official_evaluator_manifest
                        .ok_or_else(|| "missing --official-evaluator-manifest".to_string())?,
                    command,
                };
                if !config.official_evaluator_manifest.is_file() {
                    return Err(format!(
                        "Senior SWE-Bench official evaluator manifest not found: {}",
                        config.official_evaluator_manifest.display()
                    ));
                }
                return Ok(config);
            }
            "--task-package" => {
                index += 1;
                task_package =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--task-package requires a path".to_string()
                    })?));
            }
            "--task-cycle-input" => {
                index += 1;
                task_cycle_input =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--task-cycle-input requires a path".to_string()
                    })?));
            }
            "--official-evaluator-manifest" => {
                index += 1;
                official_evaluator_manifest =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--official-evaluator-manifest requires a path".to_string()
                    })?));
            }
            other => {
                return Err(format!(
                    "unknown senior-swe-bench-official-evaluator-manifest-inspect argument: {other}"
                ));
            }
        }
        index += 1;
    }
    Err("missing -- <evaluator> command".to_string())
}

fn load_senior_swe_bench_official_manifest_inspect_task(
    config: &SeniorSweBenchOfficialEvaluatorManifestInspectConfig,
) -> Result<SeniorSweBenchTaskPackageSummary, String> {
    if let Some(task_package) = &config.task_package {
        let package_json = read_artifact_or_exit(task_package.to_string_lossy().as_ref());
        parse_senior_swe_bench_task_package(&package_json)
            .map_err(|error| format!("task package error: {error}"))
    } else if let Some(task_cycle_input) = &config.task_cycle_input {
        let cycle_input_json = read_artifact_or_exit(task_cycle_input.to_string_lossy().as_ref());
        parse_senior_swe_bench_cycle_input(&cycle_input_json)
            .map_err(|error| format!("task cycle input error: {error}"))
    } else {
        Err("missing --task-package or --task-cycle-input".to_string())
    }
}

fn build_senior_swe_bench_official_evaluator_manifest_inspection(
    config: &SeniorSweBenchOfficialEvaluatorManifestInspectConfig,
) -> Result<Value, String> {
    let package = load_senior_swe_bench_official_manifest_inspect_task(config)?;
    build_senior_swe_bench_official_evaluator_manifest_inspection_value(
        &package,
        &config.official_evaluator_manifest,
        &config.command,
    )
}

fn build_senior_swe_bench_official_evaluator_manifest_inspection_value(
    package: &SeniorSweBenchTaskPackageSummary,
    official_evaluator_manifest: &Path,
    command: &[String],
) -> Result<Value, String> {
    let manifest_json = read_artifact_to_string(official_evaluator_manifest)?;
    let manifest =
        parse_senior_swe_bench_official_evaluator_manifest(&manifest_json, package, command)?;
    let manifest_hash = file_content_hash(official_evaluator_manifest)?;
    Ok(json!({
        "schema_version": "a2d.senior-swe-bench-official-evaluator-manifest-inspection.v1",
        "task_id": manifest.task_id,
        "repo": manifest.repo,
        "official_evaluator_manifest_path": official_evaluator_manifest,
        "official_evaluator_manifest_hash": manifest_hash,
        "official_benchmark_url": manifest.benchmark_url,
        "official_hidden_holdouts": manifest.hidden_holdouts,
        "official_github_solution_search_allowed": manifest.github_solution_search_allowed,
        "official_benchmark_provided_command": manifest.benchmark_provided_command,
        "provider_invocations_started": false,
        "evaluator_invocations_started": false,
        "fitness_evidence_inspection_started": false,
        "github_solution_search_allowed": false,
        "fitness_claim_allowed_before_evidence": false,
        "official_senior_swe_bench_mastery": false,
        "note": "manifest inspection only: validates official evaluator provenance without running evaluators or claiming fitness",
    }))
}

fn run_senior_swe_bench_evaluate(args: &[String]) {
    let mut config = parse_senior_swe_bench_evaluate_args(args).unwrap_or_else(|error| {
        eprintln!("{error}");
        eprintln!("Usage: a2d senior-swe-bench-evaluate (--task-package <json>|--task-cycle-input <json>) (--candidate-patch <diff>|--candidate-patch-artifact <artifact> --extracted-candidate-patch <diff>) --checkout <dir> [--apply-candidate-patch] [--official-evaluator-manifest <json> --official-evaluator-manifest-inspection <json>] [--output <json>] -- <local-evaluator> [args...]");
        std::process::exit(1);
    });
    let package = load_senior_swe_bench_evaluation_task(&config).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench evaluator task input error: {error}");
        std::process::exit(1);
    });
    let official_manifest = load_senior_swe_bench_official_evaluator_manifest(&config, &package)
        .unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench official evaluator manifest error: {error}");
            std::process::exit(1);
        });
    let evaluator_kind = if official_manifest.is_some() {
        "official_senior_swe_bench"
    } else {
        "provided_local_command"
    };
    let official_manifest_hash = config
        .official_evaluator_manifest
        .as_ref()
        .map(|path| file_content_hash(path))
        .transpose()
        .unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench official evaluator manifest hash error: {error}");
            std::process::exit(1);
        });
    let official_manifest_inspection_hash = config
        .official_evaluator_manifest_inspection
        .as_ref()
        .map(|path| file_content_hash(path))
        .transpose()
        .unwrap_or_else(|error| {
            eprintln!(
                "Senior SWE-Bench official evaluator manifest inspection hash error: {error}"
            );
            std::process::exit(1);
        });
    let candidate_patch_artifact_hash = materialize_senior_swe_bench_candidate_patch_artifact(
        &mut config,
    )
    .unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench candidate patch artifact error: {error}");
        std::process::exit(1);
    });
    if !config.candidate_patch.is_file() {
        eprintln!(
            "Senior SWE-Bench candidate patch not found: {}",
            config.candidate_patch.display()
        );
        std::process::exit(1);
    }
    if !config.checkout.is_dir() {
        eprintln!(
            "Senior SWE-Bench checkout directory not found: {}",
            config.checkout.display()
        );
        std::process::exit(1);
    }
    let candidate_patch_hash = file_content_hash(&config.candidate_patch).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench candidate patch hash error: {error}");
        std::process::exit(1);
    });
    let original_checkout_before =
        checkout_content_fingerprint(&config.checkout).unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench original checkout fingerprint error: {error}");
            std::process::exit(1);
        });
    let candidate_patch_preflight_command = validate_senior_swe_bench_candidate_patch_applicable(
        &config.checkout,
        &config.candidate_patch,
    )
    .unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench candidate patch preflight error: {error}");
        std::process::exit(1);
    });

    let prepared_checkout =
        prepare_senior_swe_bench_evaluator_checkout(&config).unwrap_or_else(|error| {
            eprintln!("Senior SWE-Bench evaluator checkout error: {error}");
            std::process::exit(1);
        });

    let mut outcome = run_local_senior_swe_bench_evaluator(&package, &config, &prepared_checkout);
    let original_checkout_mutated = original_checkout_mutated_after_evaluator(
        &config.checkout,
        &original_checkout_before,
        &mut outcome,
    );
    if config.apply_candidate_patch && original_checkout_mutated {
        outcome.status_success = false;
        outcome.stderr = format!(
            "{}\nSenior SWE-Bench local evaluator mutated the original checkout while --apply-candidate-patch requires isolated evaluation",
            outcome.stderr
        );
    }
    let fitness = senior_swe_bench_local_fitness_report(&outcome);
    let evidence_path = if outcome.status_success {
        fitness_evidence_export_dir()
            .map(|export_dir| {
                let path = export_standalone_fitness_evidence(
                    &fitness,
                    &export_dir,
                    &format!("senior-swe-bench-{}", safe_file_stem(&package.task_id)),
                    Some(&candidate_patch_hash),
                    config.candidate_patch_artifact.as_deref(),
                    candidate_patch_artifact_hash.as_deref(),
                    Some(evaluator_kind),
                    Some(prepared_checkout.candidate_patch_applied),
                    Some(prepared_checkout.evaluator_checkout_mode),
                    Some(original_checkout_mutated),
                    Some(&config.candidate_patch),
                    Some(&prepared_checkout.evaluator_checkout),
                    Some(true),
                    Some("passed"),
                    Some(&candidate_patch_preflight_command),
                    config.official_evaluator_manifest.as_deref(),
                    official_manifest_hash.as_deref(),
                    config.official_evaluator_manifest_inspection.as_deref(),
                    official_manifest_inspection_hash.as_deref(),
                    official_manifest.as_ref(),
                )
                .unwrap_or_else(|error| {
                    eprintln!("Senior SWE-Bench fitness evidence export error: {error}");
                    std::process::exit(1);
                });
                validate_fitness_evidence_candidate_patch_binding(
                    &path,
                    &config.candidate_patch,
                    Some(prepared_checkout.candidate_patch_applied),
                    Some(prepared_checkout.evaluator_checkout_mode),
                    Some(original_checkout_mutated),
                    Some(&prepared_checkout.evaluator_checkout),
                    config.candidate_patch_artifact.as_deref(),
                )
                .unwrap_or_else(|error| {
                    eprintln!("Senior SWE-Bench fitness evidence binding error: {error}");
                    std::process::exit(1);
                });
                path
            })
            .map(|path| retry_artifact_path_string(&path))
    } else {
        None
    };
    let source_provenance = collect_source_provenance().unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench local evaluation source provenance error: {error}");
        std::process::exit(1);
    });
    let evaluation = build_senior_swe_bench_local_evaluation(
        &package,
        evaluator_kind,
        if outcome.status_success {
            "passed"
        } else {
            "failed"
        },
        outcome.exit_code,
        config.candidate_patch.to_string_lossy(),
        candidate_patch_hash,
        config.checkout.to_string_lossy(),
        prepared_checkout.evaluator_checkout.to_string_lossy(),
        prepared_checkout.candidate_patch_applied,
        prepared_checkout.evaluator_checkout_mode,
        original_checkout_mutated,
        true,
        "passed",
        candidate_patch_preflight_command,
        source_provenance.source_revision,
        source_provenance.source_tree_dirty,
        source_provenance.source_diff_scope,
        source_provenance.source_diff_hash,
        source_provenance.evidence_command,
        config.command.clone(),
        config
            .official_evaluator_manifest
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        official_manifest_hash,
        config
            .official_evaluator_manifest_inspection
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        official_manifest_inspection_hash,
        official_manifest.as_ref().map(|_| true),
        official_manifest.as_ref(),
        &outcome.stdout,
        &outcome.stderr,
        evidence_path.clone(),
    );
    let json = serde_json::to_vec_pretty(&evaluation)
        .expect("Senior SWE-Bench local evaluation must serialize");
    if let Some(output) = &config.output {
        if let Some(parent) = output.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).unwrap_or_else(|error| {
                eprintln!(
                    "Failed to create Senior SWE-Bench evaluation output dir {}: {error}",
                    parent.display()
                );
                std::process::exit(1);
            });
        }
        fs::write(output, json).unwrap_or_else(|error| {
            eprintln!(
                "Failed to write Senior SWE-Bench evaluation {}: {error}",
                output.display()
            );
            std::process::exit(1);
        });
        println!("Senior SWE-Bench local evaluation: {}", output.display());
    } else {
        println!("{}", String::from_utf8_lossy(&json));
    }
    if let Some(path) = evidence_path {
        println!("Senior SWE-Bench fitness evidence: {path}");
    }
    if !outcome.status_success {
        std::process::exit(2);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeniorSweBenchEvaluateConfig {
    task_package: Option<PathBuf>,
    task_cycle_input: Option<PathBuf>,
    candidate_patch: PathBuf,
    candidate_patch_artifact: Option<PathBuf>,
    extracted_candidate_patch: Option<PathBuf>,
    checkout: PathBuf,
    output: Option<PathBuf>,
    apply_candidate_patch: bool,
    official_evaluator_manifest: Option<PathBuf>,
    official_evaluator_manifest_inspection: Option<PathBuf>,
    command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeniorSweBenchLocalOutcome {
    status_success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn parse_senior_swe_bench_evaluate_args(
    args: &[String],
) -> Result<SeniorSweBenchEvaluateConfig, String> {
    let mut task_package = None;
    let mut task_cycle_input = None;
    let mut candidate_patch = None;
    let mut candidate_patch_artifact = None;
    let mut extracted_candidate_patch = None;
    let mut checkout = None;
    let mut output = None;
    let mut apply_candidate_patch = false;
    let mut official_evaluator_manifest = None;
    let mut official_evaluator_manifest_inspection = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--" => {
                let command = args[index + 1..].to_vec();
                if command.is_empty() {
                    return Err("Senior SWE-Bench evaluator command is empty".to_string());
                }
                validate_senior_swe_bench_task_input_args(
                    task_package.as_ref(),
                    task_cycle_input.as_ref(),
                )?;
                return Ok(SeniorSweBenchEvaluateConfig {
                    task_package,
                    task_cycle_input,
                    candidate_patch: validate_senior_swe_bench_candidate_patch_args(
                        candidate_patch.as_ref(),
                        candidate_patch_artifact.as_ref(),
                        extracted_candidate_patch.as_ref(),
                    )?,
                    candidate_patch_artifact,
                    extracted_candidate_patch,
                    checkout: checkout.ok_or_else(|| "missing --checkout".to_string())?,
                    output,
                    apply_candidate_patch,
                    official_evaluator_manifest,
                    official_evaluator_manifest_inspection,
                    command,
                });
            }
            "--task-package" => {
                index += 1;
                task_package =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--task-package requires a path".to_string()
                    })?));
            }
            "--task-cycle-input" => {
                index += 1;
                task_cycle_input =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--task-cycle-input requires a path".to_string()
                    })?));
            }
            "--candidate-patch" => {
                index += 1;
                candidate_patch =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--candidate-patch requires a path".to_string()
                    })?));
            }
            "--candidate-patch-artifact" => {
                index += 1;
                candidate_patch_artifact =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--candidate-patch-artifact requires a path".to_string()
                    })?));
            }
            "--extracted-candidate-patch" => {
                index += 1;
                extracted_candidate_patch =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--extracted-candidate-patch requires a path".to_string()
                    })?));
            }
            "--checkout" => {
                index += 1;
                checkout = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--checkout requires a path".to_string())?,
                ));
            }
            "--output" => {
                index += 1;
                output = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--output requires a path".to_string())?,
                ));
            }
            "--apply-candidate-patch" => {
                apply_candidate_patch = true;
            }
            "--official-evaluator-manifest" => {
                index += 1;
                official_evaluator_manifest =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--official-evaluator-manifest requires a path".to_string()
                    })?));
            }
            "--official-evaluator-manifest-inspection" => {
                index += 1;
                official_evaluator_manifest_inspection =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--official-evaluator-manifest-inspection requires a path".to_string()
                    })?));
            }
            other => {
                return Err(format!(
                    "unknown senior-swe-bench-evaluate argument: {other}"
                ));
            }
        }
        index += 1;
    }
    Err("missing -- <local-evaluator> command".to_string())
}

fn validate_senior_swe_bench_task_input_args(
    task_package: Option<&PathBuf>,
    task_cycle_input: Option<&PathBuf>,
) -> Result<(), String> {
    match (task_package, task_cycle_input) {
        (Some(_), Some(_)) => {
            Err("use either --task-package or --task-cycle-input, not both".to_string())
        }
        (None, None) => Err("missing --task-package or --task-cycle-input".to_string()),
        _ => Ok(()),
    }
}

fn validate_senior_swe_bench_candidate_patch_args(
    candidate_patch: Option<&PathBuf>,
    candidate_patch_artifact: Option<&PathBuf>,
    extracted_candidate_patch: Option<&PathBuf>,
) -> Result<PathBuf, String> {
    match (candidate_patch, candidate_patch_artifact) {
        (Some(_), Some(_)) => {
            Err("use either --candidate-patch or --candidate-patch-artifact, not both".to_string())
        }
        (None, None) => Err("missing --candidate-patch or --candidate-patch-artifact".to_string()),
        (Some(path), None) => {
            if extracted_candidate_patch.is_some() {
                return Err(
                    "--extracted-candidate-patch requires --candidate-patch-artifact".to_string(),
                );
            }
            Ok(path.clone())
        }
        (None, Some(_)) => extracted_candidate_patch.cloned().ok_or_else(|| {
            "--candidate-patch-artifact requires --extracted-candidate-patch".to_string()
        }),
    }
}

fn materialize_senior_swe_bench_candidate_patch_artifact(
    config: &mut SeniorSweBenchEvaluateConfig,
) -> Result<Option<String>, String> {
    let Some(artifact_path) = &config.candidate_patch_artifact else {
        return Ok(None);
    };
    let artifact_bytes = if artifact_path == Path::new("-") {
        let mut input = Vec::new();
        std::io::stdin().read_to_end(&mut input).map_err(|error| {
            format!("failed to read candidate patch artifact from stdin: {error}")
        })?;
        input
    } else {
        fs::read(artifact_path).map_err(|error| {
            format!(
                "failed to read candidate patch artifact {}: {error}",
                artifact_path.display()
            )
        })?
    };
    let artifact_hash = git_hash_object_bytes(&artifact_bytes)?;
    let artifact = String::from_utf8(artifact_bytes).map_err(|error| {
        format!(
            "candidate patch artifact {} is not UTF-8 text: {error}",
            artifact_path.display()
        )
    })?;
    let patch = extract_senior_swe_bench_candidate_patch(&artifact)?;
    if let Some(parent) = config.candidate_patch.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create extracted candidate patch dir {}: {error}",
                parent.display()
            )
        })?;
    }
    if config.candidate_patch.exists() {
        let existing = fs::read_to_string(&config.candidate_patch).map_err(|error| {
            format!(
                "failed to read existing extracted candidate patch {}: {error}",
                config.candidate_patch.display()
            )
        })?;
        if existing != patch {
            return Err(format!(
                "existing extracted candidate patch {} does not match candidate patch artifact",
                config.candidate_patch.display()
            ));
        }
        return Ok(Some(artifact_hash));
    }
    fs::write(&config.candidate_patch, patch).map_err(|error| {
        format!(
            "failed to write extracted candidate patch {}: {error}",
            config.candidate_patch.display()
        )
    })?;
    Ok(Some(artifact_hash))
}

fn load_senior_swe_bench_evaluation_task(
    config: &SeniorSweBenchEvaluateConfig,
) -> Result<SeniorSweBenchTaskPackageSummary, String> {
    if let Some(task_package) = &config.task_package {
        let package_json = read_artifact_or_exit(task_package.to_string_lossy().as_ref());
        parse_senior_swe_bench_task_package(&package_json)
            .map_err(|error| format!("task package error: {error}"))
    } else if let Some(task_cycle_input) = &config.task_cycle_input {
        let cycle_input_json = read_artifact_or_exit(task_cycle_input.to_string_lossy().as_ref());
        parse_senior_swe_bench_cycle_input(&cycle_input_json)
            .map_err(|error| format!("task cycle input error: {error}"))
    } else {
        Err("missing --task-package or --task-cycle-input".to_string())
    }
}

fn load_senior_swe_bench_official_evaluator_manifest(
    config: &SeniorSweBenchEvaluateConfig,
    package: &SeniorSweBenchTaskPackageSummary,
) -> Result<Option<SeniorSweBenchOfficialEvaluatorManifestSummary>, String> {
    match (
        &config.official_evaluator_manifest,
        &config.official_evaluator_manifest_inspection,
    ) {
        (None, None) => Ok(None),
        (Some(_), None) => Err("Senior SWE-Bench evaluate requires --official-evaluator-manifest-inspection when --official-evaluator-manifest is supplied; run senior-swe-bench-official-evaluator-manifest-inspect first".to_string()),
        (None, Some(_)) => Err("Senior SWE-Bench evaluate --official-evaluator-manifest-inspection requires --official-evaluator-manifest".to_string()),
        (Some(manifest_path), Some(inspection_path)) => {
            let manifest_json = read_artifact_or_exit(manifest_path.to_string_lossy().as_ref());
            let manifest = parse_senior_swe_bench_official_evaluator_manifest(
                &manifest_json,
                package,
                &config.command,
            )?;
            let inspection_text = read_artifact_to_string(inspection_path)?;
            let inspection: Value = serde_json::from_str(&inspection_text).map_err(|error| {
                format!(
                    "invalid Senior SWE-Bench official evaluator manifest inspection JSON: {error}"
                )
            })?;
            validate_retry_execute_official_manifest_inspection(
                &inspection,
                package,
                manifest_path,
                &config.command,
            )?;
            Ok(Some(manifest))
        }
    }
}

#[derive(Debug)]
struct SeniorSweBenchPreparedCheckout {
    evaluator_checkout: PathBuf,
    candidate_patch_applied: bool,
    evaluator_checkout_mode: &'static str,
    _cleanup: Option<SeniorSweBenchCheckoutCleanup>,
}

#[derive(Debug)]
struct SeniorSweBenchCheckoutCleanup {
    path: PathBuf,
}

impl Drop for SeniorSweBenchCheckoutCleanup {
    fn drop(&mut self) {
        if env::var("A2D_SENIOR_SWE_BENCH_KEEP_PATCHED_CHECKOUT")
            .ok()
            .as_deref()
            == Some("1")
        {
            return;
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn prepare_senior_swe_bench_evaluator_checkout(
    config: &SeniorSweBenchEvaluateConfig,
) -> Result<SeniorSweBenchPreparedCheckout, String> {
    if !config.apply_candidate_patch {
        return Ok(SeniorSweBenchPreparedCheckout {
            evaluator_checkout: config.checkout.clone(),
            candidate_patch_applied: false,
            evaluator_checkout_mode: "supplied_checkout",
            _cleanup: None,
        });
    }

    let temp_root = env::var("A2D_SENIOR_SWE_BENCH_PATCHED_CHECKOUT_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::temp_dir().join(format!(
                "a2d-senior-swe-bench-evaluator-{}",
                unique_suffix()
            ))
        });
    validate_patched_checkout_temp_root(&config.checkout, &temp_root)?;
    fs::create_dir_all(&temp_root).map_err(|error| {
        format!(
            "failed to create patched checkout temp root {}: {error}",
            temp_root.display()
        )
    })?;
    let evaluator_checkout = temp_root.join(format!("patched-checkout-{}", unique_suffix()));
    copy_dir_recursively(&config.checkout, &evaluator_checkout)?;
    apply_candidate_patch_to_checkout(&evaluator_checkout, &config.candidate_patch).map_err(
        |error| {
            let _ = fs::remove_dir_all(&evaluator_checkout);
            error
        },
    )?;

    Ok(SeniorSweBenchPreparedCheckout {
        evaluator_checkout: evaluator_checkout.clone(),
        candidate_patch_applied: true,
        evaluator_checkout_mode: "isolated_copy",
        _cleanup: Some(SeniorSweBenchCheckoutCleanup {
            path: evaluator_checkout,
        }),
    })
}

fn validate_patched_checkout_temp_root(checkout: &Path, temp_root: &Path) -> Result<(), String> {
    if path_may_resolve_inside(temp_root, checkout) {
        return Err(format!(
            "refusing to place patched checkout temp root {} inside original checkout {}",
            temp_root.display(),
            checkout.display()
        ));
    }
    Ok(())
}

fn copy_dir_recursively(source: &Path, destination: &Path) -> Result<(), String> {
    if path_may_resolve_inside(destination, source) {
        return Err(format!(
            "refusing to copy checkout {} into its own descendant {}",
            source.display(),
            destination.display()
        ));
    }
    fs::create_dir_all(destination).map_err(|error| {
        format!(
            "failed to create checkout copy {}: {error}",
            destination.display()
        )
    })?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("failed to read checkout {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to read checkout entry: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect {}: {error}", source_path.display()))?;
        if file_type.is_dir() {
            copy_dir_recursively(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "failed to copy {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        } else if file_type.is_symlink() {
            copy_symlink(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_symlink(source: &Path, destination: &Path) -> Result<(), String> {
    let target = fs::read_link(source)
        .map_err(|error| format!("failed to read symlink {}: {error}", source.display()))?;
    std::os::unix::fs::symlink(&target, destination).map_err(|error| {
        format!(
            "failed to copy symlink {} to {}: {error}",
            source.display(),
            destination.display()
        )
    })
}

#[cfg(not(unix))]
fn copy_symlink(source: &Path, destination: &Path) -> Result<(), String> {
    let target = fs::read_link(source)
        .map_err(|error| format!("failed to read symlink {}: {error}", source.display()))?;
    let resolved = source
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(target);
    if resolved.is_dir() {
        copy_dir_recursively(&resolved, destination)
    } else {
        fs::copy(&resolved, destination)
            .map(|_| ())
            .map_err(|error| {
                format!(
                    "failed to copy symlink target {} to {}: {error}",
                    resolved.display(),
                    destination.display()
                )
            })
    }
}

fn path_may_resolve_inside(path: &Path, root: &Path) -> bool {
    let root = match fs::canonicalize(root) {
        Ok(root) => root,
        Err(_) => return false,
    };
    let absolute = normalize_absolute_path(path);
    if absolute == root || absolute.starts_with(&root) {
        return true;
    }
    let mut ancestor = absolute.as_path();
    loop {
        if ancestor.exists() {
            return fs::canonicalize(ancestor)
                .is_ok_and(|canonical| canonical == root || canonical.starts_with(&root));
        }
        match ancestor.parent() {
            Some(parent) => ancestor = parent,
            None => return false,
        }
    }
}

fn normalize_absolute_path(path: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str())
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
        }
    }
    normalized
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckoutContentFingerprint {
    entries: Vec<(String, String)>,
}

fn checkout_content_fingerprint(checkout: &Path) -> Result<CheckoutContentFingerprint, String> {
    let root = fs::canonicalize(checkout).map_err(|error| {
        format!(
            "failed to canonicalize checkout {}: {error}",
            checkout.display()
        )
    })?;
    let mut stack = vec![root.clone()];
    let mut entries = Vec::new();
    while let Some(path) = stack.pop() {
        let mut children = fs::read_dir(&path)
            .map_err(|error| format!("failed to read checkout {}: {error}", path.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("failed to read checkout entry: {error}"))?;
        children.sort_by_key(|entry| entry.path());
        for entry in children {
            let path = entry.path();
            let relative = path
                .strip_prefix(&root)
                .map_err(|error| format!("failed to relativize {}: {error}", path.display()))?
                .to_string_lossy()
                .to_string();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
            if metadata.file_type().is_symlink() {
                let target = fs::read_link(&path).map_err(|error| {
                    format!("failed to read symlink {}: {error}", path.display())
                })?;
                entries.push((relative, format!("symlink:{}", target.to_string_lossy())));
            } else if metadata.is_dir() {
                entries.push((relative, "dir".to_string()));
                stack.push(path);
            } else if metadata.is_file() {
                entries.push((relative, format!("file:{}", file_content_hash(&path)?)));
            } else {
                entries.push((relative, "other".to_string()));
            }
        }
    }
    entries.sort();
    Ok(CheckoutContentFingerprint { entries })
}

fn original_checkout_mutated_after_evaluator(
    checkout: &Path,
    before: &CheckoutContentFingerprint,
    outcome: &mut SeniorSweBenchLocalOutcome,
) -> bool {
    match checkout_content_fingerprint(checkout) {
        Ok(after) => after != *before,
        Err(error) => {
            outcome.stderr = format!(
                "{}\nSenior SWE-Bench original checkout fingerprint after evaluator failed: {error}",
                outcome.stderr
            );
            true
        }
    }
}

fn validate_senior_swe_bench_candidate_patch_applicable(
    checkout: &Path,
    candidate_patch: &Path,
) -> Result<String, String> {
    let patch_path = fs::canonicalize(candidate_patch)
        .map_err(|error| format!("failed to canonicalize candidate patch: {error}"))?;
    let command_text = format!(
        "git apply --check --whitespace=nowarn -- {}",
        patch_path.display()
    );
    let output = Command::new("git")
        .arg("apply")
        .arg("--check")
        .arg("--whitespace=nowarn")
        .arg("--")
        .arg(&patch_path)
        .current_dir(checkout)
        .output()
        .map_err(|error| {
            format!(
                "failed to run {command_text} in {}: {error}",
                checkout.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "{command_text} failed in {}: {}",
            checkout.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(command_text)
}

fn apply_candidate_patch_to_checkout(
    checkout: &Path,
    candidate_patch: &Path,
) -> Result<(), String> {
    let patch_path = fs::canonicalize(candidate_patch)
        .map_err(|error| format!("failed to canonicalize candidate patch: {error}"))?;
    let output = Command::new("git")
        .arg("apply")
        .arg("--whitespace=nowarn")
        .arg("--")
        .arg(&patch_path)
        .current_dir(checkout)
        .output()
        .map_err(|error| format!("failed to run git apply {}: {error}", patch_path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "git apply {} failed in {}: {}",
            patch_path.display(),
            checkout.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn file_content_hash(path: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .arg("hash-object")
        .arg("--")
        .arg(path)
        .output()
        .map_err(|error| format!("failed to run git hash-object {}: {error}", path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "git hash-object {} failed: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    validate_git_object_hash(&hash)
        .map_err(|error| format!("git hash-object {} {error}: {hash}", path.display()))?;
    Ok(hash)
}

fn git_hash_object_bytes(bytes: &[u8]) -> Result<String, String> {
    let mut child = Command::new("git")
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run git hash-object --stdin: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "failed to open git hash-object stdin".to_string())?
        .write_all(bytes)
        .map_err(|error| format!("failed to write bytes to git hash-object: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for git hash-object --stdin: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git hash-object --stdin failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    validate_git_object_hash(&hash)?;
    Ok(hash)
}

fn validate_git_object_hash(hash: &str) -> Result<(), String> {
    if hash.len() != 40 || !hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Err("returned invalid object id".to_string())
    } else {
        Ok(())
    }
}

fn run_local_senior_swe_bench_evaluator(
    package: &SeniorSweBenchTaskPackageSummary,
    config: &SeniorSweBenchEvaluateConfig,
    prepared_checkout: &SeniorSweBenchPreparedCheckout,
) -> SeniorSweBenchLocalOutcome {
    let candidate_patch = fs::canonicalize(&config.candidate_patch)
        .unwrap_or_else(|_| config.candidate_patch.clone());
    let original_checkout =
        fs::canonicalize(&config.checkout).unwrap_or_else(|_| config.checkout.clone());
    let evaluator_checkout = fs::canonicalize(&prepared_checkout.evaluator_checkout)
        .unwrap_or_else(|_| prepared_checkout.evaluator_checkout.clone());
    let stdout_path = unique_temp_path("senior-swe-bench-evaluator", "stdout");
    let stderr_path = unique_temp_path("senior-swe-bench-evaluator", "stderr");
    let stdout_file = fs::File::create(&stdout_path).unwrap_or_else(|error| {
        eprintln!("failed to create evaluator stdout capture: {error}");
        std::process::exit(1);
    });
    let stderr_file = fs::File::create(&stderr_path).unwrap_or_else(|error| {
        eprintln!("failed to create evaluator stderr capture: {error}");
        std::process::exit(1);
    });
    let mut command = Command::new(&config.command[0]);
    command
        .args(&config.command[1..])
        .current_dir(&evaluator_checkout)
        .env("A2D_SENIOR_SWE_BENCH_TASK_ID", &package.task_id)
        .env("A2D_SENIOR_SWE_BENCH_REPO", &package.repo)
        .env(
            "A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED",
            if package.github_solution_search_allowed {
                "true"
            } else {
                "false"
            },
        )
        .env(
            "A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN",
            if package.github_solution_search_allowed {
                "false"
            } else {
                "true"
            },
        )
        .env("A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH", &candidate_patch)
        .env("A2D_SENIOR_SWE_BENCH_ORIGINAL_CHECKOUT", &original_checkout)
        .env(
            "A2D_SENIOR_SWE_BENCH_EVALUATOR_CHECKOUT",
            &evaluator_checkout,
        )
        .env(
            "A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH_APPLIED",
            if prepared_checkout.candidate_patch_applied {
                "true"
            } else {
                "false"
            },
        )
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));
    let mut child = command.spawn().unwrap_or_else(|error| {
        eprintln!("failed to start Senior SWE-Bench local evaluator: {error}");
        std::process::exit(1);
    });
    let timeout = env::var("A2D_SENIOR_SWE_BENCH_EVALUATOR_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(300));
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let status = child.wait().unwrap_or_else(|error| {
                    eprintln!("failed to collect timed-out evaluator status: {error}");
                    std::process::exit(1);
                });
                let stdout = read_and_remove_capture(&stdout_path);
                let stderr = read_and_remove_capture(&stderr_path);
                return SeniorSweBenchLocalOutcome {
                    status_success: false,
                    exit_code: status.code(),
                    stdout,
                    stderr: format!(
                        "{}\nSenior SWE-Bench local evaluator timed out after {}s",
                        stderr,
                        timeout.as_secs()
                    ),
                };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                eprintln!("failed while waiting for Senior SWE-Bench local evaluator: {error}");
                std::process::exit(1);
            }
        }
    }
    let status = child.wait().unwrap_or_else(|error| {
        eprintln!("failed to collect Senior SWE-Bench local evaluator status: {error}");
        std::process::exit(1);
    });
    let stdout = read_and_remove_capture(&stdout_path);
    let stderr = read_and_remove_capture(&stderr_path);
    SeniorSweBenchLocalOutcome {
        status_success: status.success(),
        exit_code: status.code(),
        stdout,
        stderr,
    }
}

fn read_and_remove_capture(path: &Path) -> String {
    let content = fs::read_to_string(path).unwrap_or_default();
    let _ = fs::remove_file(path);
    content
}

fn senior_swe_bench_local_fitness_report(outcome: &SeniorSweBenchLocalOutcome) -> FitnessReport {
    // The no-search case is deliberately named as policy-declared evidence:
    // A²D validated task/manifest policy and propagated env flags, but this is
    // not OS/network forensics proving the evaluator or provider had no egress.
    FitnessReport::compute(vec![
        CaseResult {
            name: "all_tests_pass".to_string(),
            passed: outcome.status_success,
        },
        CaseResult {
            name: "hidden_acceptance".to_string(),
            passed: outcome.status_success,
        },
        CaseResult {
            name: "has_no_solution_search_policy_declared".to_string(),
            passed: true,
        },
    ])
}

fn export_standalone_fitness_evidence(
    report: &FitnessReport,
    export_dir: &Path,
    challenge_name: &str,
    candidate_patch_hash: Option<&str>,
    candidate_patch_artifact_path: Option<&Path>,
    candidate_patch_artifact_hash: Option<&str>,
    evaluator_kind: Option<&str>,
    candidate_patch_applied: Option<bool>,
    evaluator_checkout_mode: Option<&str>,
    original_checkout_mutated: Option<bool>,
    candidate_patch_path: Option<&Path>,
    evaluator_checkout_path: Option<&Path>,
    candidate_patch_preflight_checked: Option<bool>,
    candidate_patch_preflight_status: Option<&str>,
    candidate_patch_preflight_command: Option<&str>,
    official_evaluator_manifest_path: Option<&Path>,
    official_evaluator_manifest_hash: Option<&str>,
    official_evaluator_manifest_inspection_path: Option<&Path>,
    official_evaluator_manifest_inspection_hash: Option<&str>,
    official_manifest: Option<&SeniorSweBenchOfficialEvaluatorManifestSummary>,
) -> Result<PathBuf, String> {
    fs::create_dir_all(export_dir).map_err(|error| {
        format!(
            "failed to create fitness evidence export dir {}: {error}",
            export_dir.display()
        )
    })?;
    let value: Value = serde_json::from_slice(&fitness_evidence_artifact(
        0,
        report,
        standalone_fitness_evidence_delta(report),
    ))
    .map_err(|error| format!("fitness evidence artifact was not JSON: {error}"))?;
    let mut value = add_export_source_provenance(value)?;
    if let Some(candidate_patch_hash) = candidate_patch_hash {
        value
            .as_object_mut()
            .ok_or_else(|| {
                "fitness evidence must be a JSON object before candidate patch provenance"
                    .to_string()
            })?
            .insert(
                "candidate_patch_hash".to_string(),
                Value::String(candidate_patch_hash.to_string()),
            );
    }
    if let Some(candidate_patch_path) = candidate_patch_path {
        value
            .as_object_mut()
            .ok_or_else(|| {
                "fitness evidence must be a JSON object before candidate patch path provenance"
                    .to_string()
            })?
            .insert(
                "candidate_patch_path".to_string(),
                Value::String(candidate_patch_path.to_string_lossy().to_string()),
            );
    }
    if let Some(candidate_patch_artifact_path) = candidate_patch_artifact_path {
        value
            .as_object_mut()
            .ok_or_else(|| {
                "fitness evidence must be a JSON object before candidate patch artifact path provenance"
                    .to_string()
            })?
            .insert(
                "candidate_patch_artifact_path".to_string(),
                Value::String(candidate_patch_artifact_path.to_string_lossy().to_string()),
            );
    }
    if let Some(candidate_patch_artifact_hash) = candidate_patch_artifact_hash {
        value
            .as_object_mut()
            .ok_or_else(|| {
                "fitness evidence must be a JSON object before candidate patch artifact hash provenance"
                    .to_string()
            })?
            .insert(
                "candidate_patch_artifact_hash".to_string(),
                Value::String(candidate_patch_artifact_hash.to_string()),
            );
    }
    if let Some(evaluator_kind) = evaluator_kind {
        value
            .as_object_mut()
            .ok_or_else(|| {
                "fitness evidence must be a JSON object before evaluator provenance".to_string()
            })?
            .insert(
                "evaluator_kind".to_string(),
                Value::String(evaluator_kind.to_string()),
            );
    }
    if let Some(evaluator_checkout_path) = evaluator_checkout_path {
        value
            .as_object_mut()
            .ok_or_else(|| {
                "fitness evidence must be a JSON object before evaluator checkout path provenance"
                    .to_string()
            })?
            .insert(
                "evaluator_checkout".to_string(),
                Value::String(evaluator_checkout_path.to_string_lossy().to_string()),
            );
    }
    if let Some(candidate_patch_applied) = candidate_patch_applied {
        value
            .as_object_mut()
            .ok_or_else(|| {
                "fitness evidence must be a JSON object before candidate patch application provenance"
                    .to_string()
            })?
            .insert(
                "candidate_patch_applied".to_string(),
                Value::Bool(candidate_patch_applied),
            );
    }
    if let Some(evaluator_checkout_mode) = evaluator_checkout_mode {
        value
            .as_object_mut()
            .ok_or_else(|| {
                "fitness evidence must be a JSON object before evaluator checkout provenance"
                    .to_string()
            })?
            .insert(
                "evaluator_checkout_mode".to_string(),
                Value::String(evaluator_checkout_mode.to_string()),
            );
    }
    if let Some(original_checkout_mutated) = original_checkout_mutated {
        value
            .as_object_mut()
            .ok_or_else(|| {
                "fitness evidence must be a JSON object before checkout mutation provenance"
                    .to_string()
            })?
            .insert(
                "original_checkout_mutated".to_string(),
                Value::Bool(original_checkout_mutated),
            );
    }
    if let Some(candidate_patch_preflight_checked) = candidate_patch_preflight_checked {
        value
            .as_object_mut()
            .ok_or_else(|| {
                "fitness evidence must be a JSON object before candidate patch preflight provenance"
                    .to_string()
            })?
            .insert(
                "candidate_patch_preflight_checked".to_string(),
                Value::Bool(candidate_patch_preflight_checked),
            );
    }
    if let Some(candidate_patch_preflight_status) = candidate_patch_preflight_status {
        value
            .as_object_mut()
            .ok_or_else(|| {
                "fitness evidence must be a JSON object before candidate patch preflight status provenance"
                    .to_string()
            })?
            .insert(
                "candidate_patch_preflight_status".to_string(),
                Value::String(candidate_patch_preflight_status.to_string()),
            );
    }
    if let Some(candidate_patch_preflight_command) = candidate_patch_preflight_command {
        value
            .as_object_mut()
            .ok_or_else(|| {
                "fitness evidence must be a JSON object before candidate patch preflight command provenance"
                    .to_string()
            })?
            .insert(
                "candidate_patch_preflight_command".to_string(),
                Value::String(candidate_patch_preflight_command.to_string()),
            );
    }
    if let Some(manifest) = official_manifest {
        let official_evaluator_manifest_path =
            official_evaluator_manifest_path.ok_or_else(|| {
                "fitness evidence official manifest provenance missing manifest path".to_string()
            })?;
        let official_evaluator_manifest_hash =
            official_evaluator_manifest_hash.ok_or_else(|| {
                "fitness evidence official manifest provenance missing manifest hash".to_string()
            })?;
        let official_evaluator_manifest_inspection_path =
            official_evaluator_manifest_inspection_path.ok_or_else(|| {
                "fitness evidence official manifest provenance missing inspection path".to_string()
            })?;
        let official_evaluator_manifest_inspection_hash =
            official_evaluator_manifest_inspection_hash.ok_or_else(|| {
                "fitness evidence official manifest provenance missing inspection hash".to_string()
            })?;
        let object = value.as_object_mut().ok_or_else(|| {
            "fitness evidence must be a JSON object before official evaluator provenance"
                .to_string()
        })?;
        object.insert(
            "official_evaluator_manifest_path".to_string(),
            Value::String(retry_artifact_path_string(official_evaluator_manifest_path)),
        );
        object.insert(
            "official_evaluator_manifest_hash".to_string(),
            Value::String(official_evaluator_manifest_hash.to_string()),
        );
        object.insert(
            "official_evaluator_manifest_inspection_path".to_string(),
            Value::String(retry_artifact_path_string(
                official_evaluator_manifest_inspection_path,
            )),
        );
        object.insert(
            "official_evaluator_manifest_inspection_hash".to_string(),
            Value::String(official_evaluator_manifest_inspection_hash.to_string()),
        );
        object.insert(
            "official_evaluator_manifest_inspection_validated".to_string(),
            Value::Bool(true),
        );
        object.insert(
            "official_benchmark_url".to_string(),
            Value::String(manifest.benchmark_url.clone()),
        );
        object.insert(
            "official_task_id".to_string(),
            Value::String(manifest.task_id.clone()),
        );
        object.insert(
            "official_repo".to_string(),
            Value::String(manifest.repo.clone()),
        );
        object.insert(
            "official_hidden_holdouts".to_string(),
            Value::Bool(manifest.hidden_holdouts),
        );
        object.insert(
            "official_github_solution_search_allowed".to_string(),
            Value::Bool(manifest.github_solution_search_allowed),
        );
        object.insert(
            "official_benchmark_provided_command".to_string(),
            Value::Array(
                manifest
                    .benchmark_provided_command
                    .iter()
                    .map(|part| Value::String(part.clone()))
                    .collect(),
            ),
        );
    }
    validate_exported_fitness_evidence_value(&value)?;
    let path = fitness_evidence_export_path(export_dir, challenge_name, None, 0, 0);
    let json = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("failed to serialize fitness evidence: {error}"))?;
    fs::write(&path, json).map_err(|error| {
        format!(
            "failed to write fitness evidence export {}: {error}",
            path.display()
        )
    })?;
    Ok(path)
}

fn validate_fitness_evidence_candidate_patch_binding(
    evidence_path: &Path,
    candidate_patch_path: &Path,
    expected_candidate_patch_applied: Option<bool>,
    expected_evaluator_checkout_mode: Option<&str>,
    expected_original_checkout_mutated: Option<bool>,
    expected_evaluator_checkout_path: Option<&Path>,
    expected_candidate_patch_artifact_path: Option<&Path>,
) -> Result<(), String> {
    let bytes = fs::read(evidence_path).map_err(|error| {
        format!(
            "failed to read fitness evidence {}: {error}",
            evidence_path.display()
        )
    })?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "fitness evidence {} is not JSON: {error}",
            evidence_path.display()
        )
    })?;
    validate_exported_fitness_evidence_value(&value)?;
    let evidence_hash = value
        .get("candidate_patch_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| "fitness evidence missing candidate_patch_hash".to_string())?;
    let evaluator_kind = value
        .get("evaluator_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "fitness evidence missing evaluator_kind".to_string())?;
    if evaluator_kind == "official_senior_swe_bench" {
        validate_official_evaluator_manifest_provenance(&value)?;
    }
    let evidence_patch_path = value
        .get("candidate_patch_path")
        .and_then(Value::as_str)
        .ok_or_else(|| "fitness evidence missing candidate_patch_path".to_string())?;
    if !paths_equivalent(Path::new(evidence_patch_path), candidate_patch_path) {
        return Err(format!(
            "fitness evidence candidate_patch_path {evidence_patch_path} does not match current candidate patch path {}",
            candidate_patch_path.display()
        ));
    }
    if let Some(expected_artifact_path) = expected_candidate_patch_artifact_path {
        let evidence_artifact_path = value
            .get("candidate_patch_artifact_path")
            .and_then(Value::as_str)
            .ok_or_else(|| "fitness evidence missing candidate_patch_artifact_path".to_string())?;
        if !paths_equivalent(Path::new(evidence_artifact_path), expected_artifact_path) {
            return Err(format!(
                "fitness evidence candidate_patch_artifact_path {evidence_artifact_path} does not match current candidate patch artifact path {}",
                expected_artifact_path.display()
            ));
        }
        let evidence_artifact_hash = value
            .get("candidate_patch_artifact_hash")
            .and_then(Value::as_str)
            .ok_or_else(|| "fitness evidence missing candidate_patch_artifact_hash".to_string())?;
        if expected_artifact_path != Path::new("-") {
            let current_artifact_hash = file_content_hash(expected_artifact_path)?;
            if evidence_artifact_hash != current_artifact_hash {
                return Err(format!(
                    "fitness evidence candidate_patch_artifact_hash {evidence_artifact_hash} does not match current candidate patch artifact hash {current_artifact_hash}"
                ));
            }
        }
    } else if value.get("candidate_patch_artifact_path").is_some()
        || value.get("candidate_patch_artifact_hash").is_some()
    {
        return Err(
            "fitness evidence contains unexpected candidate_patch_artifact provenance".to_string(),
        );
    }
    if let Some(expected) = expected_candidate_patch_applied {
        let actual = value
            .get("candidate_patch_applied")
            .and_then(Value::as_bool)
            .ok_or_else(|| "fitness evidence missing candidate_patch_applied".to_string())?;
        if actual != expected {
            return Err(format!(
                "fitness evidence candidate_patch_applied {actual} does not match expected {expected}"
            ));
        }
    }
    if let Some(expected) = expected_evaluator_checkout_mode {
        let actual = value
            .get("evaluator_checkout_mode")
            .and_then(Value::as_str)
            .ok_or_else(|| "fitness evidence missing evaluator_checkout_mode".to_string())?;
        if actual != expected {
            return Err(format!(
                "fitness evidence evaluator_checkout_mode {actual} does not match expected {expected}"
            ));
        }
    }
    if let Some(expected) = expected_original_checkout_mutated {
        let actual = value
            .get("original_checkout_mutated")
            .and_then(Value::as_bool)
            .ok_or_else(|| "fitness evidence missing original_checkout_mutated".to_string())?;
        if actual != expected {
            return Err(format!(
                "fitness evidence original_checkout_mutated {actual} does not match expected {expected}"
            ));
        }
    }
    if let Some(expected) = expected_evaluator_checkout_path {
        let actual = value
            .get("evaluator_checkout")
            .and_then(Value::as_str)
            .ok_or_else(|| "fitness evidence missing evaluator_checkout".to_string())?;
        if !paths_equivalent(Path::new(actual), expected) {
            return Err(format!(
                "fitness evidence evaluator_checkout {actual} does not match current evaluator checkout {}",
                expected.display()
            ));
        }
    }
    let preflight_checked = value
        .get("candidate_patch_preflight_checked")
        .and_then(Value::as_bool)
        .ok_or_else(|| "fitness evidence missing candidate_patch_preflight_checked".to_string())?;
    if !preflight_checked {
        return Err("fitness evidence candidate_patch_preflight_checked is not true".to_string());
    }
    let preflight_status = value
        .get("candidate_patch_preflight_status")
        .and_then(Value::as_str)
        .ok_or_else(|| "fitness evidence missing candidate_patch_preflight_status".to_string())?;
    if preflight_status != "passed" {
        return Err(format!(
            "fitness evidence candidate_patch_preflight_status {preflight_status} is not passed"
        ));
    }
    let preflight_command = value
        .get("candidate_patch_preflight_command")
        .and_then(Value::as_str)
        .ok_or_else(|| "fitness evidence missing candidate_patch_preflight_command".to_string())?;
    if !preflight_command.contains("git apply --check") {
        return Err(
            "fitness evidence candidate_patch_preflight_command does not record git apply --check"
                .to_string(),
        );
    }
    let current_hash = file_content_hash(candidate_patch_path)?;
    if evidence_hash != current_hash {
        return Err(format!(
            "fitness evidence candidate_patch_hash {evidence_hash} does not match current candidate patch hash {current_hash}"
        ));
    }
    Ok(())
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn standalone_fitness_evidence_delta(report: &FitnessReport) -> f64 {
    if report.failed == 0 && report.total > 0 {
        report.fitness
    } else {
        -1.0
    }
}

fn safe_file_stem(value: &str) -> String {
    let safe = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    if safe.is_empty() {
        "task".to_string()
    } else {
        safe
    }
}

fn run_senior_swe_bench_audit(input_path: &str, mode: Option<&str>, task_id: Option<&str>) {
    if let Some(mode) = mode {
        if !matches!(mode, "task-context" | "task-package" | "task-cycle-input") {
            eprintln!("unknown senior-swe-bench-audit mode: {mode}");
            eprintln!(
                "Usage: a2d senior-swe-bench-audit <html|-> [task-context|task-package|task-cycle-input <task-id>]"
            );
            std::process::exit(1);
        }
        if task_id.is_none() {
            eprintln!("Usage: a2d senior-swe-bench-audit <html|-> {mode} <task-id>");
            std::process::exit(1);
        }
    } else if task_id.is_some() {
        eprintln!(
            "Usage: a2d senior-swe-bench-audit <html|-> [task-context|task-package|task-cycle-input <task-id>]"
        );
        std::process::exit(1);
    }

    let input = read_artifact_or_exit(input_path);
    let tasks = extract_senior_swe_bench_tasks(&input).unwrap_or_else(|error| {
        eprintln!("Senior SWE-Bench audit error: {error}");
        std::process::exit(1);
    });

    if matches!(
        mode,
        Some("task-context" | "task-package" | "task-cycle-input")
    ) {
        let requested = task_id.expect("task id presence checked before reading input");
        let (task, variant_name, variant) = find_senior_swe_bench_task_variant(&tasks, requested)
            .unwrap_or_else(|| {
                eprintln!("Senior SWE-Bench task id not found: {requested}");
                std::process::exit(1);
            });
        if mode == Some("task-package") {
            let package = build_senior_swe_bench_task_package(task, variant_name, variant);
            println!(
                "{}",
                serde_json::to_string_pretty(&package)
                    .expect("senior swe-bench task package must serialize")
            );
        } else if mode == Some("task-cycle-input") {
            let cycle_input = build_senior_swe_bench_cycle_input(task, variant_name, variant);
            println!(
                "{}",
                serde_json::to_string_pretty(&cycle_input)
                    .expect("senior swe-bench cycle input must serialize")
            );
        } else {
            println!("{}", render_senior_swe_bench_task_context(task, variant));
        }
        return;
    }

    let audit = build_senior_swe_bench_audit(&tasks, input_path);
    println!(
        "{}",
        serde_json::to_string_pretty(&audit).expect("senior swe-bench audit must serialize")
    );
}

fn score_artifact_exit_code(report: &a2d_core::benchmark::FitnessReport) -> i32 {
    if report.total > 0 && report.passed == report.total {
        0
    } else {
        2
    }
}

fn format_score_artifact_report(
    challenge_name: &str,
    report: &a2d_core::benchmark::FitnessReport,
) -> String {
    let mut output = String::new();
    output.push_str(&format!("A²D Artifact Score: {challenge_name}\n"));
    output.push_str("═══════════════════\n");
    output.push_str(&format!(
        "Fitness: {:.0}% ({}/{})\n",
        report.fitness * 100.0,
        report.passed,
        report.total
    ));
    output.push_str("Cases:\n");
    for result in &report.results {
        let marker = if result.passed { "✓" } else { "✗" };
        output.push_str(&format!("  {marker} {}\n", result.name));
    }
    if report.diagnostic.is_some() {
        output.push_str(
            "Diagnostic: captured but not printed by score-artifact (hidden acceptance barrier).\n",
        );
    }
    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RoleProviderComparisonArgs {
    replicas: usize,
    providers: Vec<String>,
}

fn parse_role_provider_comparison_args(
    provider_args: &[String],
) -> Result<RoleProviderComparisonArgs, String> {
    let mut replicas = 1;
    let mut providers = Vec::new();
    let mut index = 0;
    while index < provider_args.len() {
        let arg = &provider_args[index];
        if arg == "--replicas" {
            let value = provider_args
                .get(index + 1)
                .ok_or_else(|| "--replicas requires a positive integer".to_string())?;
            replicas = value
                .parse::<usize>()
                .map_err(|_| "--replicas requires a positive integer".to_string())?;
            index += 2;
        } else if let Some(value) = arg.strip_prefix("--replicas=") {
            replicas = value
                .parse::<usize>()
                .map_err(|_| "--replicas requires a positive integer".to_string())?;
            index += 1;
        } else if arg.starts_with("--") {
            return Err(format!("unknown compare-role-providers option: {arg}"));
        } else {
            providers.push(arg.clone());
            index += 1;
        }
    }
    if replicas == 0 {
        return Err("--replicas requires a positive integer".to_string());
    }
    Ok(RoleProviderComparisonArgs {
        replicas,
        providers,
    })
}

fn run_role_provider_comparison(name: &str, enzyme_name: &str, provider_args: &[String]) {
    let options = parse_role_provider_comparison_args(provider_args).unwrap_or_else(|error| {
        eprintln!("{error}");
        eprintln!(
            "Usage: a2d compare-role-providers <challenge> <enzyme> [--replicas N] [providers...]"
        );
        std::process::exit(1);
    });
    let challenge = load_challenge_or_exit(name);
    let enzyme_id = EnzymeId::from(enzyme_name);
    let loaded_germline = load_germline_for_topology(TopologyMode::Evolved);
    let germline = validation_germline_for_enzyme(loaded_germline, &enzyme_id);
    let registry_for_defaults = build_runtime_registry(&germline);
    let current_provider = registry_for_defaults
        .provider_for(&enzyme_id)
        .name()
        .to_string();
    let providers = if options.providers.is_empty() {
        let mut providers = vec![
            current_provider.clone(),
            "opencode/kimi-for-coding/k2p6".to_string(),
            "opencode/opencode/deepseek-v4-flash-free".to_string(),
        ];
        providers.sort();
        providers.dedup();
        providers
    } else {
        options.providers.clone()
    };

    let mut results = Vec::new();
    for replica in 1..=options.replicas {
        for provider_name in &providers {
            let loaded_germline = load_germline_for_topology(TopologyMode::Evolved);
            let germline = validation_germline_for_enzyme(loaded_germline, &enzyme_id);
            let mut registry = build_runtime_registry(&germline);
            register_experimental_provider_if_known(&mut registry, provider_name);
            let valid_enzyme_ids = BTreeSet::from([enzyme_id.clone()]);
            let application = registry.apply_policy(
                &ProviderPolicy {
                    assignments: BTreeMap::from([(enzyme_name.to_string(), provider_name.clone())]),
                },
                &valid_enzyme_ids,
            );
            if !application.rejected.is_empty() {
                results.push(json!({
                    "replica": replica,
                    "provider": provider_name,
                    "assignment_accepted": false,
                    "error": application.rejected[0].reason,
                }));
                continue;
            }
            let assigned_provider = registry.provider_for(&enzyme_id).name().to_string();
            let mut metabolism = apply_runtime_env(
                Metabolism::new(germline, registry)
                    .with_benchmark(challenge.scoring_benchmark())
                    .with_max_invocations_per_cycle(1)
                    .with_project_root(project_root()),
            );
            let failure_report_marker =
                format!("a2d role provider comparison marker for {enzyme_name}");
            seed_escalation_validation_artifacts(
                &mut metabolism,
                challenge.requirements,
                &enzyme_id,
                &failure_report_marker,
            );

            let started = Instant::now();
            let report = metabolism.run_cycle();
            let elapsed_ms = started.elapsed().as_millis();
            results.push(role_provider_comparison_result_json(
                replica,
                provider_name,
                &assigned_provider,
                elapsed_ms,
                &report,
            ));
        }
    }

    let summary = summarize_role_provider_comparison_results(&results);
    let output = json!({
        "challenge": challenge.name,
        "enzyme": enzyme_name,
        "current_provider": current_provider,
        "replicas": options.replicas,
        "providers": providers,
        "summary": summary,
        "persistence": "disabled: no lineage commits and no accepted patches applied",
        "results": results,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).expect("role comparison output must serialize")
    );
}

#[derive(Debug, Default)]
struct RoleProviderComparisonSummary {
    attempts: usize,
    assignment_accepted: usize,
    assignment_rejected: usize,
    successes: usize,
    failures: usize,
    killed: usize,
    timed_out: usize,
    materialized_output_runs: usize,
    accepted_patches: usize,
    rejected_patches: usize,
    noop_patches: usize,
    elapsed_ms: Vec<u64>,
}

fn summarize_role_provider_comparison_results(results: &[Value]) -> Value {
    let mut by_provider: BTreeMap<String, RoleProviderComparisonSummary> = BTreeMap::new();
    for result in results {
        let provider = result
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>")
            .to_string();
        let summary = by_provider.entry(provider).or_default();
        summary.attempts += 1;
        if result
            .get("assignment_accepted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            summary.assignment_accepted += 1;
        } else {
            summary.assignment_rejected += 1;
        }

        if let Some(elapsed) = result.get("elapsed_ms").and_then(Value::as_u64) {
            summary.elapsed_ms.push(elapsed);
        }
        let outcome = result.get("outcome").and_then(Value::as_str).unwrap_or("");
        if outcome.starts_with("success:") {
            summary.successes += 1;
        } else if outcome.starts_with("failed:") {
            summary.failures += 1;
        } else if outcome.starts_with("killed:") {
            summary.killed += 1;
        }
        if outcome.contains("timed out") {
            summary.timed_out += 1;
        }
        if result
            .get("materialized_outputs")
            .and_then(Value::as_array)
            .map(|outputs| !outputs.is_empty())
            .unwrap_or(false)
        {
            summary.materialized_output_runs += 1;
        }
        summary.accepted_patches += result
            .get("accepted_patches")
            .and_then(Value::as_u64)
            .unwrap_or_default() as usize;
        summary.rejected_patches += result
            .get("rejected_patches")
            .and_then(Value::as_u64)
            .unwrap_or_default() as usize;
        summary.noop_patches += result
            .get("noop_patches")
            .and_then(Value::as_u64)
            .unwrap_or_default() as usize;
    }

    Value::Object(
        by_provider
            .into_iter()
            .map(|(provider, summary)| {
                let elapsed = if summary.elapsed_ms.is_empty() {
                    json!(null)
                } else {
                    let min = summary.elapsed_ms.iter().min().copied().unwrap_or_default();
                    let max = summary.elapsed_ms.iter().max().copied().unwrap_or_default();
                    let mean =
                        summary.elapsed_ms.iter().sum::<u64>() / summary.elapsed_ms.len() as u64;
                    json!({
                        "min": min,
                        "max": max,
                        "mean": mean,
                    })
                };
                (
                    provider,
                    json!({
                        "attempts": summary.attempts,
                        "assignment_accepted": summary.assignment_accepted,
                        "assignment_rejected": summary.assignment_rejected,
                        "successes": summary.successes,
                        "failures": summary.failures,
                        "killed": summary.killed,
                        "timed_out": summary.timed_out,
                        "materialized_output_runs": summary.materialized_output_runs,
                        "accepted_patches": summary.accepted_patches,
                        "rejected_patches": summary.rejected_patches,
                        "noop_patches": summary.noop_patches,
                        "elapsed_ms": elapsed,
                    }),
                )
            })
            .collect(),
    )
}

fn role_provider_comparison_result_json(
    replica: usize,
    provider_name: &str,
    assigned_provider: &str,
    elapsed_ms: u128,
    report: &CycleReport,
) -> Value {
    let lineage = report.lineage.first();
    let materialized_output_previews: BTreeMap<String, String> = lineage
        .map(|entry| {
            entry
                .outputs
                .iter()
                .map(|(artifact, bytes)| {
                    (
                        artifact.0.clone(),
                        preview(&String::from_utf8_lossy(bytes), 1200),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let patch_record = lineage.and_then(|entry| entry.patch.as_ref()).map(|patch| {
        json!({
            "accepted": patch.accepted,
            "rejected": patch.rejected.iter().map(|rejection| json!({
                "file_path": rejection.file_path,
                "reason": rejection.reason,
            })).collect::<Vec<_>>(),
            "noops": patch.noops,
        })
    });
    let noop_patches = lineage
        .and_then(|entry| entry.patch.as_ref())
        .map(|patch| patch.noops.len())
        .unwrap_or_default();
    json!({
        "replica": replica,
        "provider": provider_name,
        "assigned_provider": assigned_provider,
        "assignment_accepted": true,
        "elapsed_ms": elapsed_ms,
        "invocations": report.invocations,
        "failed": report.failed,
        "killed": report.killed,
        "accepted_patches": report.accepted_patches,
        "rejected_patches": report.rejected_patches,
        "noop_patches": noop_patches,
        "patch_record": patch_record,
        "wall_clock_capped": report.wall_clock_capped,
        "outcome": lineage.map(|entry| format_workcell_outcome(&entry.outcome)),
        "lineage_provider": lineage.map(|entry| entry.provider.clone()),
        "materialized_outputs": lineage.map(|entry| entry.outputs.keys().map(|artifact| artifact.0.clone()).collect::<Vec<_>>()).unwrap_or_default(),
        "materialized_output_previews": materialized_output_previews,
    })
}

fn format_workcell_outcome(outcome: &a2d_core::workcell::WorkcellOutcome) -> String {
    match outcome {
        a2d_core::workcell::WorkcellOutcome::Success { outputs } => {
            format!("success: {} output(s)", outputs.len())
        }
        a2d_core::workcell::WorkcellOutcome::Failed { error } => format!("failed: {error}"),
        a2d_core::workcell::WorkcellOutcome::Killed { reason } => format!("killed: {reason:?}"),
    }
}

fn run_escalation_validation(name: &str, enzyme_name: &str) {
    let challenge = load_challenge_or_exit(name);
    let enzyme_id = EnzymeId::from(enzyme_name);
    let failure_report_marker =
        format!("a2d escalation validation failure marker for {enzyme_name}");
    let mut results = Vec::new();

    for rung in 4..=6 {
        let loaded_germline = load_germline_for_topology(TopologyMode::Evolved);
        let germline = validation_germline_for_enzyme(loaded_germline, &enzyme_id);
        let registry = build_runtime_registry(&germline);
        let mut metabolism = apply_runtime_env(
            Metabolism::new(germline, registry)
                .with_benchmark(challenge.scoring_benchmark())
                .with_max_invocations_per_cycle(1)
                .with_project_root(project_root()),
        );
        seed_escalation_validation_artifacts(
            &mut metabolism,
            challenge.requirements,
            &enzyme_id,
            &failure_report_marker,
        );
        let provider_policy_before = metabolism.provider_policy();

        let force_error = metabolism
            .force_escalation_rung_for_validation(&enzyme_id, rung)
            .err();
        let report = if force_error.is_none() {
            Some(metabolism.run_cycle())
        } else {
            None
        };
        let provider_policy_after = metabolism.provider_policy();

        results.push(escalation_validation_result_json(
            rung,
            &enzyme_id,
            force_error.as_deref(),
            report.as_ref(),
            &failure_report_marker,
            provider_policy_before != provider_policy_after,
        ));
    }

    let output = json!({
        "challenge": challenge.name,
        "enzyme": enzyme_name,
        "persistence": "disabled: no lineage commits and no accepted patches applied",
        "field_contract": "external validation reports use escalation_rung for rung metadata",
        "results": results,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).expect("validation output must serialize")
    );
}

fn validation_germline_for_enzyme(germline: Germline, enzyme_id: &EnzymeId) -> Germline {
    germline
        .get_enzyme(enzyme_id)
        .cloned()
        .map(|enzyme| Germline::new(vec![enzyme], baseline_food()))
        .unwrap_or(germline)
}

fn seed_escalation_validation_artifacts(
    metabolism: &mut Metabolism,
    requirements: &str,
    enzyme_id: &EnzymeId,
    failure_report_marker: &str,
) {
    seed_initial_runtime_artifacts(metabolism, requirements);
    metabolism.seed_artifact(
        ArtifactType::from("failure_report"),
        failure_report_marker.as_bytes().to_vec(),
    );

    match enzyme_id.0.as_str() {
        "tester" => {
            metabolism.seed_artifact(
                ArtifactType::from("code"),
                b"fn main() { println!(\"diagnostic validation code\"); }".to_vec(),
            );
        }
        "architect" => {
            metabolism.seed_artifact(
                ArtifactType::from("fitness_report"),
                b"diagnostic validation fitness: 0.50 (seeded non-empty)".to_vec(),
            );
        }
        "evolver" => {
            metabolism.seed_artifact(
                ArtifactType::from("fitness_report"),
                b"diagnostic validation fitness: 0.50 (seeded non-empty)".to_vec(),
            );
        }
        _ => {}
    }
}

fn escalation_validation_result_json(
    rung: usize,
    enzyme_id: &EnzymeId,
    force_error: Option<&str>,
    report: Option<&CycleReport>,
    failure_report_marker: &str,
    provider_policy_changed: bool,
) -> Value {
    let Some(report) = report else {
        return json!({
            "requested_rung": rung,
            "enzyme": enzyme_id.0,
            "accepted": false,
            "error": force_error.unwrap_or("validation did not run"),
            "provider_policy_changed": provider_policy_changed,
        });
    };

    let entry = report
        .lineage
        .iter()
        .find(|entry| entry.enzyme_id == *enzyme_id)
        .or_else(|| report.lineage.first());

    let Some(entry) = entry else {
        return json!({
            "requested_rung": rung,
            "enzyme": enzyme_id.0,
            "accepted": false,
            "error": "no invocation ran; selected enzyme may not have ready inputs",
            "invocations": report.invocations,
            "provider_policy_changed": provider_policy_changed,
        });
    };

    let failure_report_visible = entry
        .inputs
        .get(&ArtifactType::from("failure_report"))
        .is_some_and(|bytes| bytes == failure_report_marker.as_bytes());
    let outcome = match &entry.outcome {
        a2d_core::workcell::WorkcellOutcome::Success { .. } => "success".to_string(),
        a2d_core::workcell::WorkcellOutcome::Failed { error } => format!("failed: {error}"),
        a2d_core::workcell::WorkcellOutcome::Killed { reason } => format!("killed: {reason:?}"),
    };

    json!({
        "requested_rung": rung,
        "enzyme": entry.enzyme_id.0,
        "accepted": true,
        "provider": entry.provider,
        "outcome": outcome,
        "escalation_rung": entry.escalation_rung,
        "provider_swap": entry.provider_swap,
        "clean_session": entry.clean_session,
        "failure_report_marker_visible": failure_report_visible,
        "candidate_evaluation_count": entry.candidate_evaluations.len(),
        "candidate_evaluations": entry.candidate_evaluations.iter().map(|candidate| json!({
            "provider": candidate.provider,
            "materialized": candidate.materialized,
            "fitness": candidate.fitness.as_ref().map(|fitness| json!({
                "passed": fitness.passed,
                "total": fitness.total,
                "score": fitness.fitness,
            })),
            "error": candidate.error,
        })).collect::<Vec<_>>(),
        "provider_policy_changed": provider_policy_changed,
        "serialized_field_check": {
            "uses_escalation_rung": true,
            "internal_counter_names_hidden": true,
        },
    })
}

fn run_topology_comparison(name: &str, num_cycles: usize) {
    println!("A²D Topology Comparison: {name} ({num_cycles} cycles each)");
    println!("═══════════════════");
    println!("Persistence disabled: no lineage commits and no accepted patches applied.\n");

    let seed = run_challenge_for_topology(name, num_cycles, TopologyMode::Seed);
    let evolved = run_challenge_for_topology(name, num_cycles, TopologyMode::Evolved);

    println!("\n═══════════════════");
    println!("Topology comparison summary");
    println!(
        "Challenge: {} | requested cycles: {}",
        seed.challenge, seed.requested_cycles
    );
    println!(
        "{:<9} {:>7} {:>12} {:>10} {:>12} {:>12} {:>10} {:>18} {:>15} {:>14}",
        "Topology",
        "Enzymes",
        "Best",
        "Full@",
        "Wall(s)",
        "Invocations",
        "Failures",
        "Caps",
        "Mutations",
        "Patches"
    );
    for summary in [&seed, &evolved] {
        println!(
            "{:<9} {:>7} {:>11.0}% {:>10} {:>12.1} {:>12} {:>10} {:>18} {:>15} {:>6}/{}",
            summary.topology.label(),
            summary.enzymes,
            summary.best_fitness * 100.0,
            summary.full_fitness_display(),
            summary.elapsed_secs,
            summary.total_invocations,
            summary.provider_failures,
            summary.caps_display(),
            summary.total_mutations,
            summary.accepted_patches,
            summary.rejected_patches,
        );
    }

    let fitness_delta = evolved.best_fitness - seed.best_fitness;
    let invocation_delta = evolved.total_invocations as isize - seed.total_invocations as isize;
    let wall_delta = evolved.elapsed_secs - seed.elapsed_secs;
    println!(
        "\nDelta (evolved - seed): fitness {:+.0}pp, invocations {:+}, wall-clock {:+.1}s",
        fitness_delta * 100.0,
        invocation_delta,
        wall_delta
    );
}

#[derive(Debug, Clone)]
struct ProviderPolicyGateEvidence {
    current: TopologyRunSummary,
    proposed: TopologyRunSummary,
    deltas: Vec<String>,
    decision: ProviderPolicyGateDecision,
}

#[derive(Debug, Clone, PartialEq)]
struct ProviderPolicyGateDecision {
    accepted: bool,
    reason: String,
    fitness_delta: f64,
    invocation_delta: isize,
    wall_delta_secs: f64,
}

fn run_provider_policy_comparison_cli(
    name: &str,
    num_cycles: usize,
    proposed_policy_arg: Option<&str>,
) {
    println!("A²D Provider Policy Comparison: {name} ({num_cycles} cycles each)");
    println!("═══════════════════");
    println!("Persistence disabled: no lineage commits and no accepted patches applied.\n");

    let germline = load_or_seed_germline();
    let current_registry = build_runtime_registry(&germline);
    let current_policy =
        provider_policy_for_germline(&current_registry.current_policy(), &germline);
    let proposed_policy = match parse_provider_policy_arg(proposed_policy_arg, &current_policy) {
        Ok(policy) => policy,
        Err(error) => {
            eprintln!("Invalid proposed provider policy: {error}");
            std::process::exit(2);
        }
    };

    let evidence = run_provider_policy_gate(
        germline,
        name,
        num_cycles,
        &current_policy,
        &proposed_policy,
    );
    print_provider_policy_gate_summary(&evidence);
}

fn parse_provider_policy_arg(
    proposed_policy_arg: Option<&str>,
    current_policy: &ProviderPolicy,
) -> Result<ProviderPolicy, String> {
    let Some(arg) = proposed_policy_arg else {
        return Ok(current_policy.clone());
    };
    let json = if let Some(path) = arg.strip_prefix('@') {
        fs::read_to_string(path).map_err(|error| format!("failed to read {path}: {error}"))?
    } else {
        arg.to_string()
    };
    serde_json::from_str(&json).map_err(|error| error.to_string())
}

fn run_provider_policy_gate(
    germline: Germline,
    challenge_name: &str,
    num_cycles: usize,
    current_policy: &ProviderPolicy,
    proposed_policy: &ProviderPolicy,
) -> ProviderPolicyGateEvidence {
    println!("Provider policy gate: current vs proposed");
    for delta in provider_policy_deltas(current_policy, proposed_policy) {
        println!("  policy delta: {delta}");
    }

    let current = run_challenge_for_provider_policy(
        challenge_name,
        num_cycles,
        germline.clone(),
        current_policy,
        TopologyMode::CurrentPolicy,
    );
    let proposed = run_challenge_for_provider_policy(
        challenge_name,
        num_cycles,
        germline,
        proposed_policy,
        TopologyMode::ProposedPolicy,
    );
    let deltas = provider_policy_deltas(current_policy, proposed_policy);
    let decision = decide_provider_policy_gate(&current, &proposed);

    ProviderPolicyGateEvidence {
        current,
        proposed,
        deltas,
        decision,
    }
}

fn run_challenge_for_provider_policy(
    name: &str,
    num_cycles: usize,
    germline: Germline,
    policy: &ProviderPolicy,
    mode: TopologyMode,
) -> TopologyRunSummary {
    let challenge = load_challenge_or_exit(name);
    let challenge_name = challenge.name.to_string();
    let requirements = challenge.requirements;
    let benchmark = challenge.scoring_benchmark();

    let enzyme_count = germline.enzymes().len();
    let mut registry = build_registry();
    let valid_enzyme_ids = germline
        .enzymes()
        .into_iter()
        .map(|enzyme| enzyme.id.clone())
        .collect::<BTreeSet<_>>();
    register_experimental_providers_from_policy(&mut registry, policy);
    let application = registry.apply_policy(policy, &valid_enzyme_ids);
    if !application.accepted.is_empty() || !application.rejected.is_empty() {
        println!(
            "{} policy application: {} accepted, {} rejected assignments",
            mode.label(),
            application.accepted.len(),
            application.rejected.len()
        );
    }

    let mut metabolism = apply_runtime_env(
        Metabolism::new(germline, registry)
            .with_benchmark(benchmark)
            .with_project_root(project_root()),
    );
    seed_initial_runtime_artifacts(&mut metabolism, requirements);

    println!(
        "{} policy: {} enzymes; running {} cycle(s)...",
        mode.label(),
        enzyme_count,
        num_cycles
    );

    let started = Instant::now();
    let mut summary = TopologyRunSummary::new(mode, &challenge_name, num_cycles, enzyme_count);

    for cycle_num in 1..=num_cycles {
        let report = metabolism.run_cycle();
        summary.record_cycle(cycle_num, &report);
        print!(
            "  cycle {cycle_num}: {} invocations, {} failures, {} killed, {} mutations, {} patches",
            report.invocations,
            report.failed,
            report.killed,
            report.accepted_mutations,
            report.accepted_patches,
        );
        if let Some(ref fitness) = report.fitness {
            print!(
                " | fitness {:.0}% ({}/{})",
                fitness.fitness * 100.0,
                fitness.passed,
                fitness.total
            );
        }
        if report.capped {
            print!(" [invocation-capped]");
        }
        if report.wall_clock_capped {
            print!(" [wall-clock-capped]");
        }
        println!();
        print_topology_lineage(&report);
        print_candidate_evaluations(&report);
        export_comparison_fitness_evidence(&metabolism, &report, &challenge_name, mode.label());
    }

    summary.elapsed_secs = started.elapsed().as_secs_f64();
    println!(
        "  => best {:.0}% ({}/{}), full fitness at cycle {}, {:.1}s\n",
        summary.best_fitness * 100.0,
        summary.best_passed,
        summary.best_total,
        summary.full_fitness_display(),
        summary.elapsed_secs,
    );
    summary
}

fn provider_policy_deltas(current: &ProviderPolicy, proposed: &ProviderPolicy) -> Vec<String> {
    let mut enzymes = current.assignments.keys().cloned().collect::<BTreeSet<_>>();
    enzymes.extend(proposed.assignments.keys().cloned());

    let mut deltas = Vec::new();
    for enzyme in enzymes {
        let before = current.assignments.get(&enzyme);
        let after = proposed.assignments.get(&enzyme);
        if before != after {
            deltas.push(format!(
                "{enzyme}: {} -> {}",
                before.map(String::as_str).unwrap_or("∅"),
                after.map(String::as_str).unwrap_or("∅")
            ));
        }
    }

    if deltas.is_empty() {
        deltas.push("no assignment changes".to_string());
    }
    deltas
}

fn decide_provider_policy_gate(
    current: &TopologyRunSummary,
    proposed: &TopologyRunSummary,
) -> ProviderPolicyGateDecision {
    let fitness_delta = proposed.best_fitness - current.best_fitness;
    let invocation_delta = proposed.total_invocations as isize - current.total_invocations as isize;
    let wall_delta_secs = proposed.elapsed_secs - current.elapsed_secs;
    let invocation_slack = std::cmp::max(1, current.total_invocations / 4) as isize;
    let wall_slack = current.elapsed_secs.mul_add(0.25, 5.0);

    let (accepted, reason) = if current.best_total == 0 || proposed.best_total == 0 {
        (false, "missing fitness evidence".to_string())
    } else if fitness_delta < -f64::EPSILON {
        (false, "proposed policy has worse best fitness".to_string())
    } else if current.best_fitness == 0.0 && proposed.best_fitness == 0.0 {
        (false, "zero-fitness comparison is inconclusive".to_string())
    } else if invocation_delta > invocation_slack {
        (
            false,
            format!(
                "proposed policy materially increases invocations by {invocation_delta} (slack {invocation_slack})"
            ),
        )
    } else if wall_delta_secs > wall_slack {
        (
            false,
            format!(
                "proposed policy materially increases wall-clock by {wall_delta_secs:.1}s (slack {wall_slack:.1}s)"
            ),
        )
    } else {
        (
            true,
            "proposed policy is non-regressing within bounded comparison".to_string(),
        )
    };

    ProviderPolicyGateDecision {
        accepted,
        reason,
        fitness_delta,
        invocation_delta,
        wall_delta_secs,
    }
}

fn print_provider_policy_gate_summary(evidence: &ProviderPolicyGateEvidence) {
    println!("Provider policy comparison summary");
    println!("Policy deltas:");
    for delta in &evidence.deltas {
        println!("  - {delta}");
    }
    println!(
        "{:<9} {:>7} {:>12} {:>10} {:>12} {:>12} {:>10}",
        "Policy", "Enzymes", "Best", "Full@", "Wall(s)", "Invocations", "Failures"
    );
    for summary in [&evidence.current, &evidence.proposed] {
        println!(
            "{:<9} {:>7} {:>11.0}% {:>10} {:>12.1} {:>12} {:>10}",
            summary.topology.label(),
            summary.enzymes,
            summary.best_fitness * 100.0,
            summary.full_fitness_display(),
            summary.elapsed_secs,
            summary.total_invocations,
            summary.provider_failures,
        );
    }
    println!(
        "Provider policy gate: {} — {} (fitness {:+.0}pp, invocations {:+}, wall {:+.1}s)",
        if evidence.decision.accepted {
            "ACCEPT"
        } else {
            "REJECT"
        },
        evidence.decision.reason,
        evidence.decision.fitness_delta * 100.0,
        evidence.decision.invocation_delta,
        evidence.decision.wall_delta_secs,
    );
}

fn commit_provider_policy_if_gate_accepts(
    archive: &LineageArchive,
    policy: &ProviderPolicy,
    report: &CycleReport,
    decision: &ProviderPolicyGateDecision,
) -> Result<Option<String>, std::io::Error> {
    if !decision.accepted {
        return Ok(None);
    }
    archive.commit_provider_policy(policy, report).map(Some)
}

fn provider_policy_gate_cycles() -> usize {
    env::var("A2D_PROVIDER_POLICY_GATE_CYCLES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1)
}

fn provider_policy_gate_challenge(default: &str) -> String {
    env::var("A2D_PROVIDER_POLICY_GATE_CHALLENGE").unwrap_or_else(|_| default.to_string())
}

fn build_registry_for_topology(germline: &Germline, topology: TopologyMode) -> ProviderRegistry {
    build_registry_for_topology_with_overrides(
        germline,
        topology,
        runtime_provider_overrides_from_env(),
    )
}

fn build_registry_for_topology_with_overrides(
    germline: &Germline,
    topology: TopologyMode,
    runtime_overrides: BTreeMap<String, Option<String>>,
) -> ProviderRegistry {
    match topology {
        TopologyMode::Seed => {
            build_runtime_registry_with_options(germline, true, runtime_overrides)
        }
        TopologyMode::Evolved | TopologyMode::CurrentPolicy | TopologyMode::ProposedPolicy => {
            build_runtime_registry_with_options(germline, false, runtime_overrides)
        }
    }
}

fn run_challenge_for_topology(
    name: &str,
    num_cycles: usize,
    topology: TopologyMode,
) -> TopologyRunSummary {
    let challenge = load_challenge_or_exit(name);
    let challenge_name = challenge.name.to_string();
    let requirements = challenge.requirements;
    let benchmark = challenge.scoring_benchmark();

    let germline = load_germline_for_topology(topology);
    let enzyme_count = germline.enzymes().len();
    let registry = build_registry_for_topology(&germline, topology);
    let mut metabolism = apply_runtime_env(
        Metabolism::new(germline, registry)
            .with_benchmark(benchmark)
            .with_project_root(project_root()),
    );
    seed_initial_runtime_artifacts(&mut metabolism, requirements);

    println!(
        "{} topology: {} enzymes; running {} cycle(s)...",
        topology.label(),
        enzyme_count,
        num_cycles
    );

    let started = Instant::now();
    let mut summary = TopologyRunSummary::new(topology, &challenge_name, num_cycles, enzyme_count);

    for cycle_num in 1..=num_cycles {
        let report = metabolism.run_cycle();
        summary.record_cycle(cycle_num, &report);

        print!(
            "  cycle {cycle_num}: {} invocations, {} failures, {} killed, {} mutations, {} patches",
            report.invocations,
            report.failed,
            report.killed,
            report.accepted_mutations,
            report.accepted_patches,
        );
        if let Some(ref fitness) = report.fitness {
            print!(
                " | fitness {:.0}% ({}/{})",
                fitness.fitness * 100.0,
                fitness.passed,
                fitness.total
            );
        }
        if report.capped {
            print!(" [invocation-capped]");
        }
        if report.wall_clock_capped {
            print!(" [wall-clock-capped]");
        }
        println!();
        print_topology_lineage(&report);
        print_candidate_evaluations(&report);
        export_comparison_fitness_evidence(&metabolism, &report, &challenge_name, topology.label());
    }

    summary.elapsed_secs = started.elapsed().as_secs_f64();
    println!(
        "  => best {:.0}% ({}/{}), full fitness at cycle {}, {:.1}s\n",
        summary.best_fitness * 100.0,
        summary.best_passed,
        summary.best_total,
        summary.full_fitness_display(),
        summary.elapsed_secs,
    );

    summary
}

fn compact_one_line(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }

    let keep = max_chars.saturating_sub(1);
    let mut truncated = compact.chars().take(keep).collect::<String>();
    truncated.push('…');
    truncated
}

fn format_topology_lineage_entry(entry: &InvocationLineage) -> String {
    let outcome = match &entry.outcome {
        a2d_core::workcell::WorkcellOutcome::Success { .. } => "OK".to_string(),
        a2d_core::workcell::WorkcellOutcome::Failed { error } => {
            format!("FAIL: {}", compact_one_line(error, 240))
        }
        a2d_core::workcell::WorkcellOutcome::Killed { reason } => format!("KILL: {reason:?}"),
    };
    let escalation = if entry.escalation_rung == 0 {
        String::new()
    } else {
        let mut parts = vec![format!("rung {}", entry.escalation_rung)];
        if entry.provider_swap {
            parts.push("swap".to_string());
        }
        if entry.clean_session {
            parts.push("clean".to_string());
        }
        format!(" {{{}}}", parts.join(", "))
    };

    format!(
        "    [{} via {}{}] {outcome}",
        entry.enzyme_id, entry.provider, escalation
    )
}

fn print_topology_lineage(report: &CycleReport) {
    for entry in &report.lineage {
        println!("{}", format_topology_lineage_entry(entry));
    }
}

fn print_candidate_evaluations(report: &CycleReport) {
    for entry in &report.lineage {
        if entry.candidate_evaluations.is_empty() {
            continue;
        }
        println!("    candidate portfolio for {}:", entry.enzyme_id);
        for candidate in &entry.candidate_evaluations {
            let fitness = candidate
                .fitness
                .as_ref()
                .map(|fitness| {
                    format!(
                        "fitness {:.0}% ({}/{})",
                        fitness.fitness * 100.0,
                        fitness.passed,
                        fitness.total
                    )
                })
                .unwrap_or_else(|| {
                    if candidate.materialized {
                        "materialized, not fitness-scored".to_string()
                    } else {
                        "no materialized artifact".to_string()
                    }
                });
            let error = candidate
                .error
                .as_ref()
                .map(|error| format!(" — {error}"))
                .unwrap_or_default();
            println!("      {}: {}{}", candidate.provider, fitness, error);
        }
    }
}

fn show_lineage() {
    let dir = lineage_dir();
    match LineageArchive::init(&dir) {
        Ok(archive) => {
            println!("A²D Lineage");
            println!("═══════════");
            match archive.log(20) {
                Ok(entries) if entries.is_empty() => {
                    println!("No lineage yet. Run `a2d cycle` to start.");
                }
                Ok(entries) => {
                    for entry in &entries {
                        println!("  {entry}");
                    }
                    println!("\n{} commits in lineage", entries.len());
                }
                Err(e) => eprintln!("Failed to read lineage: {e}"),
            }
        }
        Err(e) => eprintln!("Lineage archive not initialized: {e}"),
    }
}

fn apply_runtime_env(metabolism: Metabolism) -> Metabolism {
    let metabolism = match env::var("A2D_MAX_CYCLE_SECS") {
        Ok(value) if value == "0" => metabolism.without_max_cycle_wall_clock(),
        Ok(value) => match value.parse::<u64>() {
            Ok(secs) => metabolism.with_max_cycle_wall_clock(std::time::Duration::from_secs(secs)),
            Err(_) => metabolism,
        },
        Err(_) => metabolism,
    };

    match env::var("A2D_PROVIDER_COOLDOWN_SECS") {
        Ok(value) => match value.parse::<u64>() {
            Ok(secs) => {
                metabolism.with_provider_failure_cooldown(std::time::Duration::from_secs(secs))
            }
            Err(_) => metabolism,
        },
        Err(_) => metabolism,
    }
}

fn project_root() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn lineage_dir() -> PathBuf {
    project_root().join(".a2d").join("lineage")
}

fn fitness_evidence_export_dir() -> Option<PathBuf> {
    nonempty_env_path("A2D_FITNESS_EVIDENCE_EXPORT_DIR")
        .or_else(|| nonempty_env_path("A2D_FITNESS_EVIDENCE_DIR"))
}

fn nonempty_env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn export_comparison_fitness_evidence(
    metabolism: &Metabolism,
    report: &CycleReport,
    challenge_name: &str,
    run_label: &str,
) {
    let Some(export_dir) = fitness_evidence_export_dir() else {
        return;
    };
    match export_cycle_fitness_evidence(
        metabolism,
        report,
        &export_dir,
        challenge_name,
        Some(run_label),
    ) {
        Ok(path) => println!("    fitness evidence: {}", path.display()),
        Err(error) => {
            eprintln!("    fitness evidence export error: {error}");
            std::process::exit(1);
        }
    }
}

fn export_cycle_fitness_evidence(
    metabolism: &Metabolism,
    report: &CycleReport,
    export_dir: &Path,
    challenge_name: &str,
    run_label: Option<&str>,
) -> Result<PathBuf, String> {
    let artifacts = metabolism.artifacts();
    let value = select_exportable_fitness_evidence(
        report,
        artifacts
            .get(&ArtifactType::from("fitness_report"))
            .map(Vec::as_slice),
    )?;

    fs::create_dir_all(export_dir).map_err(|error| {
        format!(
            "failed to create fitness evidence export dir {}: {error}",
            export_dir.display()
        )
    })?;
    let evidence_cycle = value
        .get("cycle")
        .and_then(Value::as_u64)
        .ok_or_else(|| "fitness evidence missing numeric cycle after validation".to_string())?;
    let path = fitness_evidence_export_path(
        export_dir,
        challenge_name,
        run_label,
        evidence_cycle,
        report.cycle,
    );
    let value = add_export_source_provenance(value)?;
    validate_exported_fitness_evidence_value(&value)?;
    let json = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("failed to serialize fitness evidence: {error}"))?;
    fs::write(&path, json).map_err(|error| {
        format!(
            "failed to write fitness evidence export {}: {error}",
            path.display()
        )
    })?;
    Ok(path)
}

fn fitness_evidence_export_path(
    export_dir: &Path,
    challenge_name: &str,
    run_label: Option<&str>,
    evidence_cycle: u64,
    report_cycle: usize,
) -> PathBuf {
    let prefix = run_label
        .filter(|label| !label.is_empty())
        .map(|label| format!("{label}-"))
        .unwrap_or_default();
    if evidence_cycle == report_cycle as u64 {
        export_dir.join(format!(
            "{prefix}{challenge_name}-cycle-{evidence_cycle}-fitness-evidence.json"
        ))
    } else {
        export_dir.join(format!(
            "{prefix}{challenge_name}-cycle-{evidence_cycle}-consumed-by-cycle-{report_cycle}-fitness-evidence.json"
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceProvenance {
    source_revision: String,
    source_tree_dirty: bool,
    source_diff_scope: String,
    source_diff_hash: String,
    evidence_command: String,
}

fn collect_source_provenance() -> Result<SourceProvenance, String> {
    let source_diff_scope = "crates".to_string();
    let source_revision = git_scope_revision(&source_diff_scope)?;
    let source_status = git_status_for_scope(&source_diff_scope)?;
    reject_untracked_source_files(&source_diff_scope, &source_status)?;
    let source_tree_dirty = !source_status.is_empty();
    let source_diff_hash = git_diff_hash_for_scope(&source_diff_scope)?;
    let command = env::args().skip(1).collect::<Vec<_>>().join(" ");
    Ok(SourceProvenance {
        source_revision,
        source_tree_dirty,
        source_diff_scope,
        source_diff_hash,
        evidence_command: if command.is_empty() {
            if cfg!(test) {
                "cargo test".to_string()
            } else {
                "<unknown>".to_string()
            }
        } else {
            command
        },
    })
}

fn add_export_source_provenance(mut value: Value) -> Result<Value, String> {
    let provenance = collect_source_provenance()?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "fitness evidence must be a JSON object before provenance".to_string())?;
    object.insert(
        "source_revision".to_string(),
        Value::String(provenance.source_revision),
    );
    object.insert(
        "source_tree_dirty".to_string(),
        Value::Bool(provenance.source_tree_dirty),
    );
    object.insert(
        "source_diff_scope".to_string(),
        Value::String(provenance.source_diff_scope),
    );
    object.insert(
        "source_diff_hash".to_string(),
        Value::String(provenance.source_diff_hash),
    );
    object.insert(
        "evidence_command".to_string(),
        Value::String(provenance.evidence_command),
    );
    Ok(value)
}

fn git_output_at(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to run git {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {} failed: {stderr}", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_repo_relative_scope_at(root: &Path, scope: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-prefix"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to resolve A²D git prefix: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("failed to resolve A²D git prefix: {stderr}"));
    }
    let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let project_prefix = prefix
        .find("/crates/")
        .map(|index| &prefix[..index + 1])
        .unwrap_or(prefix.as_str());
    Ok(format!("{project_prefix}{scope}"))
}

fn git_scope_revision(scope: &str) -> Result<String, String> {
    git_scope_revision_at(Path::new(env!("CARGO_MANIFEST_DIR")), scope)
}

fn git_scope_revision_at(root: &Path, scope: &str) -> Result<String, String> {
    let repo_relative_scope = git_repo_relative_scope_at(root, scope)?;
    let revision_spec = format!("HEAD:{repo_relative_scope}");
    git_output_at(root, &["rev-parse", "--short", &revision_spec])
}

fn git_status_for_scope(scope: &str) -> Result<String, String> {
    git_status_for_scope_at(Path::new(env!("CARGO_MANIFEST_DIR")), scope)
}

fn git_status_for_scope_at(root: &Path, scope: &str) -> Result<String, String> {
    let repo_relative_scope = git_repo_relative_scope_at(root, scope)?;
    let top_pathspec = format!(":(top){repo_relative_scope}");
    git_output_at(root, &["status", "--short", "--", &top_pathspec])
}

fn reject_untracked_source_files(scope: &str, status: &str) -> Result<(), String> {
    if let Some(line) = status.lines().find(|line| line.starts_with("?? ")) {
        return Err(format!(
            "exported fitness evidence cannot bind untracked source file in {scope}: {line}"
        ));
    }
    Ok(())
}

fn git_diff_hash_for_scope(scope: &str) -> Result<String, String> {
    git_diff_hash_for_scope_at(Path::new(env!("CARGO_MANIFEST_DIR")), scope)
}

fn git_diff_hash_for_scope_at(root: &Path, scope: &str) -> Result<String, String> {
    let repo_relative_scope = git_repo_relative_scope_at(root, scope)?;
    let top_pathspec = format!(":(top){repo_relative_scope}");
    let diff = Command::new("git")
        .args(["diff", "--binary", "HEAD", "--", &top_pathspec])
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to run git diff for {scope}: {error}"))?;
    if !diff.status.success() {
        let stderr = String::from_utf8_lossy(&diff.stderr);
        return Err(format!("git diff for {scope} failed: {stderr}"));
    }

    let mut child = Command::new("git")
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run git hash-object: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "failed to open git hash-object stdin".to_string())?
        .write_all(&diff.stdout)
        .map_err(|error| format!("failed to write diff to git hash-object: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for git hash-object: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git hash-object failed: {stderr}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn select_exportable_fitness_evidence(
    report: &CycleReport,
    current_fitness_artifact: Option<&[u8]>,
) -> Result<Value, String> {
    if report.fitness.is_some() {
        let bytes = current_fitness_artifact.ok_or_else(|| {
            "cycle reported fitness but no fitness_report artifact exists".to_string()
        })?;
        return validate_exportable_fitness_evidence(bytes, report.cycle);
    }

    for entry in &report.lineage {
        if let Some(bytes) = entry.inputs.get(&ArtifactType::from("fitness_report")) {
            if let Ok(value) = validate_fresh_exportable_fitness_evidence(bytes, report.cycle) {
                return Ok(value);
            }
        }
    }

    Err(
        "export requested but no current or fresh previous actual-test fitness evidence is available"
            .to_string(),
    )
}

fn validate_exportable_fitness_evidence(
    bytes: &[u8],
    report_cycle: usize,
) -> Result<Value, String> {
    let value = validate_exportable_fitness_evidence_shape(bytes)?;
    require_evidence_cycle(&value, report_cycle)?;
    Ok(value)
}

fn validate_fresh_exportable_fitness_evidence(
    bytes: &[u8],
    report_cycle: usize,
) -> Result<Value, String> {
    let value = validate_exportable_fitness_evidence_shape(bytes)?;
    let cycle = evidence_cycle(&value)?;
    if cycle.saturating_add(1) != report_cycle as u64 {
        return Err(format!(
            "fitness evidence cycle {cycle} is not fresh for report cycle {report_cycle}"
        ));
    }
    Ok(value)
}

fn validate_exportable_fitness_evidence_shape(bytes: &[u8]) -> Result<Value, String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("fitness evidence is not JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "fitness evidence must be a JSON object".to_string())?;
    let required_fields = BTreeSet::from([
        "actual_tests_evaluated",
        "cycle",
        "delta_from_last_non_regressing_fitness",
        "diagnostic_present",
        "failed",
        "failed_cases",
        "fitness",
        "non_regressing",
        "passed",
        "results",
        "schema_version",
        "total",
    ]);
    let optional_provenance_fields = BTreeSet::from([
        "evidence_command",
        "source_diff_hash",
        "source_diff_scope",
        "source_revision",
        "source_tree_dirty",
        "candidate_patch_hash",
        "candidate_patch_path",
        "candidate_patch_artifact_path",
        "candidate_patch_artifact_hash",
        "evaluator_kind",
        "evaluator_checkout",
        "candidate_patch_applied",
        "evaluator_checkout_mode",
        "original_checkout_mutated",
        "candidate_patch_preflight_checked",
        "candidate_patch_preflight_status",
        "candidate_patch_preflight_command",
        "official_evaluator_manifest_path",
        "official_evaluator_manifest_hash",
        "official_evaluator_manifest_inspection_path",
        "official_evaluator_manifest_inspection_hash",
        "official_evaluator_manifest_inspection_validated",
        "official_benchmark_url",
        "official_task_id",
        "official_repo",
        "official_hidden_holdouts",
        "official_github_solution_search_allowed",
        "official_benchmark_provided_command",
    ]);
    for field in object.keys() {
        if !required_fields.contains(field.as_str())
            && !optional_provenance_fields.contains(field.as_str())
        {
            return Err(format!(
                "fitness evidence contains unreviewed field: {field}"
            ));
        }
    }
    for field in required_fields {
        if !object.contains_key(field) {
            return Err(format!("fitness evidence missing required field: {field}"));
        }
    }

    require_json_bool(&value, "actual_tests_evaluated", true)?;
    require_json_bool_field(&value, "diagnostic_present")?;
    require_json_bool(&value, "non_regressing", true)?;
    require_json_string(&value, "schema_version", "a2d.fitness-evidence.v1")?;
    require_json_nonnegative_u64(&value, "passed")?;
    require_json_nonnegative_u64(&value, "failed")?;
    require_json_nonnegative_u64(&value, "total")?;
    let fitness = value
        .get("fitness")
        .and_then(Value::as_f64)
        .ok_or_else(|| "fitness evidence missing numeric fitness".to_string())?;
    if !(0.0..=1.0).contains(&fitness) {
        return Err(format!("fitness evidence fitness out of range: {fitness}"));
    }

    evidence_cycle(&value)?;

    let delta = value
        .get("delta_from_last_non_regressing_fitness")
        .and_then(Value::as_f64)
        .ok_or_else(|| "fitness evidence missing numeric delta".to_string())?;
    if delta < 0.0 {
        return Err(format!("fitness evidence regressed by {delta}"));
    }

    let results = value
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| "fitness evidence missing results array".to_string())?;
    let mut has_holdout_status = false;
    for result in results {
        let result_object = result
            .as_object()
            .ok_or_else(|| "fitness evidence result is not an object".to_string())?;
        let result_fields = BTreeSet::from(["name", "passed"]);
        for field in result_object.keys() {
            if !result_fields.contains(field.as_str()) {
                return Err(format!(
                    "fitness evidence result contains unreviewed field: {field}"
                ));
            }
        }
        for field in result_fields {
            if !result_object.contains_key(field) {
                return Err(format!("fitness evidence result missing field: {field}"));
            }
        }
        let name = result
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "fitness evidence result missing name".to_string())?;
        result
            .get("passed")
            .and_then(Value::as_bool)
            .ok_or_else(|| format!("fitness evidence result {name} missing passed bool"))?;
        if name == "hidden_acceptance" || name == "all_tests_pass" {
            has_holdout_status = true;
        } else if !is_public_fitness_case_name_for_cli(name) {
            return Err(format!(
                "fitness evidence leaks non-public case name in results: {name}"
            ));
        }
    }
    if !has_holdout_status {
        return Err(
            "fitness evidence missing hidden-holdout status (all_tests_pass or hidden_acceptance)"
                .to_string(),
        );
    }

    let failed_cases = value
        .get("failed_cases")
        .and_then(Value::as_array)
        .ok_or_else(|| "fitness evidence missing failed_cases array".to_string())?;
    for failed_case in failed_cases {
        let name = failed_case
            .as_str()
            .ok_or_else(|| "fitness evidence failed_cases entry is not a string".to_string())?;
        if name != "hidden_acceptance" && !is_public_fitness_case_name_for_cli(name) {
            return Err(format!(
                "fitness evidence leaks non-public case name in failed_cases: {name}"
            ));
        }
    }

    validate_optional_evaluator_kind(&value)?;
    validate_official_evaluator_manifest_provenance(&value)?;

    Ok(value)
}

fn validate_exported_fitness_evidence_value(value: &Value) -> Result<(), String> {
    validate_exportable_fitness_evidence_shape(&serde_json::to_vec(value).map_err(|error| {
        format!("fitness evidence value could not be serialized for validation: {error}")
    })?)?;

    let source_revision = value
        .get("source_revision")
        .and_then(Value::as_str)
        .ok_or_else(|| "exported fitness evidence missing source_revision".to_string())?;
    let current_revision = git_scope_revision("crates")?;
    if source_revision != current_revision {
        return Err(format!(
            "exported fitness evidence source_revision {source_revision} does not match current revision {current_revision}"
        ));
    }

    let source_tree_dirty = value
        .get("source_tree_dirty")
        .and_then(Value::as_bool)
        .ok_or_else(|| "exported fitness evidence missing source_tree_dirty".to_string())?;
    let source_diff_scope = value
        .get("source_diff_scope")
        .and_then(Value::as_str)
        .ok_or_else(|| "exported fitness evidence missing source_diff_scope".to_string())?;
    if source_diff_scope != "crates" {
        return Err(format!(
            "exported fitness evidence source_diff_scope must be crates, got {source_diff_scope}"
        ));
    }

    let source_diff_hash = value
        .get("source_diff_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| "exported fitness evidence missing source_diff_hash".to_string())?;
    if source_diff_hash.len() != 40 || !source_diff_hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(
            "exported fitness evidence source_diff_hash is not a git object id".to_string(),
        );
    }
    let current_diff_hash = git_diff_hash_for_scope(source_diff_scope)?;
    if source_diff_hash != current_diff_hash {
        return Err(format!(
            "exported fitness evidence source_diff_hash {source_diff_hash} does not match current {source_diff_scope} diff hash {current_diff_hash}"
        ));
    }

    let current_status = git_status_for_scope(source_diff_scope)?;
    reject_untracked_source_files(source_diff_scope, &current_status)?;
    let current_dirty = !current_status.is_empty();
    if source_tree_dirty != current_dirty {
        return Err(format!(
            "exported fitness evidence source_tree_dirty {source_tree_dirty} does not match current dirty status {current_dirty}"
        ));
    }

    let evidence_command = value
        .get("evidence_command")
        .and_then(Value::as_str)
        .ok_or_else(|| "exported fitness evidence missing evidence_command".to_string())?;
    if evidence_command.trim().is_empty() || evidence_command == "<unknown>" {
        return Err("exported fitness evidence evidence_command is empty".to_string());
    }
    if let Some(candidate_patch_hash_value) = value.get("candidate_patch_hash") {
        let candidate_patch_hash = candidate_patch_hash_value.as_str().ok_or_else(|| {
            "exported fitness evidence candidate_patch_hash is not a string".to_string()
        })?;
        if candidate_patch_hash.len() != 40
            || !candidate_patch_hash
                .chars()
                .all(|ch| ch.is_ascii_hexdigit())
        {
            return Err(
                "exported fitness evidence candidate_patch_hash is not a git object id".to_string(),
            );
        }
    }
    if let Some(candidate_patch_path_value) = value.get("candidate_patch_path") {
        let candidate_patch_path = candidate_patch_path_value.as_str().ok_or_else(|| {
            "exported fitness evidence candidate_patch_path is not a string".to_string()
        })?;
        if candidate_patch_path.trim().is_empty() {
            return Err("exported fitness evidence candidate_patch_path is empty".to_string());
        }
    }
    validate_optional_candidate_patch_artifact_provenance(value)?;
    validate_optional_evaluator_kind(value)?;
    validate_optional_patch_application_provenance(value)?;
    validate_optional_candidate_patch_preflight_provenance(value)?;
    validate_official_evaluator_manifest_provenance(value)?;

    Ok(())
}

fn validate_optional_candidate_patch_artifact_provenance(value: &Value) -> Result<(), String> {
    match (
        value.get("candidate_patch_artifact_path"),
        value.get("candidate_patch_artifact_hash"),
    ) {
        (None, None) => Ok(()),
        (Some(path_value), Some(hash_value)) => {
            let path = path_value.as_str().ok_or_else(|| {
                "exported fitness evidence candidate_patch_artifact_path is not a string"
                    .to_string()
            })?;
            if path.trim().is_empty() {
                return Err(
                    "exported fitness evidence candidate_patch_artifact_path is empty".to_string(),
                );
            }
            let hash = hash_value.as_str().ok_or_else(|| {
                "exported fitness evidence candidate_patch_artifact_hash is not a string"
                    .to_string()
            })?;
            if hash.len() != 40 || !hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
                return Err(
                    "exported fitness evidence candidate_patch_artifact_hash is not a git object id"
                        .to_string(),
                );
            }
            Ok(())
        }
        _ => Err(
            "exported fitness evidence candidate_patch_artifact provenance is incomplete"
                .to_string(),
        ),
    }
}

fn validate_optional_patch_application_provenance(value: &Value) -> Result<(), String> {
    if let Some(applied_value) = value.get("candidate_patch_applied") {
        applied_value.as_bool().ok_or_else(|| {
            "exported fitness evidence candidate_patch_applied is not a bool".to_string()
        })?;
    }
    if let Some(mutated_value) = value.get("original_checkout_mutated") {
        mutated_value.as_bool().ok_or_else(|| {
            "exported fitness evidence original_checkout_mutated is not a bool".to_string()
        })?;
    }
    if let Some(mode_value) = value.get("evaluator_checkout_mode") {
        let mode = mode_value.as_str().ok_or_else(|| {
            "exported fitness evidence evaluator_checkout_mode is not a string".to_string()
        })?;
        match mode {
            "supplied_checkout" | "isolated_copy" => {}
            _ => {
                return Err(format!(
                    "exported fitness evidence evaluator_checkout_mode is unreviewed: {mode}"
                ));
            }
        }
    }
    Ok(())
}

fn validate_optional_candidate_patch_preflight_provenance(value: &Value) -> Result<(), String> {
    if let Some(checked_value) = value.get("candidate_patch_preflight_checked") {
        checked_value.as_bool().ok_or_else(|| {
            "exported fitness evidence candidate_patch_preflight_checked is not a bool".to_string()
        })?;
    }
    if let Some(status_value) = value.get("candidate_patch_preflight_status") {
        let status = status_value.as_str().ok_or_else(|| {
            "exported fitness evidence candidate_patch_preflight_status is not a string".to_string()
        })?;
        if status != "passed" {
            return Err(format!(
                "exported fitness evidence candidate_patch_preflight_status is unreviewed: {status}"
            ));
        }
    }
    if let Some(command_value) = value.get("candidate_patch_preflight_command") {
        let command = command_value.as_str().ok_or_else(|| {
            "exported fitness evidence candidate_patch_preflight_command is not a string"
                .to_string()
        })?;
        if !command.contains("git apply --check") {
            return Err(
                "exported fitness evidence candidate_patch_preflight_command must record git apply --check"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn validate_official_evaluator_manifest_provenance(value: &Value) -> Result<(), String> {
    let official_fields = [
        "official_evaluator_manifest_path",
        "official_evaluator_manifest_hash",
        "official_evaluator_manifest_inspection_path",
        "official_evaluator_manifest_inspection_hash",
        "official_evaluator_manifest_inspection_validated",
        "official_benchmark_url",
        "official_task_id",
        "official_repo",
        "official_hidden_holdouts",
        "official_github_solution_search_allowed",
        "official_benchmark_provided_command",
    ];
    let evaluator_kind = value.get("evaluator_kind").and_then(Value::as_str);
    let has_official_fields = official_fields
        .iter()
        .any(|field| value.get(field).is_some());
    if evaluator_kind != Some("official_senior_swe_bench") {
        if has_official_fields {
            return Err(
                "official Senior SWE-Bench provenance present for non-official evaluator evidence"
                    .to_string(),
            );
        }
        return Ok(());
    }
    for field in official_fields {
        if value.get(field).is_none() {
            return Err(format!(
                "official Senior SWE-Bench evidence missing {field}"
            ));
        }
    }
    for field in [
        "official_evaluator_manifest_path",
        "official_evaluator_manifest_inspection_path",
        "official_benchmark_url",
        "official_task_id",
        "official_repo",
    ] {
        let text = value
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("official Senior SWE-Bench evidence {field} is not a string"))?;
        if text.trim().is_empty() {
            return Err(format!(
                "official Senior SWE-Bench evidence {field} is empty"
            ));
        }
    }
    let manifest_hash = value
        .get("official_evaluator_manifest_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "official Senior SWE-Bench evidence official_evaluator_manifest_hash is not a string"
                .to_string()
        })?;
    if manifest_hash.len() != 40 || !manifest_hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(
            "official Senior SWE-Bench evidence official_evaluator_manifest_hash is not a git object id"
                .to_string(),
        );
    }
    let inspection_hash = value
        .get("official_evaluator_manifest_inspection_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "official Senior SWE-Bench evidence official_evaluator_manifest_inspection_hash is not a string"
                .to_string()
        })?;
    if inspection_hash.len() != 40 || !inspection_hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(
            "official Senior SWE-Bench evidence official_evaluator_manifest_inspection_hash is not a git object id"
                .to_string(),
        );
    }
    if value
        .get("official_evaluator_manifest_inspection_validated")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(
            "official Senior SWE-Bench evidence official_evaluator_manifest_inspection_validated is not true"
                .to_string(),
        );
    }
    if value
        .get("official_hidden_holdouts")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(
            "official Senior SWE-Bench evidence official_hidden_holdouts is not true".to_string(),
        );
    }
    if value
        .get("official_github_solution_search_allowed")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err("official Senior SWE-Bench evidence allows GitHub solution search".to_string());
    }
    let command = value
        .get("official_benchmark_provided_command")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "official Senior SWE-Bench evidence official_benchmark_provided_command is not an array"
                .to_string()
        })?;
    if command.is_empty() {
        return Err(
            "official Senior SWE-Bench evidence official_benchmark_provided_command is empty"
                .to_string(),
        );
    }
    let command = command
        .iter()
        .map(|part| {
            let part = part.as_str().ok_or_else(|| {
                "official Senior SWE-Bench evidence official_benchmark_provided_command contains non-string entry"
                    .to_string()
            })?;
            if part.trim().is_empty() {
                return Err(
                    "official Senior SWE-Bench evidence official_benchmark_provided_command contains empty entry"
                        .to_string(),
                );
            }
            Ok(part.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;

    validate_official_evaluator_manifest_files_from_evidence(
        value,
        manifest_hash,
        inspection_hash,
        &command,
    )?;
    Ok(())
}

fn resolve_evidence_referenced_path(path_text: &str) -> Result<PathBuf, String> {
    let path = Path::new(path_text);
    let project_root = normalize_retry_path(a2d_project_root());
    let candidate = if path.is_absolute() {
        normalize_retry_path(path.to_path_buf())
    } else {
        let project_candidate = normalize_retry_path(project_root.join(path));
        if !project_candidate.starts_with(&project_root) {
            return Err(format!(
                "official Senior SWE-Bench evidence relative path {path_text} escapes the A²D project root"
            ));
        }
        if !project_candidate.exists() {
            return Err(format!(
                "official Senior SWE-Bench evidence relative path {path_text} does not resolve under the A²D project root"
            ));
        }
        project_candidate
    };
    let canonical_root = fs::canonicalize(&project_root).map_err(|error| {
        format!(
            "failed to canonicalize A²D project root {}: {error}",
            project_root.display()
        )
    })?;
    let canonical_candidate = fs::canonicalize(&candidate).map_err(|error| {
        format!(
            "failed to canonicalize official Senior SWE-Bench evidence path {}: {error}",
            candidate.display()
        )
    })?;
    if !canonical_candidate.starts_with(&canonical_root) {
        if path.is_absolute() {
            return Err(format!(
                "official Senior SWE-Bench evidence path {path_text} resolves outside the A²D project root"
            ));
        }
        return Err(format!(
            "official Senior SWE-Bench evidence relative path {path_text} resolves outside the A²D project root"
        ));
    }
    Ok(candidate)
}

fn validate_official_evaluator_manifest_files_from_evidence(
    value: &Value,
    expected_manifest_hash: &str,
    expected_inspection_hash: &str,
    command: &[String],
) -> Result<(), String> {
    let manifest_path_text = value
        .get("official_evaluator_manifest_path")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "official Senior SWE-Bench evidence official_evaluator_manifest_path is not a string"
                .to_string()
        })?;
    let inspection_path_text = value
        .get("official_evaluator_manifest_inspection_path")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "official Senior SWE-Bench evidence official_evaluator_manifest_inspection_path is not a string"
                .to_string()
        })?;
    let manifest_path = resolve_evidence_referenced_path(manifest_path_text)?;
    let inspection_path = resolve_evidence_referenced_path(inspection_path_text)?;
    let actual_manifest_hash = file_content_hash(&manifest_path).map_err(|error| {
        format!(
            "official Senior SWE-Bench evidence manifest file hash error for {}: {error}",
            manifest_path.display()
        )
    })?;
    if actual_manifest_hash != expected_manifest_hash {
        return Err(format!(
            "official Senior SWE-Bench evidence manifest hash {expected_manifest_hash} does not match current manifest hash {actual_manifest_hash}"
        ));
    }
    let actual_inspection_hash = file_content_hash(&inspection_path).map_err(|error| {
        format!(
            "official Senior SWE-Bench evidence inspection file hash error for {}: {error}",
            inspection_path.display()
        )
    })?;
    if actual_inspection_hash != expected_inspection_hash {
        return Err(format!(
            "official Senior SWE-Bench evidence inspection hash {expected_inspection_hash} does not match current inspection hash {actual_inspection_hash}"
        ));
    }

    let package = SeniorSweBenchTaskPackageSummary {
        task_id: value
            .get("official_task_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "official Senior SWE-Bench evidence official_task_id is not a string".to_string()
            })?
            .to_string(),
        repo: value
            .get("official_repo")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "official Senior SWE-Bench evidence official_repo is not a string".to_string()
            })?
            .to_string(),
        github_solution_search_allowed: false,
    };
    let manifest_text = read_artifact_to_string(&manifest_path)?;
    let manifest =
        parse_senior_swe_bench_official_evaluator_manifest(&manifest_text, &package, command)?;
    if value.get("official_benchmark_url").and_then(Value::as_str)
        != Some(manifest.benchmark_url.as_str())
    {
        return Err(
            "official Senior SWE-Bench evidence benchmark URL does not match manifest".to_string(),
        );
    }
    let inspection_text = read_artifact_to_string(&inspection_path)?;
    let inspection: Value = serde_json::from_str(&inspection_text).map_err(|error| {
        format!("invalid Senior SWE-Bench official evaluator manifest inspection JSON: {error}")
    })?;
    validate_retry_execute_official_manifest_inspection(
        &inspection,
        &package,
        &manifest_path,
        command,
    )?;
    Ok(())
}

fn validate_optional_evaluator_kind(value: &Value) -> Result<(), String> {
    if let Some(evaluator_kind_value) = value.get("evaluator_kind") {
        let evaluator_kind = evaluator_kind_value
            .as_str()
            .ok_or_else(|| "fitness evidence evaluator_kind is not a string".to_string())?;
        if !matches!(
            evaluator_kind,
            "provided_local_command" | "official_senior_swe_bench"
        ) {
            return Err(format!(
                "fitness evidence evaluator_kind is not recognized: {evaluator_kind}"
            ));
        }
    }
    Ok(())
}

fn require_evidence_cycle(value: &Value, report_cycle: usize) -> Result<(), String> {
    let cycle = evidence_cycle(value)?;
    if cycle != report_cycle as u64 {
        return Err(format!(
            "fitness evidence cycle {cycle} does not match report cycle {report_cycle}"
        ));
    }
    Ok(())
}

fn evidence_cycle(value: &Value) -> Result<u64, String> {
    value
        .get("cycle")
        .and_then(Value::as_u64)
        .ok_or_else(|| "fitness evidence missing numeric cycle".to_string())
}

fn require_json_bool(value: &Value, field: &str, expected: bool) -> Result<(), String> {
    match value.get(field).and_then(Value::as_bool) {
        Some(actual) if actual == expected => Ok(()),
        _ => Err(format!("fitness evidence field {field} is not {expected}")),
    }
}

fn require_json_bool_field(value: &Value, field: &str) -> Result<bool, String> {
    value
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("fitness evidence missing boolean field {field}"))
}

fn require_json_string(value: &Value, field: &str, expected: &str) -> Result<(), String> {
    match value.get(field).and_then(Value::as_str) {
        Some(actual) if actual == expected => Ok(()),
        _ => Err(format!("fitness evidence field {field} is not {expected}")),
    }
}

fn require_json_nonnegative_u64(value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("fitness evidence missing nonnegative integer field {field}"))
}

fn is_public_fitness_case_name_for_cli(name: &str) -> bool {
    matches!(name, "compiles" | "has_tests" | "all_tests_pass") || name.starts_with("has_")
}

/// Apply accepted system patches to the real source tree.
fn report_has_actual_fitness_evidence(report: &CycleReport) -> bool {
    if report.fitness_delta.is_some_and(|delta| delta < 0.0) {
        return false;
    }

    (report.fitness.is_some() && report.fitness_delta.is_some_and(|delta| delta >= 0.0))
        || report
            .lineage
            .iter()
            .any(|entry| lineage_has_fresh_fitness_evidence(entry, report.cycle))
}

fn lineage_has_fresh_fitness_evidence(entry: &InvocationLineage, report_cycle: usize) -> bool {
    entry
        .inputs
        .get(&ArtifactType::from("fitness_report"))
        .is_some_and(|bytes| is_fresh_non_regressing_fitness_evidence_artifact(bytes, report_cycle))
}

fn is_fresh_non_regressing_fitness_evidence_artifact(bytes: &[u8], report_cycle: usize) -> bool {
    serde_json::from_slice::<Value>(bytes).is_ok_and(|value| {
        value
            .get("schema_version")
            .is_some_and(|schema| schema == "a2d.fitness-evidence.v1")
            && value
                .get("actual_tests_evaluated")
                .is_some_and(|actual| actual == true)
            && value
                .get("non_regressing")
                .is_some_and(|non_regressing| non_regressing == true)
            && value
                .get("delta_from_last_non_regressing_fitness")
                .and_then(Value::as_f64)
                .is_some_and(|delta| delta >= 0.0)
            && value
                .get("cycle")
                .and_then(Value::as_u64)
                .is_some_and(|evidence_cycle| {
                    evidence_cycle.saturating_add(1) == report_cycle as u64
                })
    })
}

fn apply_accepted_patches(metabolism: &Metabolism) {
    let root = project_root();
    for patch in metabolism.pending_patches() {
        let target = root.join(&patch.file_path);
        match fs::write(&target, &patch.new_content) {
            Ok(()) => println!("  AUTOPOIESIS: applied patch to {}", patch.file_path),
            Err(e) => eprintln!("  AUTOPOIESIS: failed to apply {}: {e}", patch.file_path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(name: &str) -> ArtifactType {
        ArtifactType::from(name)
    }

    fn rust_files_under(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(path) = stack.pop() {
            for entry in fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    files.push(path);
                }
            }
        }
        files
    }

    fn fitness(passed: usize, total: usize) -> a2d_core::benchmark::FitnessReport {
        a2d_core::benchmark::FitnessReport {
            total,
            passed,
            failed: total - passed,
            fitness: if total == 0 {
                0.0
            } else {
                passed as f64 / total as f64
            },
            results: Vec::new(),
            diagnostic: None,
        }
    }

    #[test]
    fn score_artifact_exit_code_is_nonzero_unless_perfect() {
        assert_eq!(score_artifact_exit_code(&fitness(6, 6)), 0);
        assert_eq!(score_artifact_exit_code(&fitness(5, 6)), 2);
        assert_eq!(score_artifact_exit_code(&fitness(0, 0)), 2);
    }

    #[test]
    fn senior_swe_bench_task_variant_lookup_finds_hard_and_guided_ids() {
        let task = SeniorSweBenchTask {
            family: "family".to_string(),
            repo: "repo".to_string(),
            repo_slug: "owner/repo".to_string(),
            task_type: "bug".to_string(),
            segment: "investigate".to_string(),
            tags: Vec::new(),
            in_benchmark: true,
            in_sample: false,
            version: "2026.06".to_string(),
            description: "desc".to_string(),
            taxonomy: Default::default(),
            environment: Default::default(),
            hard: Some(SeniorSweBenchVariant {
                task_id: "task-hard".to_string(),
                difficulty: "frontier".to_string(),
            }),
            guided: Some(SeniorSweBenchVariant {
                task_id: "task-guided".to_string(),
                difficulty: "solved".to_string(),
            }),
        };
        let tasks = vec![task];

        assert_eq!(
            find_senior_swe_bench_task_variant(&tasks, "task-hard")
                .map(|(_, variant_name, _)| variant_name),
            Some("hard")
        );
        assert_eq!(
            find_senior_swe_bench_task_variant(&tasks, "task-guided")
                .map(|(_, variant_name, _)| variant_name),
            Some("guided")
        );
        assert!(find_senior_swe_bench_task_variant(&tasks, "missing").is_none());
    }

    #[test]
    fn senior_swe_bench_evaluate_args_require_local_command_after_separator() {
        let args = vec![
            "--task-package".to_string(),
            "task.json".to_string(),
            "--candidate-patch".to_string(),
            "candidate.diff".to_string(),
            "--checkout".to_string(),
            "checkout".to_string(),
            "--".to_string(),
            "./official-evaluator".to_string(),
            "--task".to_string(),
        ];
        let config = parse_senior_swe_bench_evaluate_args(&args).unwrap();
        assert_eq!(config.task_package, Some(PathBuf::from("task.json")));
        assert_eq!(config.task_cycle_input, None);
        assert_eq!(config.candidate_patch, PathBuf::from("candidate.diff"));
        assert_eq!(config.checkout, PathBuf::from("checkout"));
        assert!(!config.apply_candidate_patch);
        assert_eq!(
            config.command,
            vec!["./official-evaluator".to_string(), "--task".to_string()]
        );

        let apply_patch_args = vec![
            "--task-package".to_string(),
            "task.json".to_string(),
            "--candidate-patch".to_string(),
            "candidate.diff".to_string(),
            "--checkout".to_string(),
            "checkout".to_string(),
            "--apply-candidate-patch".to_string(),
            "--".to_string(),
            "./official-evaluator".to_string(),
        ];
        let apply_config = parse_senior_swe_bench_evaluate_args(&apply_patch_args).unwrap();
        assert!(apply_config.apply_candidate_patch);

        let cycle_input_args = vec![
            "--task-cycle-input".to_string(),
            "cycle-input.json".to_string(),
            "--candidate-patch".to_string(),
            "candidate.diff".to_string(),
            "--checkout".to_string(),
            "checkout".to_string(),
            "--".to_string(),
            "./official-evaluator".to_string(),
        ];
        let cycle_config = parse_senior_swe_bench_evaluate_args(&cycle_input_args).unwrap();
        assert_eq!(cycle_config.task_package, None);
        assert_eq!(
            cycle_config.task_cycle_input,
            Some(PathBuf::from("cycle-input.json"))
        );

        let official_manifest_args = vec![
            "--task-package".to_string(),
            "task.json".to_string(),
            "--candidate-patch".to_string(),
            "candidate.diff".to_string(),
            "--checkout".to_string(),
            "checkout".to_string(),
            "--official-evaluator-manifest".to_string(),
            "manifest.json".to_string(),
            "--".to_string(),
            "./official-evaluator".to_string(),
        ];
        let official_config =
            parse_senior_swe_bench_evaluate_args(&official_manifest_args).unwrap();
        assert_eq!(
            official_config.official_evaluator_manifest,
            Some(PathBuf::from("manifest.json"))
        );

        let artifact_patch_args = vec![
            "--task-package".to_string(),
            "task.json".to_string(),
            "--candidate-patch-artifact".to_string(),
            "coder-output.md".to_string(),
            "--extracted-candidate-patch".to_string(),
            "candidate.diff".to_string(),
            "--checkout".to_string(),
            "checkout".to_string(),
            "--".to_string(),
            "./official-evaluator".to_string(),
        ];
        let artifact_config = parse_senior_swe_bench_evaluate_args(&artifact_patch_args).unwrap();
        assert_eq!(
            artifact_config.candidate_patch_artifact,
            Some(PathBuf::from("coder-output.md"))
        );
        assert_eq!(
            artifact_config.extracted_candidate_patch,
            Some(PathBuf::from("candidate.diff"))
        );
        assert_eq!(
            artifact_config.candidate_patch,
            PathBuf::from("candidate.diff")
        );

        let both_candidate_inputs = vec![
            "--task-package".to_string(),
            "task.json".to_string(),
            "--candidate-patch".to_string(),
            "candidate.diff".to_string(),
            "--candidate-patch-artifact".to_string(),
            "coder-output.md".to_string(),
            "--extracted-candidate-patch".to_string(),
            "candidate.diff".to_string(),
            "--checkout".to_string(),
            "checkout".to_string(),
            "--".to_string(),
            "./official-evaluator".to_string(),
        ];
        assert!(
            parse_senior_swe_bench_evaluate_args(&both_candidate_inputs)
                .unwrap_err()
                .contains("not both")
        );

        let missing_extracted_patch = vec![
            "--task-package".to_string(),
            "task.json".to_string(),
            "--candidate-patch-artifact".to_string(),
            "coder-output.md".to_string(),
            "--checkout".to_string(),
            "checkout".to_string(),
            "--".to_string(),
            "./official-evaluator".to_string(),
        ];
        assert!(
            parse_senior_swe_bench_evaluate_args(&missing_extracted_patch)
                .unwrap_err()
                .contains("extracted-candidate-patch")
        );

        let both_task_inputs = vec![
            "--task-package".to_string(),
            "task.json".to_string(),
            "--task-cycle-input".to_string(),
            "cycle-input.json".to_string(),
            "--candidate-patch".to_string(),
            "candidate.diff".to_string(),
            "--checkout".to_string(),
            "checkout".to_string(),
            "--".to_string(),
            "./official-evaluator".to_string(),
        ];
        assert!(
            parse_senior_swe_bench_evaluate_args(&both_task_inputs)
                .unwrap_err()
                .contains("not both")
        );

        let missing_command = vec![
            "--task-package".to_string(),
            "task.json".to_string(),
            "--candidate-patch".to_string(),
            "candidate.diff".to_string(),
            "--checkout".to_string(),
            "checkout".to_string(),
        ];
        assert!(parse_senior_swe_bench_evaluate_args(&missing_command).is_err());
    }

    #[test]
    fn senior_swe_bench_evaluate_requires_official_manifest_inspection_sidecar() {
        let config = SeniorSweBenchEvaluateConfig {
            task_package: Some(PathBuf::from("task.json")),
            task_cycle_input: None,
            candidate_patch: PathBuf::from("candidate.diff"),
            candidate_patch_artifact: None,
            extracted_candidate_patch: None,
            checkout: PathBuf::from("checkout"),
            output: None,
            apply_candidate_patch: false,
            official_evaluator_manifest: Some(PathBuf::from("official-manifest.json")),
            official_evaluator_manifest_inspection: None,
            command: vec!["./official-evaluator".to_string()],
        };
        let package = SeniorSweBenchTaskPackageSummary {
            task_id: "task-hard".to_string(),
            repo: "owner/repo".to_string(),
            github_solution_search_allowed: false,
        };

        let error = load_senior_swe_bench_official_evaluator_manifest(&config, &package)
            .expect_err("official evaluator manifest requires prior inspection sidecar");

        assert!(
            error.contains("official-evaluator-manifest-inspection"),
            "{error}"
        );
    }

    #[test]
    fn senior_swe_bench_candidate_patch_preflight_checks_applicability_without_applying() {
        let root = env::temp_dir().join(format!(
            "a2d-senior-swe-bench-preflight-{}",
            unique_suffix()
        ));
        let checkout = root.join("checkout");
        fs::create_dir_all(&checkout).unwrap();
        fs::write(checkout.join("lib.rs"), "original\n").unwrap();
        let patch = root.join("candidate.diff");
        fs::write(
            &patch,
            "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-original\n+patched\n",
        )
        .unwrap();

        let command = validate_senior_swe_bench_candidate_patch_applicable(&checkout, &patch)
            .expect("applicable candidate patch passes preflight");

        assert!(command.contains("git apply --check"));
        assert_eq!(
            fs::read_to_string(checkout.join("lib.rs")).unwrap(),
            "original\n"
        );

        fs::write(&patch, "not a unified diff\n").unwrap();
        let error = validate_senior_swe_bench_candidate_patch_applicable(&checkout, &patch)
            .expect_err("malformed candidate patch fails preflight");
        assert!(error.contains("git apply --check"));
        assert_eq!(
            fs::read_to_string(checkout.join("lib.rs")).unwrap(),
            "original\n"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn senior_swe_bench_prepare_checkout_applies_patch_to_isolated_copy() {
        let root =
            env::temp_dir().join(format!("a2d-senior-swe-bench-prepare-{}", unique_suffix()));
        let checkout = root.join("checkout");
        fs::create_dir_all(&checkout).unwrap();
        fs::write(checkout.join("lib.rs"), "original\n").unwrap();
        let patch = root.join("candidate.diff");
        fs::write(
            &patch,
            "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-original\n+patched\n",
        )
        .unwrap();
        let config = SeniorSweBenchEvaluateConfig {
            task_package: Some(root.join("task.json")),
            task_cycle_input: None,
            candidate_patch: patch,
            candidate_patch_artifact: None,
            extracted_candidate_patch: None,
            checkout: checkout.clone(),
            output: None,
            apply_candidate_patch: true,
            official_evaluator_manifest: None,
            official_evaluator_manifest_inspection: None,
            command: vec!["sh".to_string(), "evaluator.sh".to_string()],
        };

        let prepared = prepare_senior_swe_bench_evaluator_checkout(&config).unwrap();

        assert!(prepared.candidate_patch_applied);
        assert_eq!(prepared.evaluator_checkout_mode, "isolated_copy");
        assert_ne!(prepared.evaluator_checkout, checkout);
        assert_eq!(
            fs::read_to_string(checkout.join("lib.rs")).unwrap(),
            "original\n"
        );
        assert_eq!(
            fs::read_to_string(prepared.evaluator_checkout.join("lib.rs")).unwrap(),
            "patched\n"
        );
        drop(prepared);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn senior_swe_bench_evaluator_runs_in_patched_checkout_when_enabled() {
        let root = env::temp_dir().join(format!(
            "a2d-senior-swe-bench-evaluator-{}",
            unique_suffix()
        ));
        let checkout = root.join("checkout");
        fs::create_dir_all(&checkout).unwrap();
        fs::write(checkout.join("lib.rs"), "original\n").unwrap();
        let patch = root.join("candidate.diff");
        fs::write(
            &patch,
            "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-original\n+patched\n",
        )
        .unwrap();
        let evaluator = root.join("evaluator.sh");
        fs::write(
            &evaluator,
            "set -eu\ntest \"$(cat lib.rs)\" = \"patched\"\ntest \"$(cat \"$A2D_SENIOR_SWE_BENCH_ORIGINAL_CHECKOUT/lib.rs\")\" = \"original\"\ntest \"$A2D_SENIOR_SWE_BENCH_CANDIDATE_PATCH_APPLIED\" = \"true\"\ntest \"$A2D_SENIOR_SWE_BENCH_GITHUB_SOLUTION_SEARCH_ALLOWED\" = \"false\"\ntest \"$A2D_SENIOR_SWE_BENCH_PUBLIC_SOLUTION_SEARCH_FORBIDDEN\" = \"true\"\necho patched-checkout-ok\n",
        )
        .unwrap();
        let config = SeniorSweBenchEvaluateConfig {
            task_package: Some(root.join("task.json")),
            task_cycle_input: None,
            candidate_patch: patch,
            candidate_patch_artifact: None,
            extracted_candidate_patch: None,
            checkout: checkout.clone(),
            output: None,
            apply_candidate_patch: true,
            official_evaluator_manifest: None,
            official_evaluator_manifest_inspection: None,
            command: vec!["sh".to_string(), evaluator.to_string_lossy().to_string()],
        };
        let package = SeniorSweBenchTaskPackageSummary {
            task_id: "task-hard".to_string(),
            repo: "owner/repo".to_string(),
            github_solution_search_allowed: false,
        };

        let prepared = prepare_senior_swe_bench_evaluator_checkout(&config).unwrap();
        let outcome = run_local_senior_swe_bench_evaluator(&package, &config, &prepared);

        assert!(outcome.status_success, "stderr: {}", outcome.stderr);
        assert!(outcome.stdout.contains("patched-checkout-ok"));
        assert_eq!(
            fs::read_to_string(checkout.join("lib.rs")).unwrap(),
            "original\n"
        );
        drop(prepared);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn senior_swe_bench_rejects_patched_temp_root_inside_original_checkout() {
        let root = env::temp_dir().join(format!(
            "a2d-senior-swe-bench-temp-root-{}",
            unique_suffix()
        ));
        let checkout = root.join("checkout");
        fs::create_dir_all(&checkout).unwrap();

        let direct_descendant = checkout.join("patched-temp");
        assert!(
            validate_patched_checkout_temp_root(&checkout, &direct_descendant)
                .unwrap_err()
                .contains("inside original checkout")
        );

        #[cfg(unix)]
        {
            let checkout_link = root.join("checkout-link");
            std::os::unix::fs::symlink(&checkout, &checkout_link).unwrap();
            let symlink_descendant = checkout_link.join("patched-temp");
            assert!(
                validate_patched_checkout_temp_root(&checkout, &symlink_descendant)
                    .unwrap_err()
                    .contains("inside original checkout")
            );
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn senior_swe_bench_checkout_fingerprint_detects_new_files_in_empty_directories() {
        let root = env::temp_dir().join(format!(
            "a2d-senior-swe-bench-fingerprint-dir-{}",
            unique_suffix()
        ));
        let checkout = root.join("checkout");
        let empty = checkout.join("empty");
        fs::create_dir_all(&empty).unwrap();

        let before = checkout_content_fingerprint(&checkout).unwrap();
        fs::write(empty.join("created-by-evaluator.txt"), "new\n").unwrap();
        let after = checkout_content_fingerprint(&checkout).unwrap();

        assert_ne!(before, after);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn senior_swe_bench_checkout_fingerprint_detects_symlink_retargeting() {
        let root = env::temp_dir().join(format!(
            "a2d-senior-swe-bench-fingerprint-symlink-{}",
            unique_suffix()
        ));
        let checkout = root.join("checkout");
        fs::create_dir_all(&checkout).unwrap();
        fs::write(checkout.join("target-a.txt"), "a\n").unwrap();
        fs::write(checkout.join("target-b.txt"), "b\n").unwrap();
        let link = checkout.join("link.txt");
        std::os::unix::fs::symlink("target-a.txt", &link).unwrap();

        let before = checkout_content_fingerprint(&checkout).unwrap();
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("target-b.txt", &link).unwrap();
        let after = checkout_content_fingerprint(&checkout).unwrap();

        assert_ne!(before, after);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn senior_swe_bench_apply_patch_detects_original_mutation_through_symlink_escape() {
        let root = env::temp_dir().join(format!(
            "a2d-senior-swe-bench-symlink-escape-{}",
            unique_suffix()
        ));
        let checkout = root.join("checkout");
        fs::create_dir_all(&checkout).unwrap();
        fs::write(checkout.join("lib.rs"), "original\n").unwrap();
        fs::write(checkout.join("target.txt"), "original-target\n").unwrap();
        std::os::unix::fs::symlink(checkout.join("target.txt"), checkout.join("escape")).unwrap();
        let patch = root.join("candidate.diff");
        fs::write(
            &patch,
            "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-original\n+patched\n",
        )
        .unwrap();
        let evaluator = root.join("evaluator.sh");
        fs::write(
            &evaluator,
            "set -eu\ntest \"$(cat lib.rs)\" = \"patched\"\nprintf mutated-through-symlink > escape\n",
        )
        .unwrap();
        let config = SeniorSweBenchEvaluateConfig {
            task_package: Some(root.join("task.json")),
            task_cycle_input: None,
            candidate_patch: patch,
            candidate_patch_artifact: None,
            extracted_candidate_patch: None,
            checkout: checkout.clone(),
            output: None,
            apply_candidate_patch: true,
            official_evaluator_manifest: None,
            official_evaluator_manifest_inspection: None,
            command: vec!["sh".to_string(), evaluator.to_string_lossy().to_string()],
        };
        let package = SeniorSweBenchTaskPackageSummary {
            task_id: "task-hard".to_string(),
            repo: "owner/repo".to_string(),
            github_solution_search_allowed: false,
        };
        let before = checkout_content_fingerprint(&checkout).unwrap();
        let prepared = prepare_senior_swe_bench_evaluator_checkout(&config).unwrap();
        let mut outcome = run_local_senior_swe_bench_evaluator(&package, &config, &prepared);

        assert!(outcome.status_success, "stderr: {}", outcome.stderr);
        assert!(original_checkout_mutated_after_evaluator(
            &checkout,
            &before,
            &mut outcome
        ));
        assert_eq!(
            fs::read_to_string(checkout.join("target.txt")).unwrap(),
            "mutated-through-symlink"
        );
        drop(prepared);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn senior_swe_bench_patch_apply_failure_prevents_prepared_checkout() {
        let root =
            env::temp_dir().join(format!("a2d-senior-swe-bench-badpatch-{}", unique_suffix()));
        let checkout = root.join("checkout");
        fs::create_dir_all(&checkout).unwrap();
        fs::write(checkout.join("lib.rs"), "original\n").unwrap();
        let patch = root.join("candidate.diff");
        fs::write(&patch, "not a unified diff\n").unwrap();
        let config = SeniorSweBenchEvaluateConfig {
            task_package: Some(root.join("task.json")),
            task_cycle_input: None,
            candidate_patch: patch,
            candidate_patch_artifact: None,
            extracted_candidate_patch: None,
            checkout: checkout.clone(),
            output: None,
            apply_candidate_patch: true,
            official_evaluator_manifest: None,
            official_evaluator_manifest_inspection: None,
            command: vec!["sh".to_string(), "evaluator.sh".to_string()],
        };

        let error = prepare_senior_swe_bench_evaluator_checkout(&config).unwrap_err();

        assert!(error.contains("git apply"));
        assert_eq!(
            fs::read_to_string(checkout.join("lib.rs")).unwrap(),
            "original\n"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn senior_swe_bench_local_fitness_evidence_contains_holdout_and_policy_status() {
        let report = senior_swe_bench_local_fitness_report(&SeniorSweBenchLocalOutcome {
            status_success: true,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        });
        let names = report
            .results
            .iter()
            .map(|result| result.name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"all_tests_pass"));
        assert!(names.contains(&"hidden_acceptance"));
        assert!(names.contains(&"has_no_solution_search_policy_declared"));
        assert!(!names.contains(&"has_no_solution_search"));
        assert_eq!(report.fitness, 1.0);
        assert_eq!(standalone_fitness_evidence_delta(&report), 1.0);
    }

    #[test]
    fn senior_swe_bench_official_manifest_is_serialized_into_fitness_evidence() {
        let export_dir = a2d_project_root().join("target").join(format!(
            "a2d-senior-swe-bench-official-evidence-{}",
            unique_suffix()
        ));
        let manifest_path = export_dir.join("official-manifest.json");
        let evaluator_checkout = export_dir.join("checkout");
        let candidate_patch = export_dir.join("candidate.diff");
        fs::create_dir_all(&evaluator_checkout).unwrap();
        fs::write(&candidate_patch, "diff --git a/lib.rs b/lib.rs\n").unwrap();
        let command = vec!["./official-evaluator".to_string()];
        let package = SeniorSweBenchTaskPackageSummary {
            task_id: "task-hard".to_string(),
            repo: "owner/repo".to_string(),
            github_solution_search_allowed: false,
        };
        let manifest = SeniorSweBenchOfficialEvaluatorManifestSummary {
            benchmark_url: "https://senior-swe-bench.snorkel.ai/tasks".to_string(),
            task_id: package.task_id.clone(),
            repo: package.repo.clone(),
            hidden_holdouts: true,
            github_solution_search_allowed: false,
            benchmark_provided_command: command.clone(),
        };
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&json!({
                "schema_version": "a2d.senior-swe-bench-official-evaluator-manifest.v1",
                "benchmark_url": manifest.benchmark_url,
                "task_id": manifest.task_id,
                "repo": manifest.repo,
                "hidden_holdouts": manifest.hidden_holdouts,
                "github_solution_search_allowed": manifest.github_solution_search_allowed,
                "benchmark_provided_command": manifest.benchmark_provided_command,
            }))
            .unwrap(),
        )
        .unwrap();
        let inspection_path = export_dir.join("official-manifest-inspection.json");
        let inspection = build_senior_swe_bench_official_evaluator_manifest_inspection_value(
            &package,
            &manifest_path,
            &command,
        )
        .unwrap();
        fs::write(
            &inspection_path,
            serde_json::to_vec_pretty(&inspection).unwrap(),
        )
        .unwrap();
        let manifest_hash = file_content_hash(&manifest_path).unwrap();
        let inspection_hash = file_content_hash(&inspection_path).unwrap();
        let candidate_patch_hash = file_content_hash(&candidate_patch).unwrap();
        let report = senior_swe_bench_local_fitness_report(&SeniorSweBenchLocalOutcome {
            status_success: true,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        });

        let evidence_path = export_standalone_fitness_evidence(
            &report,
            &export_dir,
            "senior-swe-bench-task-hard-official",
            Some(&candidate_patch_hash),
            None,
            None,
            Some("official_senior_swe_bench"),
            Some(true),
            Some("isolated_copy"),
            Some(false),
            Some(&candidate_patch),
            Some(&evaluator_checkout),
            Some(true),
            Some("passed"),
            Some("git apply --check --whitespace=nowarn -- candidate.diff"),
            Some(&manifest_path),
            Some(&manifest_hash),
            Some(&inspection_path),
            Some(&inspection_hash),
            Some(&manifest),
        )
        .expect("official Senior SWE-Bench evidence exports with manifest provenance");
        let evidence: Value = serde_json::from_slice(&fs::read(&evidence_path).unwrap()).unwrap();

        assert_eq!(
            evidence["evaluator_kind"].as_str(),
            Some("official_senior_swe_bench")
        );
        assert_eq!(
            evidence["official_evaluator_manifest_hash"].as_str(),
            Some(manifest_hash.as_str())
        );
        assert_eq!(
            evidence["official_evaluator_manifest_inspection_hash"].as_str(),
            Some(inspection_hash.as_str())
        );
        assert_eq!(
            evidence["official_evaluator_manifest_inspection_validated"].as_bool(),
            Some(true)
        );
        assert_eq!(
            evidence["official_benchmark_url"].as_str(),
            Some("https://senior-swe-bench.snorkel.ai/tasks")
        );
        assert_eq!(evidence["official_task_id"].as_str(), Some("task-hard"));
        assert_eq!(evidence["official_repo"].as_str(), Some("owner/repo"));
        assert_eq!(evidence["official_hidden_holdouts"].as_bool(), Some(true));
        assert_eq!(
            evidence["official_github_solution_search_allowed"].as_bool(),
            Some(false)
        );
        assert_eq!(
            evidence["official_benchmark_provided_command"]
                .as_array()
                .unwrap()[0]
                .as_str(),
            Some("./official-evaluator")
        );
        validate_fitness_evidence_candidate_patch_binding(
            &evidence_path,
            &candidate_patch,
            Some(true),
            Some("isolated_copy"),
            Some(false),
            Some(&evaluator_checkout),
            None,
        )
        .expect("official evidence binding validates manifest-backed evidence");
        let _ = fs::remove_dir_all(export_dir);
    }

    #[test]
    fn official_evidence_validation_resolves_repo_relative_manifest_files() {
        let relative_dir = PathBuf::from("target").join(format!(
            "a2d-official-evidence-relative-{}",
            unique_suffix()
        ));
        let absolute_dir = a2d_project_root().join(&relative_dir);
        fs::create_dir_all(&absolute_dir).unwrap();
        let manifest_path = absolute_dir.join("manifest.json");
        let inspection_path = absolute_dir.join("inspection.json");
        let command = vec!["./official-evaluator".to_string(), "--task".to_string()];
        let package = SeniorSweBenchTaskPackageSummary {
            task_id: "task-hard".to_string(),
            repo: "owner/repo".to_string(),
            github_solution_search_allowed: false,
        };
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&json!({
                "schema_version": "a2d.senior-swe-bench-official-evaluator-manifest.v1",
                "benchmark_url": "https://senior-swe-bench.snorkel.ai/tasks/task-hard",
                "task_id": package.task_id,
                "repo": package.repo,
                "hidden_holdouts": true,
                "github_solution_search_allowed": false,
                "benchmark_provided_command": command,
            }))
            .unwrap(),
        )
        .unwrap();
        let package = SeniorSweBenchTaskPackageSummary {
            task_id: "task-hard".to_string(),
            repo: "owner/repo".to_string(),
            github_solution_search_allowed: false,
        };
        let command = vec!["./official-evaluator".to_string(), "--task".to_string()];
        let inspection = build_senior_swe_bench_official_evaluator_manifest_inspection_value(
            &package,
            &manifest_path,
            &command,
        )
        .unwrap();
        fs::write(
            &inspection_path,
            serde_json::to_vec_pretty(&inspection).unwrap(),
        )
        .unwrap();

        let report = senior_swe_bench_local_fitness_report(&SeniorSweBenchLocalOutcome {
            status_success: true,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        });
        let mut evidence: Value = serde_json::from_slice(&fitness_evidence_artifact(
            0,
            &report,
            standalone_fitness_evidence_delta(&report),
        ))
        .unwrap();
        evidence = add_export_source_provenance(evidence).unwrap();
        evidence["evaluator_kind"] = json!("official_senior_swe_bench");
        evidence["official_evaluator_manifest_path"] = json!(
            relative_dir
                .join("manifest.json")
                .to_string_lossy()
                .to_string()
        );
        evidence["official_evaluator_manifest_hash"] =
            json!(file_content_hash(&manifest_path).unwrap());
        evidence["official_evaluator_manifest_inspection_path"] = json!(
            relative_dir
                .join("inspection.json")
                .to_string_lossy()
                .to_string()
        );
        evidence["official_evaluator_manifest_inspection_hash"] =
            json!(file_content_hash(&inspection_path).unwrap());
        evidence["official_evaluator_manifest_inspection_validated"] = json!(true);
        evidence["official_benchmark_url"] =
            json!("https://senior-swe-bench.snorkel.ai/tasks/task-hard");
        evidence["official_task_id"] = json!("task-hard");
        evidence["official_repo"] = json!("owner/repo");
        evidence["official_hidden_holdouts"] = json!(true);
        evidence["official_github_solution_search_allowed"] = json!(false);
        evidence["official_benchmark_provided_command"] = json!(["./official-evaluator", "--task"]);

        validate_exported_fitness_evidence_value(&evidence)
            .expect("repo-relative manifest and inspection paths validate");

        let mut escaping_relative = evidence.clone();
        escaping_relative["official_evaluator_manifest_path"] = json!("../AGENTS.md");
        assert!(
            validate_exported_fitness_evidence_value(&escaping_relative)
                .expect_err("escaping relative official manifest path is rejected")
                .contains("escapes the A²D project root")
        );

        let mut missing_relative = evidence.clone();
        missing_relative["official_evaluator_manifest_path"] = json!(
            relative_dir
                .join("missing-manifest.json")
                .to_string_lossy()
                .to_string()
        );
        assert!(
            validate_exported_fitness_evidence_value(&missing_relative)
                .expect_err("missing relative official manifest is rejected")
                .contains("does not resolve under the A²D project root")
        );

        let outside_dir =
            env::temp_dir().join(format!("a2d-official-evidence-outside-{}", unique_suffix()));
        fs::create_dir_all(&outside_dir).unwrap();
        let outside_manifest = outside_dir.join("manifest.json");
        fs::write(&outside_manifest, fs::read(&manifest_path).unwrap()).unwrap();
        let mut outside_absolute = evidence.clone();
        outside_absolute["official_evaluator_manifest_path"] =
            json!(outside_manifest.to_string_lossy().to_string());
        assert!(
            validate_exported_fitness_evidence_value(&outside_absolute)
                .expect_err("absolute official manifest path outside project root is rejected")
                .contains("resolves outside the A²D project root")
        );
        let _ = fs::remove_dir_all(outside_dir);
        let _ = fs::remove_dir_all(absolute_dir);
    }

    #[test]
    fn senior_swe_bench_exported_fitness_evidence_binds_candidate_patch_hash() {
        let candidate_patch_path = env::temp_dir().join(format!(
            "a2d-senior-swe-bench-candidate-{}.diff",
            unique_suffix()
        ));
        fs::write(&candidate_patch_path, "diff --git a/lib.rs b/lib.rs\n").unwrap();
        let candidate_patch_hash = file_content_hash(&candidate_patch_path).unwrap();
        let candidate_patch_artifact_path = env::temp_dir().join(format!(
            "a2d-senior-swe-bench-candidate-artifact-{}.md",
            unique_suffix()
        ));
        fs::write(
            &candidate_patch_artifact_path,
            "Here is the patch:\n```diff\ndiff --git a/lib.rs b/lib.rs\n```\n",
        )
        .unwrap();
        let candidate_patch_artifact_hash =
            file_content_hash(&candidate_patch_artifact_path).unwrap();
        let evaluator_checkout_path = env::temp_dir().join(format!(
            "a2d-senior-swe-bench-evaluator-checkout-{}",
            unique_suffix()
        ));
        fs::create_dir_all(&evaluator_checkout_path).unwrap();
        let other_evaluator_checkout_path = env::temp_dir().join(format!(
            "a2d-senior-swe-bench-other-evaluator-checkout-{}",
            unique_suffix()
        ));
        fs::create_dir_all(&other_evaluator_checkout_path).unwrap();
        let report = senior_swe_bench_local_fitness_report(&SeniorSweBenchLocalOutcome {
            status_success: true,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        });
        let mut evidence: Value = serde_json::from_slice(&fitness_evidence_artifact(
            0,
            &report,
            standalone_fitness_evidence_delta(&report),
        ))
        .expect("evidence serializes as JSON");
        evidence = add_export_source_provenance(evidence).unwrap();
        let object = evidence.as_object_mut().unwrap();
        object.insert(
            "candidate_patch_hash".to_string(),
            Value::String(candidate_patch_hash.clone()),
        );
        object.insert(
            "candidate_patch_path".to_string(),
            Value::String(candidate_patch_path.to_string_lossy().to_string()),
        );
        object.insert(
            "evaluator_kind".to_string(),
            Value::String("provided_local_command".to_string()),
        );
        object.insert(
            "evaluator_checkout".to_string(),
            Value::String(evaluator_checkout_path.to_string_lossy().to_string()),
        );
        object.insert("candidate_patch_applied".to_string(), Value::Bool(true));
        object.insert(
            "evaluator_checkout_mode".to_string(),
            Value::String("isolated_copy".to_string()),
        );
        object.insert("original_checkout_mutated".to_string(), Value::Bool(false));
        object.insert(
            "candidate_patch_preflight_checked".to_string(),
            Value::Bool(true),
        );
        object.insert(
            "candidate_patch_preflight_status".to_string(),
            Value::String("passed".to_string()),
        );
        object.insert(
            "candidate_patch_preflight_command".to_string(),
            Value::String(format!(
                "git apply --check --whitespace=nowarn -- {}",
                candidate_patch_path.display()
            )),
        );
        object.insert(
            "evidence_command".to_string(),
            Value::String("senior-swe-bench-evaluate --task-package task.json".to_string()),
        );

        validate_exported_fitness_evidence_value(&evidence).unwrap();
        assert_eq!(
            evidence["candidate_patch_hash"].as_str(),
            Some(candidate_patch_hash.as_str())
        );
        assert_eq!(
            evidence["candidate_patch_path"].as_str(),
            Some(candidate_patch_path.to_str().unwrap())
        );
        assert_eq!(
            evidence["evaluator_kind"].as_str(),
            Some("provided_local_command")
        );

        let mut misleading_local = evidence.clone();
        misleading_local["official_hidden_holdouts"] = json!(true);
        assert!(
            validate_exported_fitness_evidence_value(&misleading_local)
                .expect_err("provided-local evidence cannot carry official fields")
                .contains("non-official evaluator evidence")
        );

        let mut incomplete_official = evidence.clone();
        incomplete_official["evaluator_kind"] = json!("official_senior_swe_bench");
        assert!(
            validate_exported_fitness_evidence_value(&incomplete_official)
                .expect_err("official evidence requires manifest provenance")
                .contains("official Senior SWE-Bench evidence missing")
        );

        let official_dir = a2d_project_root().join("target").join(format!(
            "a2d-senior-swe-bench-official-binding-{}",
            unique_suffix()
        ));
        fs::create_dir_all(&official_dir).unwrap();
        let official_manifest_path = official_dir.join("manifest.json");
        let official_inspection_path = official_dir.join("inspection.json");
        let official_package = SeniorSweBenchTaskPackageSummary {
            task_id: "task-hard".to_string(),
            repo: "owner/repo".to_string(),
            github_solution_search_allowed: false,
        };
        let official_command = vec!["./official-evaluator".to_string()];
        fs::write(
            &official_manifest_path,
            serde_json::to_vec_pretty(&json!({
                "schema_version": "a2d.senior-swe-bench-official-evaluator-manifest.v1",
                "benchmark_url": "https://senior-swe-bench.snorkel.ai/tasks",
                "task_id": official_package.task_id,
                "repo": official_package.repo,
                "hidden_holdouts": true,
                "github_solution_search_allowed": false,
                "benchmark_provided_command": official_command,
            }))
            .unwrap(),
        )
        .unwrap();
        let official_package = SeniorSweBenchTaskPackageSummary {
            task_id: "task-hard".to_string(),
            repo: "owner/repo".to_string(),
            github_solution_search_allowed: false,
        };
        let official_command = vec!["./official-evaluator".to_string()];
        let official_inspection =
            build_senior_swe_bench_official_evaluator_manifest_inspection_value(
                &official_package,
                &official_manifest_path,
                &official_command,
            )
            .unwrap();
        fs::write(
            &official_inspection_path,
            serde_json::to_vec_pretty(&official_inspection).unwrap(),
        )
        .unwrap();
        let official_manifest_hash = file_content_hash(&official_manifest_path).unwrap();
        let official_inspection_hash = file_content_hash(&official_inspection_path).unwrap();

        let mut official = incomplete_official;
        official["official_evaluator_manifest_path"] =
            json!(official_manifest_path.to_string_lossy().to_string());
        official["official_evaluator_manifest_hash"] = json!(official_manifest_hash);
        official["official_evaluator_manifest_inspection_path"] =
            json!(official_inspection_path.to_string_lossy().to_string());
        official["official_evaluator_manifest_inspection_hash"] = json!(official_inspection_hash);
        official["official_evaluator_manifest_inspection_validated"] = json!(true);
        official["official_benchmark_url"] = json!("https://senior-swe-bench.snorkel.ai/tasks");
        official["official_task_id"] = json!("task-hard");
        official["official_repo"] = json!("owner/repo");
        official["official_hidden_holdouts"] = json!(true);
        official["official_github_solution_search_allowed"] = json!(false);
        official["official_benchmark_provided_command"] = json!(["./official-evaluator"]);
        validate_exported_fitness_evidence_value(&official)
            .expect("official Senior SWE-Bench evidence with manifest provenance is accepted");
        official["official_hidden_holdouts"] = json!(false);
        assert!(
            validate_exported_fitness_evidence_value(&official)
                .expect_err("official evidence without hidden holdouts is rejected")
                .contains("official_hidden_holdouts")
        );

        let evidence_path = env::temp_dir().join(format!(
            "a2d-senior-swe-bench-evidence-{}.json",
            unique_suffix()
        ));
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&evidence).expect("evidence serializes"),
        )
        .unwrap();
        validate_fitness_evidence_candidate_patch_binding(
            &evidence_path,
            &candidate_patch_path,
            Some(true),
            Some("isolated_copy"),
            Some(false),
            Some(&evaluator_checkout_path),
            None,
        )
        .expect("matching candidate patch binding is accepted");

        let mut artifact_bound = evidence.clone();
        artifact_bound["candidate_patch_artifact_path"] =
            json!(candidate_patch_artifact_path.to_string_lossy().to_string());
        artifact_bound["candidate_patch_artifact_hash"] =
            json!(candidate_patch_artifact_hash.clone());
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&artifact_bound).expect("evidence serializes"),
        )
        .unwrap();
        validate_fitness_evidence_candidate_patch_binding(
            &evidence_path,
            &candidate_patch_path,
            Some(true),
            Some("isolated_copy"),
            Some(false),
            Some(&evaluator_checkout_path),
            Some(&candidate_patch_artifact_path),
        )
        .expect("matching candidate patch artifact binding is accepted");

        let mut mismatched_artifact_hash = artifact_bound.clone();
        mismatched_artifact_hash["candidate_patch_artifact_hash"] =
            json!("0123456789abcdef0123456789abcdef01234567");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&mismatched_artifact_hash).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                Some(&candidate_patch_artifact_path),
            )
            .expect_err("mismatched candidate patch artifact hash is rejected")
            .contains("does not match current candidate patch artifact hash")
        );

        let mut unexpected_artifact = artifact_bound.clone();
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&unexpected_artifact).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                None,
            )
            .expect_err("unexpected candidate patch artifact provenance is rejected")
            .contains("unexpected candidate_patch_artifact")
        );
        unexpected_artifact
            .as_object_mut()
            .unwrap()
            .remove("candidate_patch_artifact_path");
        unexpected_artifact
            .as_object_mut()
            .unwrap()
            .remove("candidate_patch_artifact_hash");

        let mut missing_preflight = evidence.clone();
        missing_preflight
            .as_object_mut()
            .unwrap()
            .remove("candidate_patch_preflight_checked");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&missing_preflight).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                None,
            )
            .expect_err("missing candidate patch preflight is rejected")
            .contains("missing candidate_patch_preflight_checked")
        );

        let mut failed_preflight = evidence.clone();
        failed_preflight["candidate_patch_preflight_status"] = json!("failed");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&failed_preflight).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                None,
            )
            .expect_err("failed candidate patch preflight is rejected")
            .contains("candidate_patch_preflight_status")
        );

        let mut missing_candidate_hash = evidence.clone();
        missing_candidate_hash
            .as_object_mut()
            .unwrap()
            .remove("candidate_patch_hash");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&missing_candidate_hash).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                None,
            )
            .expect_err("missing candidate patch hash is rejected")
            .contains("missing candidate_patch_hash")
        );

        let mut missing_candidate_path = evidence.clone();
        missing_candidate_path
            .as_object_mut()
            .unwrap()
            .remove("candidate_patch_path");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&missing_candidate_path).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                None,
            )
            .expect_err("missing candidate patch path is rejected")
            .contains("missing candidate_patch_path")
        );

        let mut missing_evaluator_kind = evidence.clone();
        missing_evaluator_kind
            .as_object_mut()
            .unwrap()
            .remove("evaluator_kind");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&missing_evaluator_kind).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                None,
            )
            .expect_err("missing evaluator kind is rejected")
            .contains("missing evaluator_kind")
        );

        let mut missing_evaluator_checkout = evidence.clone();
        missing_evaluator_checkout
            .as_object_mut()
            .unwrap()
            .remove("evaluator_checkout");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&missing_evaluator_checkout).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                None,
            )
            .expect_err("missing evaluator checkout is rejected")
            .contains("missing evaluator_checkout")
        );

        let mut missing_applied = evidence.clone();
        missing_applied
            .as_object_mut()
            .unwrap()
            .remove("candidate_patch_applied");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&missing_applied).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                None,
            )
            .expect_err("missing applied flag is rejected")
            .contains("missing candidate_patch_applied")
        );

        let mut mismatched_path = evidence.clone();
        mismatched_path["candidate_patch_path"] = json!("/tmp/other-candidate.diff");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&mismatched_path).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                None,
            )
            .expect_err("mismatched candidate patch path is rejected")
            .contains("does not match current candidate patch path")
        );

        let mut mismatched_evaluator_checkout = evidence.clone();
        mismatched_evaluator_checkout["evaluator_checkout"] =
            json!(other_evaluator_checkout_path.to_string_lossy().to_string());
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&mismatched_evaluator_checkout).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                None,
            )
            .expect_err("mismatched evaluator checkout is rejected")
            .contains("does not match current evaluator checkout")
        );

        let mut mismatched_applied = evidence.clone();
        mismatched_applied["candidate_patch_applied"] = json!(false);
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&mismatched_applied).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                None,
            )
            .expect_err("mismatched applied flag is rejected")
            .contains("candidate_patch_applied")
        );

        evidence["candidate_patch_hash"] = json!("0123456789abcdef0123456789abcdef01234567");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&evidence).expect("evidence serializes"),
        )
        .unwrap();
        assert!(
            validate_fitness_evidence_candidate_patch_binding(
                &evidence_path,
                &candidate_patch_path,
                Some(true),
                Some("isolated_copy"),
                Some(false),
                Some(&evaluator_checkout_path),
                None,
            )
            .expect_err("mismatched candidate patch hash is rejected")
            .contains("does not match current candidate patch hash")
        );

        fs::remove_file(candidate_patch_path).unwrap();
        fs::remove_file(candidate_patch_artifact_path).unwrap();
        fs::remove_file(evidence_path).unwrap();
        fs::remove_dir_all(evaluator_checkout_path).unwrap();
        fs::remove_dir_all(other_evaluator_checkout_path).unwrap();
    }

    #[test]
    fn failed_senior_swe_bench_local_evaluator_is_not_non_regressing_evidence() {
        let report = senior_swe_bench_local_fitness_report(&SeniorSweBenchLocalOutcome {
            status_success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: String::new(),
        });
        let names = report
            .results
            .iter()
            .map(|result| (result.name.as_str(), result.passed))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(names.get("all_tests_pass"), Some(&false));
        assert_eq!(names.get("hidden_acceptance"), Some(&false));
        assert_eq!(
            names.get("has_no_solution_search_policy_declared"),
            Some(&true)
        );
        assert!(!names.contains_key("has_no_solution_search"));
        let delta = standalone_fitness_evidence_delta(&report);
        assert!(delta < 0.0);

        let evidence: Value = serde_json::from_slice(&fitness_evidence_artifact(0, &report, delta))
            .expect("failed report evidence serializes");
        assert_eq!(evidence["schema_version"], "a2d.fitness-evidence.v1");
        assert_eq!(evidence["actual_tests_evaluated"], true);
        assert_eq!(evidence["non_regressing"], false);
        assert_eq!(
            evidence["failed_cases"],
            json!(["all_tests_pass", "hidden_acceptance"])
        );
        assert_eq!(
            fitness_evidence_result_passed(&evidence, "has_no_solution_search_policy_declared"),
            true
        );
    }

    #[test]
    fn a2d_core_does_not_contain_senior_swe_bench_adapter_code() {
        let core_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../a2d-core/src");
        let mut checked = 0usize;
        for file in rust_files_under(&core_src) {
            let content = fs::read_to_string(&file).unwrap();
            assert!(
                !content.contains("senior_swe_bench") && !content.contains("Senior SWE-Bench"),
                "Senior SWE-Bench adapter text leaked into a2d-core file {}",
                file.display()
            );
            checked += 1;
        }
        assert!(checked > 0, "core source scan should inspect Rust files");
    }

    #[test]
    fn a2d_core_does_not_contain_domain_challenge_catalog_code() {
        let core_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../a2d-core/src");
        let forbidden = [
            "sudoku_solver",
            "rubiks_cube",
            "chess_engine",
            "sudoku-solver",
            "rubiks-cube",
            "chess-engine",
            "a2d_rubiks_acceptance",
            "solves_easy_puzzle",
            "seeded_scramble_is_replayable_and_solvable",
        ];
        let mut checked = 0usize;
        for file in rust_files_under(&core_src) {
            let content = fs::read_to_string(&file).unwrap();
            for term in forbidden {
                assert!(
                    !content.contains(term),
                    "domain challenge catalog term {term:?} leaked into a2d-core file {}",
                    file.display()
                );
            }
            checked += 1;
        }
        assert!(checked > 0, "core source scan should inspect Rust files");
    }

    #[test]
    fn score_artifact_report_redacts_hidden_diagnostic() {
        let report = a2d_core::benchmark::FitnessReport {
            total: 2,
            passed: 1,
            failed: 1,
            fitness: 0.5,
            results: vec![
                a2d_core::benchmark::CaseResult {
                    name: "compiles".to_string(),
                    passed: true,
                },
                a2d_core::benchmark::CaseResult {
                    name: "all_tests_pass".to_string(),
                    passed: false,
                },
            ],
            diagnostic: Some("hidden puzzle 800000000003600000 and assertion text".to_string()),
        };

        let output = format_score_artifact_report("sudoku-solver", &report);

        assert!(output.contains("Fitness: 50% (1/2)"));
        assert!(output.contains("✗ all_tests_pass"));
        assert!(output.contains("Diagnostic: captured but not printed"));
        assert!(!output.contains("800000000003600000"));
        assert!(!output.contains("assertion text"));
    }

    fn complete_fitness_evidence(cycle: usize) -> Value {
        json!({
            "actual_tests_evaluated": true,
            "cycle": cycle,
            "delta_from_last_non_regressing_fitness": 0.0,
            "diagnostic_present": false,
            "failed": 0,
            "failed_cases": [],
            "fitness": 1.0,
            "non_regressing": true,
            "passed": 4,
            "results": [
                {"name": "compiles", "passed": true},
                {"name": "has_tests", "passed": true},
                {"name": "all_tests_pass", "passed": true},
                {"name": "hidden_acceptance", "passed": true}
            ],
            "schema_version": "a2d.fitness-evidence.v1",
            "total": 4
        })
    }

    fn validate_value(value: Value, cycle: usize) -> Result<Value, String> {
        validate_exportable_fitness_evidence(&serde_json::to_vec(&value).unwrap(), cycle)
    }

    #[test]
    fn exportable_fitness_evidence_validation_rejects_missing_stale_or_regressing_evidence() {
        assert!(validate_exportable_fitness_evidence(b"not json", 1).is_err());

        let mut stale = complete_fitness_evidence(0);
        stale["cycle"] = json!(0);
        assert!(validate_value(stale, 1).is_err());

        let mut regressing = complete_fitness_evidence(1);
        regressing["non_regressing"] = json!(false);
        regressing["delta_from_last_non_regressing_fitness"] = json!(-0.1);
        assert!(validate_value(regressing, 1).is_err());

        let mut incomplete = complete_fitness_evidence(1);
        incomplete.as_object_mut().unwrap().remove("fitness");
        assert!(validate_value(incomplete, 1).is_err());

        let mut unknown_field = complete_fitness_evidence(1);
        unknown_field["diagnostic"] = json!("must not be exported");
        assert!(validate_value(unknown_field, 1).is_err());
    }

    #[test]
    fn exportable_fitness_evidence_validation_requires_schema_and_redacted_hidden_status() {
        let mut missing_hidden_status = complete_fitness_evidence(2);
        missing_hidden_status["results"] = json!([
            {"name": "compiles", "passed": true},
            {"name": "has_parse_fn", "passed": true}
        ]);
        assert!(validate_value(missing_hidden_status, 2).is_err());

        let mut leaking_hidden_case = complete_fitness_evidence(2);
        leaking_hidden_case["results"] = json!([
            {"name": "hidden_sudoku_backtracking_case", "passed": false},
            {"name": "hidden_acceptance", "passed": false}
        ]);
        leaking_hidden_case["failed_cases"] = json!(["hidden_sudoku_backtracking_case"]);
        assert!(validate_value(leaking_hidden_case, 2).is_err());

        let valid = complete_fitness_evidence(2);
        let value = validate_value(valid, 2).expect("valid evidence");
        assert_eq!(value["schema_version"], "a2d.fitness-evidence.v1");

        let mut missing_official_manifest = complete_fitness_evidence(2);
        missing_official_manifest["evaluator_kind"] = json!("official_senior_swe_bench");
        assert!(
            validate_value(missing_official_manifest, 2)
                .expect_err("generic evidence shape rejects official claims without manifest")
                .contains("official Senior SWE-Bench evidence missing")
        );

        let mut misleading_local = complete_fitness_evidence(2);
        misleading_local["evaluator_kind"] = json!("provided_local_command");
        misleading_local["official_hidden_holdouts"] = json!(true);
        assert!(
            validate_value(misleading_local, 2)
                .expect_err("generic evidence shape rejects official fields on local evidence")
                .contains("non-official evaluator evidence")
        );
    }

    #[test]
    fn fitness_evidence_inspect_requires_current_non_regressing_actual_tests() {
        let mut evidence = add_export_source_provenance(complete_fitness_evidence(0)).unwrap();
        inspect_fitness_evidence_value(&evidence, true)
            .expect("current full-passing evidence is inspectable");

        evidence["results"] = json!([
            {"name": "compiles", "passed": true},
            {"name": "has_tests", "passed": true},
            {"name": "all_tests_pass", "passed": false},
            {"name": "hidden_acceptance", "passed": false}
        ]);
        evidence["failed_cases"] = json!(["all_tests_pass", "hidden_acceptance"]);
        evidence["failed"] = json!(2);
        evidence["passed"] = json!(2);
        evidence["fitness"] = json!(0.5);
        inspect_fitness_evidence_value(&evidence, false).expect(
            "partial non-regressing actual-test evidence is inspectable without pass requirement",
        );
        assert!(
            inspect_fitness_evidence_value(&evidence, true)
                .expect_err("all-tests-pass requirement rejects partial evidence")
                .contains("all_tests_pass")
        );

        evidence["results"] = json!([
            {"name": "compiles", "passed": true},
            {"name": "has_tests", "passed": false},
            {"name": "all_tests_pass", "passed": true}
        ]);
        evidence["failed_cases"] = json!(["has_tests"]);
        evidence["failed"] = json!(1);
        evidence["passed"] = json!(2);
        evidence["fitness"] = json!(2.0 / 3.0);
        let contradictory_error = inspect_fitness_evidence_value(&evidence, true)
            .expect_err("all-tests-pass requirement rejects contradictory failed cases");
        assert!(
            contradictory_error.contains("inconsistent"),
            "{contradictory_error}"
        );

        evidence["non_regressing"] = json!(false);
        let error = inspect_fitness_evidence_value(&evidence, false)
            .expect_err("regressing evidence is rejected");
        assert!(error.contains("non_regressing"), "{error}");
    }

    #[test]
    fn exported_fitness_evidence_validation_requires_source_provenance() {
        let mut evidence = complete_fitness_evidence(0);
        assert!(validate_exported_fitness_evidence_value(&evidence).is_err());

        evidence["source_revision"] =
            json!(git_scope_revision("crates").expect("git revision works"));
        evidence["source_tree_dirty"] = json!(
            !git_status_for_scope("crates")
                .expect("git status works")
                .is_empty()
        );
        evidence["source_diff_scope"] = json!("crates");
        evidence["source_diff_hash"] =
            json!(git_diff_hash_for_scope("crates").expect("git diff hash works"));
        evidence["evidence_command"] = json!("challenge sudoku 1");
        validate_exported_fitness_evidence_value(&evidence).expect("provenance is valid");

        evidence["candidate_patch_hash"] = json!("not-a-git-object-id");
        assert!(validate_exported_fitness_evidence_value(&evidence).is_err());
        evidence["candidate_patch_hash"] = json!(123);
        assert!(validate_exported_fitness_evidence_value(&evidence).is_err());
        evidence["candidate_patch_hash"] = json!("0123456789abcdef0123456789abcdef01234567");
        validate_exported_fitness_evidence_value(&evidence)
            .expect("valid candidate patch hash is accepted");
        evidence["candidate_patch_artifact_path"] = json!("coder-output.md");
        assert!(
            validate_exported_fitness_evidence_value(&evidence)
                .expect_err("artifact provenance requires path and hash")
                .contains("candidate_patch_artifact")
        );
        evidence
            .as_object_mut()
            .unwrap()
            .remove("candidate_patch_artifact_path");
        evidence["candidate_patch_artifact_hash"] =
            json!("0123456789abcdef0123456789abcdef01234567");
        assert!(
            validate_exported_fitness_evidence_value(&evidence)
                .expect_err("artifact provenance requires path and hash")
                .contains("candidate_patch_artifact")
        );
        evidence["candidate_patch_artifact_path"] = json!("coder-output.md");
        validate_exported_fitness_evidence_value(&evidence)
            .expect("complete candidate patch artifact provenance is accepted");
        evidence["candidate_patch_artifact_hash"] = json!("not-a-git-object-id");
        assert!(
            validate_exported_fitness_evidence_value(&evidence)
                .expect_err("artifact hash must be a git object id")
                .contains("candidate_patch_artifact_hash")
        );
        evidence
            .as_object_mut()
            .unwrap()
            .remove("candidate_patch_artifact_path");
        evidence
            .as_object_mut()
            .unwrap()
            .remove("candidate_patch_artifact_hash");
        evidence["evaluator_kind"] = json!(123);
        assert!(validate_exported_fitness_evidence_value(&evidence).is_err());
        evidence["evaluator_kind"] = json!("unreviewed_evaluator");
        assert!(validate_exported_fitness_evidence_value(&evidence).is_err());
        evidence["evaluator_kind"] = json!("provided_local_command");
        validate_exported_fitness_evidence_value(&evidence)
            .expect("recognized provided-local evaluator kind is accepted");
        evidence["evaluator_kind"] = json!("official_senior_swe_bench");
        assert!(
            validate_exported_fitness_evidence_value(&evidence)
                .expect_err("official evaluator kind requires manifest provenance")
                .contains("official Senior SWE-Bench evidence missing")
        );
        evidence
            .as_object_mut()
            .unwrap()
            .remove("candidate_patch_hash");
        evidence.as_object_mut().unwrap().remove("evaluator_kind");

        evidence["source_diff_hash"] = json!("0123456789abcdef0123456789abcdef01234567");
        assert!(validate_exported_fitness_evidence_value(&evidence).is_err());

        evidence["source_diff_hash"] =
            json!(git_diff_hash_for_scope("crates").expect("git diff hash works"));
        evidence["source_revision"] = json!("bogus");
        assert!(validate_exported_fitness_evidence_value(&evidence).is_err());
    }

    #[test]
    fn fitness_evidence_source_status_rejects_untracked_files() {
        reject_untracked_source_files("crates", " M crates/a2d-cli/src/main.rs\n")
            .expect("tracked source changes can be hashed");
        let error = reject_untracked_source_files(
            "crates",
            " M crates/a2d-cli/src/main.rs\n?? crates/a2d-cli/src/new_file.rs\n",
        )
        .expect_err("untracked source files must fail closed");
        assert!(error.contains("untracked source file"), "{error}");
    }

    #[test]
    fn fitness_evidence_export_path_names_labeled_comparison_cycles() {
        assert_eq!(
            fitness_evidence_export_path(
                Path::new("evidence"),
                "sudoku-solver",
                Some("seed"),
                0,
                0,
            ),
            PathBuf::from("evidence/seed-sudoku-solver-cycle-0-fitness-evidence.json")
        );
        assert_eq!(
            fitness_evidence_export_path(
                Path::new("evidence"),
                "sudoku-solver",
                Some("evolved"),
                0,
                1,
            ),
            PathBuf::from(
                "evidence/evolved-sudoku-solver-cycle-0-consumed-by-cycle-1-fitness-evidence.json"
            )
        );
    }

    #[test]
    fn exportable_fitness_evidence_rejects_current_store_evidence_without_current_fitness() {
        let previous = complete_fitness_evidence(1);
        let bytes = serde_json::to_vec(&previous).expect("fixture serializes");
        let report = CycleReport {
            cycle: 2,
            fitness: None,
            fitness_delta: None,
            ..Default::default()
        };

        assert!(select_exportable_fitness_evidence(&report, Some(&bytes)).is_err());
    }

    #[test]
    fn exportable_fitness_evidence_ignores_invalid_current_store_when_lineage_input_is_fresh() {
        let previous = complete_fitness_evidence(1);
        let mut lineage = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: BTreeMap::new(),
        });
        lineage.inputs.insert(
            art("fitness_report"),
            serde_json::to_vec(&previous).expect("fixture serializes"),
        );
        let report = CycleReport {
            cycle: 2,
            fitness: None,
            fitness_delta: None,
            lineage: vec![lineage],
            ..Default::default()
        };

        let selected = select_exportable_fitness_evidence(&report, Some(b"provider forged text"))
            .expect("fresh lineage input evidence");

        assert_eq!(selected["cycle"], 1);
        assert_eq!(selected["schema_version"], "a2d.fitness-evidence.v1");
    }

    #[test]
    fn exportable_fitness_evidence_can_come_from_fresh_consumed_previous_cycle() {
        let previous = complete_fitness_evidence(1);
        let mut lineage = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: BTreeMap::new(),
        });
        lineage.inputs.insert(
            art("fitness_report"),
            serde_json::to_vec(&previous).expect("fixture serializes"),
        );
        let report = CycleReport {
            cycle: 2,
            fitness: None,
            fitness_delta: None,
            lineage: vec![lineage],
            ..Default::default()
        };

        let selected = select_exportable_fitness_evidence(&report, None).expect("fresh evidence");

        assert_eq!(selected["cycle"], 1);
        assert_eq!(selected["schema_version"], "a2d.fitness-evidence.v1");
    }

    #[test]
    fn exportable_fitness_evidence_rejects_provider_fabricated_output_evidence() {
        let fabricated = complete_fitness_evidence(1);
        let mut lineage = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: BTreeMap::new(),
        });
        lineage.outputs.insert(
            art("fitness_report"),
            serde_json::to_vec(&fabricated).expect("fixture serializes"),
        );
        let report = CycleReport {
            cycle: 2,
            lineage: vec![lineage],
            ..Default::default()
        };

        assert!(select_exportable_fitness_evidence(&report, None).is_err());
    }

    #[test]
    fn exportable_fitness_evidence_rejects_feedback_cycle_without_consumed_fresh_evidence() {
        let stale = complete_fitness_evidence(0);
        let mut lineage = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: BTreeMap::new(),
        });
        lineage.inputs.insert(
            art("fitness_report"),
            serde_json::to_vec(&stale).expect("fixture serializes"),
        );
        let report = CycleReport {
            cycle: 2,
            lineage: vec![lineage],
            ..Default::default()
        };

        assert!(select_exportable_fitness_evidence(&report, None).is_err());
    }

    #[test]
    fn lineage_durability_gate_requires_actual_fitness_evidence() {
        let no_evidence = CycleReport {
            accepted_mutations: 1,
            fitness: None,
            fitness_delta: None,
            ..Default::default()
        };
        assert!(!report_has_actual_fitness_evidence(&no_evidence));

        let mut legacy_lineage = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: BTreeMap::new(),
        });
        legacy_lineage
            .inputs
            .insert(art("fitness_report"), b"fitness: 0.50".to_vec());
        let legacy_evidence = CycleReport {
            accepted_mutations: 1,
            lineage: vec![legacy_lineage],
            ..Default::default()
        };
        assert!(!report_has_actual_fitness_evidence(&legacy_evidence));

        let mut regressing_lineage = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: BTreeMap::new(),
        });
        regressing_lineage.inputs.insert(
            art("fitness_report"),
            br#"{"schema_version":"a2d.fitness-evidence.v1","actual_tests_evaluated":true,"cycle":1,"non_regressing":false,"delta_from_last_non_regressing_fitness":-0.1}"#.to_vec(),
        );
        let regressing_lineage_evidence = CycleReport {
            cycle: 2,
            accepted_mutations: 1,
            lineage: vec![regressing_lineage],
            ..Default::default()
        };
        assert!(!report_has_actual_fitness_evidence(
            &regressing_lineage_evidence
        ));

        let mut lineage = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: BTreeMap::new(),
        });
        lineage.inputs.insert(
            art("fitness_report"),
            br#"{"schema_version":"a2d.fitness-evidence.v1","actual_tests_evaluated":true,"cycle":1,"non_regressing":true,"delta_from_last_non_regressing_fitness":0.1}"#.to_vec(),
        );
        let input_evidence = CycleReport {
            cycle: 2,
            accepted_mutations: 1,
            lineage: vec![lineage],
            ..Default::default()
        };
        assert!(report_has_actual_fitness_evidence(&input_evidence));

        let mut stale_lineage = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: BTreeMap::new(),
        });
        stale_lineage.inputs.insert(
            art("fitness_report"),
            br#"{"schema_version":"a2d.fitness-evidence.v1","actual_tests_evaluated":true,"cycle":0,"non_regressing":true,"delta_from_last_non_regressing_fitness":0.1}"#.to_vec(),
        );
        let stale_evidence = CycleReport {
            cycle: 2,
            accepted_mutations: 1,
            lineage: vec![stale_lineage],
            ..Default::default()
        };
        assert!(!report_has_actual_fitness_evidence(&stale_evidence));

        let regressing_evidence = CycleReport {
            accepted_mutations: 1,
            fitness: Some(fitness(4, 6)),
            fitness_delta: Some(-0.16),
            ..Default::default()
        };
        assert!(!report_has_actual_fitness_evidence(&regressing_evidence));

        let evidence = CycleReport {
            accepted_mutations: 1,
            fitness: Some(fitness(5, 6)),
            fitness_delta: Some(0.83),
            ..Default::default()
        };
        assert!(report_has_actual_fitness_evidence(&evidence));
    }

    fn policy_summary(
        mode: TopologyMode,
        fitness_value: f64,
        invocations: usize,
        wall_secs: f64,
    ) -> TopologyRunSummary {
        let mut summary = TopologyRunSummary::new(mode, "sudoku", 1, 4);
        summary.best_fitness = fitness_value;
        summary.best_total = 6;
        summary.best_passed = (fitness_value * 6.0).round() as usize;
        summary.total_invocations = invocations;
        summary.elapsed_secs = wall_secs;
        summary
    }

    fn topology_entry(outcome: a2d_core::workcell::WorkcellOutcome) -> InvocationLineage {
        InvocationLineage {
            cycle: 1,
            workcell_id: a2d_core::workcell::WorkcellId("wc-test".to_string()),
            enzyme_id: EnzymeId::from("evolver"),
            provider: "opencode/kimi-for-coding/k2p6".to_string(),
            escalation_rung: 0,
            provider_swap: false,
            clean_session: false,
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            tool_events: Vec::new(),
            health: a2d_core::observer::observe(&[]),
            outcome,
            mutation: None,
            patch: None,
            provider_policy: None,
            candidate_evaluations: Vec::new(),
        }
    }

    #[test]
    fn topology_lineage_entry_formats_failure_on_one_line() {
        let entry = topology_entry(a2d_core::workcell::WorkcellOutcome::Failed {
            error: "model invocation failed:\nprovider timed out after 90s".to_string(),
        });

        assert_eq!(
            format_topology_lineage_entry(&entry),
            "    [evolver via opencode/kimi-for-coding/k2p6] FAIL: model invocation failed: provider timed out after 90s"
        );
    }

    #[test]
    fn topology_lineage_entry_truncates_long_errors() {
        let entry = topology_entry(a2d_core::workcell::WorkcellOutcome::Failed {
            error: "x".repeat(300),
        });
        let formatted = format_topology_lineage_entry(&entry);

        assert!(formatted.ends_with('…'));
        assert!(formatted.chars().count() < 320);
    }

    #[test]
    fn topology_lineage_entry_shows_escalation_rung_flags() {
        let mut entry = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: BTreeMap::new(),
        });
        entry.escalation_rung = 5;
        entry.provider_swap = true;
        entry.clean_session = true;

        assert_eq!(
            format_topology_lineage_entry(&entry),
            "    [evolver via opencode/kimi-for-coding/k2p6 {rung 5, swap, clean}] OK"
        );
    }

    #[test]
    fn escalation_validation_json_uses_marker_visibility_and_hides_internal_counter_names() {
        let marker = "seeded validation failure marker";
        let mut entry = topology_entry(a2d_core::workcell::WorkcellOutcome::Failed {
            error: "forced timeout".to_string(),
        });
        entry
            .inputs
            .insert(art("failure_report"), marker.as_bytes().to_vec());
        entry.escalation_rung = 4;
        entry.provider_swap = true;
        let report = CycleReport {
            invocations: 1,
            lineage: vec![entry],
            ..Default::default()
        };

        let value = escalation_validation_result_json(
            4,
            &EnzymeId::from("evolver"),
            None,
            Some(&report),
            marker,
            false,
        );
        let encoded = serde_json::to_string(&value).unwrap();

        assert_eq!(value["failure_report_marker_visible"], true);
        assert!(encoded.contains("escalation_rung"));
        assert!(!encoded.contains("loop_rung"));
        assert!(!encoded.contains("enzyme_loop_count"));
    }

    #[test]
    fn role_provider_comparison_args_parse_replicas_and_providers() {
        let args = vec![
            "--replicas".to_string(),
            "3".to_string(),
            "pi/minimax/MiniMax-M3".to_string(),
            "opencode/kimi-for-coding/k2p6".to_string(),
        ];

        let parsed = parse_role_provider_comparison_args(&args).unwrap();

        assert_eq!(parsed.replicas, 3);
        assert_eq!(
            parsed.providers,
            vec![
                "pi/minimax/MiniMax-M3".to_string(),
                "opencode/kimi-for-coding/k2p6".to_string(),
            ]
        );
    }

    #[test]
    fn role_provider_comparison_args_reject_zero_replicas() {
        let args = vec!["--replicas=0".to_string()];

        let error = parse_role_provider_comparison_args(&args).unwrap_err();

        assert!(error.contains("positive integer"));
    }

    #[test]
    fn role_provider_comparison_json_separates_assignment_from_outcome_success() {
        let mut entry = topology_entry(a2d_core::workcell::WorkcellOutcome::Failed {
            error: "provider timed out".to_string(),
        });
        entry.outputs.insert(
            art("system_patch"),
            br#"{"action":"noop","reason":"already optimal"}"#.to_vec(),
        );
        entry.patch = Some(a2d_core::metabolism::PatchRecord {
            noops: vec!["already optimal".to_string()],
            ..Default::default()
        });
        let report = CycleReport {
            invocations: 1,
            failed: 1,
            accepted_patches: 0,
            rejected_patches: 0,
            lineage: vec![entry],
            ..Default::default()
        };

        let value = role_provider_comparison_result_json(
            2,
            "opencode/zai-coding-plan/glm-5.1",
            "opencode/zai-coding-plan/glm-5.1",
            5000,
            &report,
        );

        assert_eq!(value["replica"], 2);
        assert_eq!(value["assignment_accepted"], true);
        assert_eq!(value["failed"], 1);
        assert!(value["outcome"].as_str().unwrap().contains("failed:"));
        assert_eq!(
            value["materialized_output_previews"]["system_patch"],
            r#"{"action":"noop","reason":"already optimal"}"#
        );
        assert_eq!(value["patch_record"]["noops"][0], "already optimal");
        assert_eq!(value["accepted_patches"], 0);
        assert_eq!(value["rejected_patches"], 0);
        assert!(value.get("accepted").is_none());
    }

    #[test]
    fn role_provider_comparison_summary_counts_replicated_provider_outcomes() {
        let mut success_entry = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: BTreeMap::new(),
        });
        success_entry
            .outputs
            .insert(art("test_results"), b"ok".to_vec());
        success_entry.patch = Some(a2d_core::metabolism::PatchRecord {
            noops: vec!["no source change needed".to_string()],
            ..Default::default()
        });
        let success_report = CycleReport {
            invocations: 1,
            accepted_patches: 1,
            lineage: vec![success_entry],
            ..Default::default()
        };
        let timeout_report = CycleReport {
            invocations: 1,
            failed: 1,
            lineage: vec![topology_entry(
                a2d_core::workcell::WorkcellOutcome::Failed {
                    error: "model invocation failed: provider timed out after 60s".to_string(),
                },
            )],
            ..Default::default()
        };
        let results = vec![
            role_provider_comparison_result_json(
                1,
                "provider-a",
                "provider-a",
                10,
                &success_report,
            ),
            role_provider_comparison_result_json(
                2,
                "provider-a",
                "provider-a",
                20,
                &timeout_report,
            ),
            json!({
                "replica": 1,
                "provider": "missing-provider",
                "assignment_accepted": false,
                "error": "provider is not registered",
            }),
        ];

        let summary = summarize_role_provider_comparison_results(&results);

        assert_eq!(summary["provider-a"]["attempts"], 2);
        assert_eq!(summary["provider-a"]["assignment_accepted"], 2);
        assert_eq!(summary["provider-a"]["successes"], 1);
        assert_eq!(summary["provider-a"]["failures"], 1);
        assert_eq!(summary["provider-a"]["timed_out"], 1);
        assert_eq!(summary["provider-a"]["materialized_output_runs"], 1);
        assert_eq!(summary["provider-a"]["accepted_patches"], 1);
        assert_eq!(summary["provider-a"]["noop_patches"], 1);
        assert_eq!(summary["provider-a"]["elapsed_ms"]["min"], 10);
        assert_eq!(summary["provider-a"]["elapsed_ms"]["max"], 20);
        assert_eq!(summary["provider-a"]["elapsed_ms"]["mean"], 15);
        assert_eq!(summary["missing-provider"]["attempts"], 1);
        assert_eq!(summary["missing-provider"]["assignment_rejected"], 1);
        assert!(summary["missing-provider"]["elapsed_ms"].is_null());
    }

    #[test]
    fn escalation_validation_json_does_not_treat_empty_food_failure_report_as_visible() {
        let marker = "seeded validation failure marker";
        let mut entry = topology_entry(a2d_core::workcell::WorkcellOutcome::Failed {
            error: "forced timeout".to_string(),
        });
        entry.inputs.insert(art("failure_report"), Vec::new());
        entry.escalation_rung = 5;
        entry.provider_swap = true;
        entry.clean_session = true;
        let report = CycleReport {
            invocations: 1,
            lineage: vec![entry],
            ..Default::default()
        };

        let value = escalation_validation_result_json(
            5,
            &EnzymeId::from("evolver"),
            None,
            Some(&report),
            marker,
            false,
        );

        assert_eq!(value["failure_report_marker_visible"], false);
    }

    #[test]
    fn provider_policy_gate_rejects_worse_fitness_and_withholds_lineage() {
        let current = policy_summary(TopologyMode::CurrentPolicy, 0.83, 4, 20.0);
        let proposed = policy_summary(TopologyMode::ProposedPolicy, 0.67, 4, 20.0);
        let decision = decide_provider_policy_gate(&current, &proposed);
        let root = std::env::temp_dir().join(format!(
            "a2d-provider-policy-gate-reject-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let archive = LineageArchive::init(&root).unwrap();
        let policy = ProviderPolicy {
            assignments: BTreeMap::from([(
                "coder".to_string(),
                "opencode/kimi-for-coding/k2p6".to_string(),
            )]),
        };
        let report = CycleReport {
            accepted_provider_policy_changes: 1,
            ..Default::default()
        };

        let committed =
            commit_provider_policy_if_gate_accepts(&archive, &policy, &report, &decision).unwrap();

        assert!(!decision.accepted);
        assert!(committed.is_none());
        assert!(archive.read_provider_policy().is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn provider_policy_gate_persists_clearly_better_policy() {
        let current = policy_summary(TopologyMode::CurrentPolicy, 0.67, 4, 20.0);
        let proposed = policy_summary(TopologyMode::ProposedPolicy, 0.83, 4, 21.0);
        let decision = decide_provider_policy_gate(&current, &proposed);
        let root = std::env::temp_dir().join(format!(
            "a2d-provider-policy-gate-accept-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let archive = LineageArchive::init(&root).unwrap();
        let policy = ProviderPolicy {
            assignments: BTreeMap::from([(
                "tester".to_string(),
                "opencode/zai-coding-plan/glm-5.1".to_string(),
            )]),
        };
        let report = CycleReport {
            accepted_provider_policy_changes: 1,
            ..Default::default()
        };

        let committed =
            commit_provider_policy_if_gate_accepts(&archive, &policy, &report, &decision).unwrap();

        assert!(decision.accepted, "{}", decision.reason);
        assert!(committed.is_some());
        assert_eq!(archive.read_provider_policy().unwrap(), policy);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn provider_policy_deltas_name_current_and_proposed_assignments() {
        let current = ProviderPolicy {
            assignments: BTreeMap::from([("coder".to_string(), "old".to_string())]),
        };
        let proposed = ProviderPolicy {
            assignments: BTreeMap::from([("coder".to_string(), "new".to_string())]),
        };

        let deltas = provider_policy_deltas(&current, &proposed);

        assert_eq!(deltas, vec!["coder: old -> new"]);
    }

    #[test]
    fn force_seed_germline_accepts_explicit_seed_modes() {
        assert!(force_seed_germline(Some("seed")));
        assert!(force_seed_germline(Some("baseline")));
        assert!(force_seed_germline(Some("4")));
        assert!(force_seed_germline(Some("1")));
        assert!(!force_seed_germline(Some("lineage")));
        assert!(!force_seed_germline(None));
    }

    #[test]
    fn escalation_validation_germline_isolates_requested_enzyme() {
        let germline = seed_germline();
        let isolated = validation_germline_for_enzyme(germline, &EnzymeId::from("architect"));
        let ids = isolated
            .enzymes()
            .into_iter()
            .map(|enzyme| enzyme.id.0.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["architect"]);
        assert!(isolated.food().contains(&art("failure_report")));
        assert!(isolated.food().contains(&art("fitness_report")));
    }

    #[test]
    fn live_registry_keeps_glm_off_coder_and_evolver_critical_path() {
        let registry = build_registry();

        assert_eq!(
            registry.provider_for(&EnzymeId::from("coder")).name(),
            "opencode/kimi-for-coding/k2p6"
        );
        assert_eq!(
            registry.provider_for(&EnzymeId::from("evolver")).name(),
            "opencode/kimi-for-coding/k2p6"
        );
        assert_eq!(
            registry.provider_for(&EnzymeId::from("tester")).name(),
            "opencode/zai-coding-plan/glm-5.1"
        );
        assert_eq!(
            registry.provider_for(&EnzymeId::from("architect")).name(),
            "opencode/zai-coding-plan/glm-5.1"
        );
        assert_eq!(
            registry.provider_for(&EnzymeId::from("maintainer")).name(),
            "pi/default"
        );
        assert!(
            registry
                .providers()
                .contains(&"opencode/opencode/deepseek-v4-flash-free")
        );
        assert!(registry.providers().contains(&"pi/default"));
    }

    #[test]
    fn live_registry_keeps_new_model_lanes_opt_in() {
        let registry = build_registry();
        let providers = registry.providers();

        assert!(!providers.contains(&"opencode/kimi-for-coding/k2p7"));
        assert!(!providers.contains(&"opencode/kimi-k2.7-code"));
        assert!(!providers.contains(&"opencode/zai-coding-plan/glm-5.2"));
        assert!(!providers.contains(&"opencode/minimax-coding-plan/MiniMax-3"));
        assert!(!providers.contains(&"pi/kimi-coding/k2p7"));
        assert!(!providers.contains(&"pi/minimax/MiniMax-M3"));
        assert!(!providers.contains(&"pi/zai/glm-5.2"));
    }

    #[test]
    fn seed_mode_runtime_registry_still_applies_explicit_overrides() {
        let germline = seed_germline();
        let registry = build_runtime_registry_with_options(
            &germline,
            true,
            BTreeMap::from([
                (
                    "tester".to_string(),
                    Some("pi/minimax/MiniMax-M3".to_string()),
                ),
                (
                    "architect".to_string(),
                    Some("pi/minimax/MiniMax-M3".to_string()),
                ),
            ]),
        );

        assert_eq!(
            registry.provider_for(&EnzymeId::from("tester")).name(),
            "pi/minimax/MiniMax-M3"
        );
        assert_eq!(
            registry.provider_for(&EnzymeId::from("architect")).name(),
            "pi/minimax/MiniMax-M3"
        );
    }

    #[test]
    fn topology_seed_registry_path_still_applies_explicit_overrides() {
        let germline = seed_germline();
        let registry = build_registry_for_topology_with_overrides(
            &germline,
            TopologyMode::Seed,
            BTreeMap::from([(
                "architect".to_string(),
                Some("pi/minimax/MiniMax-M3".to_string()),
            )]),
        );

        assert_eq!(
            registry.provider_for(&EnzymeId::from("architect")).name(),
            "pi/minimax/MiniMax-M3"
        );
    }

    #[test]
    fn runtime_provider_overrides_auto_register_known_experimental_lanes() {
        let mut registry = build_registry();
        let application = apply_runtime_provider_overrides(
            &mut registry,
            BTreeMap::from([
                (
                    "tester".to_string(),
                    Some("opencode/kimi-for-coding/k2p7".to_string()),
                ),
                (
                    "architect".to_string(),
                    Some("pi/kimi-coding/k2p7".to_string()),
                ),
            ]),
        );

        assert_eq!(application.accepted.len(), 2);
        assert!(application.rejected.is_empty());
        assert_eq!(
            registry.provider_for(&EnzymeId::from("tester")).name(),
            "opencode/kimi-for-coding/k2p7"
        );
        assert_eq!(
            registry.provider_for(&EnzymeId::from("architect")).name(),
            "pi/kimi-coding/k2p7"
        );
        assert!(
            registry
                .providers()
                .contains(&"opencode/kimi-for-coding/k2p7")
        );
        assert!(!register_experimental_provider_if_known(
            &mut registry,
            "opencode/kimi-k2.7-code"
        ));
        assert!(registry.providers().contains(&"pi/kimi-coding/k2p7"));
    }

    #[test]
    fn experimental_pi_lanes_register_only_when_named() {
        let mut registry = build_registry();

        assert!(!registry.providers().contains(&"pi/kimi-coding/k2p7"));
        assert!(!registry.providers().contains(&"pi/minimax/MiniMax-M3"));
        assert!(!registry.providers().contains(&"pi/zai/glm-5.2"));

        assert!(register_experimental_provider_if_known(
            &mut registry,
            "pi/kimi-coding/k2p7"
        ));
        assert!(register_experimental_provider_if_known(
            &mut registry,
            "pi/minimax/MiniMax-M3"
        ));
        assert!(register_experimental_provider_if_known(
            &mut registry,
            "pi/zai/glm-5.2"
        ));

        assert!(registry.providers().contains(&"pi/kimi-coding/k2p7"));
        assert!(registry.providers().contains(&"pi/minimax/MiniMax-M3"));
        assert!(registry.providers().contains(&"pi/zai/glm-5.2"));
        assert!(!register_experimental_provider_if_known(
            &mut registry,
            "pi/kimi-coding/not-verified"
        ));
    }

    #[test]
    fn experimental_minimax_3_alias_registers_only_when_named() {
        let mut registry = build_registry();

        assert!(register_experimental_provider_if_known(
            &mut registry,
            "opencode/minimax-coding-plan/MiniMax-3"
        ));

        assert!(
            registry
                .providers()
                .contains(&"opencode/minimax-coding-plan/MiniMax-3")
        );
    }

    #[test]
    fn runtime_provider_overrides_reassign_tester_and_architect_without_env_mutation() {
        let mut registry = build_registry();
        let application = apply_runtime_provider_overrides(
            &mut registry,
            BTreeMap::from([
                (
                    "tester".to_string(),
                    Some("opencode/kimi-for-coding/k2p6".to_string()),
                ),
                (
                    "architect".to_string(),
                    Some("opencode/opencode/deepseek-v4-flash-free".to_string()),
                ),
            ]),
        );

        assert_eq!(application.accepted.len(), 2);
        assert!(application.rejected.is_empty());
        assert_eq!(
            registry.provider_for(&EnzymeId::from("tester")).name(),
            "opencode/kimi-for-coding/k2p6"
        );
        assert_eq!(
            registry.provider_for(&EnzymeId::from("architect")).name(),
            "opencode/opencode/deepseek-v4-flash-free"
        );
    }

    #[test]
    fn runtime_provider_overrides_reject_unknown_provider_and_preserve_default() {
        let mut registry = build_registry();
        let application = apply_runtime_provider_overrides(
            &mut registry,
            BTreeMap::from([("tester".to_string(), Some("missing".to_string()))]),
        );

        assert!(application.accepted.is_empty());
        assert_eq!(application.rejected.len(), 1);
        assert_eq!(application.rejected[0].reason, "provider is not registered");
        assert_eq!(
            registry.provider_for(&EnzymeId::from("tester")).name(),
            "opencode/zai-coding-plan/glm-5.1"
        );
    }

    #[test]
    fn runtime_provider_overrides_ignore_non_experimental_roles() {
        let mut registry = build_registry();
        let application = apply_runtime_provider_overrides(
            &mut registry,
            BTreeMap::from([(
                "coder".to_string(),
                Some("opencode/opencode/deepseek-v4-flash-free".to_string()),
            )]),
        );

        assert!(application.accepted.is_empty());
        assert_eq!(application.rejected.len(), 1);
        assert_eq!(
            application.rejected[0].reason,
            "target enzyme is not in the current germline"
        );
        assert_eq!(
            registry.provider_for(&EnzymeId::from("coder")).name(),
            "opencode/kimi-for-coding/k2p6"
        );
    }

    #[test]
    fn autopilot_repair_escalates_first_repair_attempt_to_alternate_provider() {
        let registry = build_registry();
        let maintainer = EnzymeId::from("maintainer");

        let initial = autopilot_provider_for_attempt(&registry, &maintainer, 0, None);
        let first_repair = autopilot_provider_for_attempt(&registry, &maintainer, 1, None);
        let later_repair = autopilot_provider_for_attempt(&registry, &maintainer, 2, None);

        assert_eq!(initial.provider.name(), "pi/default");
        assert!(!initial.escalated);
        assert_eq!(
            first_repair.provider.name(),
            "opencode/kimi-for-coding/k2p6"
        );
        assert!(first_repair.escalated);
        assert_eq!(first_repair.primary_provider, "pi/default");
        assert_eq!(later_repair.provider.name(), "pi/default");
        assert!(!later_repair.escalated);
        assert!(
            first_repair
                .metadata_text(1)
                .contains("registered_providers")
        );
    }

    #[test]
    fn autopilot_repair_uses_configured_registered_provider() {
        let registry = build_registry();
        let maintainer = EnzymeId::from("maintainer");

        let first_repair = autopilot_provider_for_attempt(
            &registry,
            &maintainer,
            1,
            Some("opencode/opencode/deepseek-v4-flash-free"),
        );
        let missing = autopilot_provider_for_attempt(&registry, &maintainer, 1, Some("missing"));

        assert_eq!(
            first_repair.provider.name(),
            "opencode/opencode/deepseek-v4-flash-free"
        );
        assert!(first_repair.escalated);
        assert!(
            first_repair
                .escalation_reason
                .contains("configured repair provider")
        );
        assert_eq!(missing.provider.name(), "opencode/kimi-for-coding/k2p6");
    }

    #[test]
    fn loaded_provider_policy_applies_to_registered_known_enzyme() {
        let germline = seed_germline();
        let mut registry = build_registry();
        let policy = ProviderPolicy {
            assignments: BTreeMap::from([(
                "tester".to_string(),
                "opencode/kimi-for-coding/k2p6".to_string(),
            )]),
        };

        let application = apply_loaded_provider_policy(&mut registry, &germline, &policy);

        assert_eq!(application.accepted.len(), 1);
        assert!(application.rejected.is_empty());
        assert_eq!(
            registry.provider_for(&EnzymeId::from("tester")).name(),
            "opencode/kimi-for-coding/k2p6"
        );
    }

    #[test]
    fn loaded_provider_policy_auto_registers_known_experimental_lane() {
        let germline = seed_germline();
        let mut registry = build_registry();
        let policy = ProviderPolicy {
            assignments: BTreeMap::from([(
                "tester".to_string(),
                "opencode/zai-coding-plan/glm-5.2".to_string(),
            )]),
        };

        let application = apply_loaded_provider_policy(&mut registry, &germline, &policy);

        assert_eq!(application.accepted.len(), 1);
        assert!(application.rejected.is_empty());
        assert_eq!(
            registry.provider_for(&EnzymeId::from("tester")).name(),
            "opencode/zai-coding-plan/glm-5.2"
        );
    }

    #[test]
    fn seed_germline_coder_consumes_design_plan_and_requirements() {
        let germline = seed_germline();
        let coder = germline.get_enzyme(&EnzymeId::from("coder")).unwrap();
        let architect = germline.get_enzyme(&EnzymeId::from("architect")).unwrap();

        assert!(coder.reactants.contains(&art("design")));
        assert!(coder.reactants.contains(&art("plan")));
        assert!(coder.reactants.contains(&art("requirements")));
        assert!(germline.food().contains(&art("design")));
        assert!(germline.food().contains(&art("plan")));
        assert!(germline.food().contains(&art("requirements")));
        assert!(germline.food().contains(&art("provider_health_report")));
        assert!(germline.food().contains(&art("provider_policy")));
        assert!(architect.catalysts.contains(&art("provider_health_report")));
        assert!(architect.catalysts.contains(&art("provider_policy")));
        assert!(
            coder
                .prompt_template
                .as_deref()
                .unwrap()
                .contains("Benchmark integrity rule")
        );
        assert!(
            coder
                .prompt_template
                .as_deref()
                .unwrap()
                .contains("not to search GitHub")
        );
        assert!(
            coder
                .prompt_template
                .as_deref()
                .unwrap()
                .contains("unified diff candidate patch")
        );
        assert!(
            coder
                .prompt_template
                .as_deref()
                .unwrap()
                .contains("Default deliverable")
        );
        assert!(
            coder
                .prompt_template
                .as_deref()
                .unwrap()
                .contains("without markdown fences")
        );
        assert!(
            coder
                .prompt_template
                .as_deref()
                .unwrap()
                .contains("mechanical `has_tests` fitness gate")
        );
        assert!(
            coder
                .prompt_template
                .as_deref()
                .unwrap()
                .contains("normal path, at least one edge/invalid input")
        );
    }

    #[test]
    fn normalize_loaded_enzymes_upgrades_legacy_coder_contract() {
        let legacy = EnzymeDef {
            id: EnzymeId::from("coder"),
            reactants: BTreeSet::from([art("requirements")]),
            products: BTreeSet::from([art("code")]),
            catalysts: BTreeSet::new(),
            prompt_template: Some("Given requirements, write code.".to_string()),
        };

        let normalized = normalize_loaded_enzymes(vec![legacy]);
        let coder = normalized
            .into_iter()
            .find(|enzyme| enzyme.id == EnzymeId::from("coder"))
            .unwrap();

        assert_eq!(
            coder.reactants,
            BTreeSet::from([art("design"), art("plan"), art("requirements")])
        );
        assert!(coder.catalysts.contains(&art("enzyme_defs")));
        assert!(coder.prompt_template.as_deref().unwrap().contains("design"));
        assert!(
            coder
                .prompt_template
                .as_deref()
                .unwrap()
                .contains("Benchmark integrity rule")
        );
    }

    #[test]
    fn normalize_loaded_enzymes_preserves_evolved_coder_prompt_when_adding_integrity_rule() {
        let evolved = EnzymeDef {
            id: EnzymeId::from("coder"),
            reactants: BTreeSet::from([art("design"), art("plan"), art("requirements")]),
            products: BTreeSet::from([art("code")]),
            catalysts: BTreeSet::from([art("enzyme_defs")]),
            prompt_template: Some(
                "Use design, plan, and requirements. Preserve this evolved hint.".to_string(),
            ),
        };

        let normalized = normalize_loaded_enzymes(vec![evolved]);
        let coder = normalized
            .into_iter()
            .find(|enzyme| enzyme.id == EnzymeId::from("coder"))
            .unwrap();
        let prompt = coder.prompt_template.as_deref().unwrap();

        assert!(prompt.contains("Preserve this evolved hint"));
        assert!(prompt.contains("Benchmark integrity rule"));
        assert!(prompt.contains("not to search GitHub"));
    }

    #[test]
    fn topology_summary_tracks_best_fitness_and_full_cycle() {
        let mut summary = TopologyRunSummary::new(TopologyMode::Seed, "sudoku", 3, 4);

        let first = CycleReport {
            invocations: 2,
            failed: 1,
            killed: 1,
            accepted_mutations: 1,
            wall_clock_capped: true,
            fitness: Some(fitness(0, 6)),
            ..Default::default()
        };
        summary.record_cycle(1, &first);

        let second = CycleReport {
            invocations: 3,
            fitness: Some(fitness(6, 6)),
            ..Default::default()
        };
        summary.record_cycle(2, &second);

        assert_eq!(summary.total_invocations, 5);
        assert_eq!(summary.provider_failures, 1);
        assert_eq!(summary.killed, 1);
        assert_eq!(summary.total_mutations, 1);
        assert_eq!(summary.wall_clock_capped_cycles, 1);
        assert_eq!(summary.best_passed, 6);
        assert_eq!(summary.best_total, 6);
        assert_eq!(summary.cycles_to_full_fitness, Some(2));
    }

    #[test]
    fn topology_summary_preserves_zero_fitness_denominator() {
        let mut summary = TopologyRunSummary::new(TopologyMode::Evolved, "sudoku", 1, 7);
        let report = CycleReport {
            fitness: Some(fitness(0, 6)),
            ..Default::default()
        };

        summary.record_cycle(1, &report);

        assert_eq!(summary.best_fitness, 0.0);
        assert_eq!(summary.best_passed, 0);
        assert_eq!(summary.best_total, 6);
        assert_eq!(summary.cycles_to_full_fitness, None);
    }

    #[test]
    fn normalize_loaded_enzymes_upgrades_evolver_to_mechanical_fitness() {
        let legacy = EnzymeDef {
            id: EnzymeId::from("evolver"),
            reactants: BTreeSet::from([art("test_results")]),
            products: BTreeSet::from([art("enzyme_defs")]),
            catalysts: BTreeSet::from([art("test_results")]),
            prompt_template: None,
        };

        let normalized = normalize_loaded_enzymes(vec![legacy]);
        let evolver = normalized
            .into_iter()
            .find(|enzyme| enzyme.id == EnzymeId::from("evolver"))
            .unwrap();

        assert_eq!(evolver.reactants, BTreeSet::from([art("fitness_report")]));
        assert!(!evolver.catalysts.contains(&art("test_results")));
        assert!(evolver.catalysts.contains(&art("enzyme_defs")));
        assert!(evolver.catalysts.contains(&art("failure_report")));
        assert!(evolver.catalysts.contains(&art("fitness_report")));
        assert!(evolver.catalysts.contains(&art("provider_health_report")));
        assert!(evolver.catalysts.contains(&art("provider_policy")));
    }

    #[test]
    fn parses_json_artifact_bundle_and_preserves_payloads() {
        let input = r#"{
            "design": "DESIGN",
            "plan": "PLAN",
            "requirements": "REQS",
            "enzyme_defs": [{"id":"coder"}]
        }"#;

        let artifacts = input_artifacts_from_request(input);

        assert_eq!(artifacts.get(&art("design")).unwrap(), b"DESIGN");
        assert_eq!(artifacts.get(&art("plan")).unwrap(), b"PLAN");
        assert_eq!(artifacts.get(&art("requirements")).unwrap(), b"REQS");
        assert_eq!(
            artifacts.get(&art("enzyme_defs")).unwrap(),
            br#"[{"id":"coder"}]"#
        );
    }

    #[test]
    fn cycle_input_args_parse_path_and_positive_cycle_count() {
        let config = parse_cycle_input_args(&["task-cycle-input.json".to_string()])
            .expect("default cycle-input args parse");
        assert_eq!(config.path, "task-cycle-input.json");
        assert_eq!(config.num_cycles, 1);
        assert_eq!(config.output_artifacts, None);
        assert_eq!(config.checkout, None);

        let config = parse_cycle_input_args(&["-".to_string(), "2".to_string()])
            .expect("stdin cycle-input args parse");
        assert_eq!(config.path, "-");
        assert_eq!(config.num_cycles, 2);
        assert_eq!(config.output_artifacts, None);
        assert_eq!(config.checkout, None);

        let config = parse_cycle_input_args(&[
            "task-cycle-input.json".to_string(),
            "2".to_string(),
            "--output-artifacts".to_string(),
            "runs/out".to_string(),
        ])
        .expect("cycle-input output artifacts parse");
        assert_eq!(config.num_cycles, 2);
        assert_eq!(config.output_artifacts, Some(PathBuf::from("runs/out")));
        assert_eq!(config.checkout, None);

        let config = parse_cycle_input_args(&[
            "task-cycle-input.json".to_string(),
            "--output-artifacts".to_string(),
            "runs/out".to_string(),
            "2".to_string(),
        ])
        .expect("cycle-input output artifacts parse before cycle count");
        assert_eq!(config.num_cycles, 2);
        assert_eq!(config.output_artifacts, Some(PathBuf::from("runs/out")));
        assert_eq!(config.checkout, None);

        let config = parse_cycle_input_args(&[
            "task-cycle-input.json".to_string(),
            "--checkout".to_string(),
            "runs/checkout".to_string(),
            "--output-artifacts".to_string(),
            "runs/out".to_string(),
            "2".to_string(),
        ])
        .expect("cycle-input checkout parse");
        assert_eq!(config.num_cycles, 2);
        assert_eq!(config.checkout, Some(PathBuf::from("runs/checkout")));
        assert_eq!(config.output_artifacts, Some(PathBuf::from("runs/out")));

        assert!(
            parse_cycle_input_args(&[])
                .expect_err("cycle-input requires an artifact path")
                .contains("missing cycle input path")
        );
        assert!(
            parse_cycle_input_args(&["input.json".to_string(), "0".to_string()])
                .expect_err("cycle-input rejects zero cycles")
                .contains("greater than zero")
        );
        assert!(
            parse_cycle_input_args(&[
                "input.json".to_string(),
                "1".to_string(),
                "extra".to_string()
            ])
            .expect_err("cycle-input rejects extra args")
            .contains("unknown cycle-input argument")
        );
        assert!(
            parse_cycle_input_args(&["input.json".to_string(), "--output-artifacts".to_string()])
                .expect_err("cycle-input requires output directory")
                .contains("requires a directory")
        );
        assert!(
            parse_cycle_input_args(&[
                "input.json".to_string(),
                "--output-artifacts".to_string(),
                "runs/a".to_string(),
                "--output-artifacts".to_string(),
                "runs/b".to_string(),
            ])
            .expect_err("cycle-input rejects duplicate output-artifacts")
            .contains("duplicate --output-artifacts")
        );
        assert!(
            parse_cycle_input_args(&["input.json".to_string(), "--checkout".to_string()])
                .expect_err("cycle-input requires checkout directory")
                .contains("requires a directory")
        );
        assert!(
            parse_cycle_input_args(&[
                "input.json".to_string(),
                "--checkout".to_string(),
                "a".to_string(),
                "--checkout".to_string(),
                "b".to_string(),
            ])
            .expect_err("cycle-input rejects duplicate checkout")
            .contains("duplicate --checkout")
        );
    }

    #[test]
    fn cycle_input_checkout_context_enriches_design_without_trusting_bundle() {
        let root = env::temp_dir().join(format!("a2d-cycle-input-checkout-{}", unique_suffix()));
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        fs::write(src.join("lib.rs"), "pub fn answer() -> i32 { 41 }\n").unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/config"), "must not leak").unwrap();
        fs::write(root.join("secrets.yaml"), "api_key: must not leak").unwrap();
        fs::write(root.join(".env"), "TOKEN=must not leak").unwrap();
        fs::write(
            src.join("credentials.json"),
            "{\"token\":\"must not leak\"}",
        )
        .unwrap();

        let input = r#"{"requirements":"fix from local checkout only","design":"Use local tests.","plan":"Return diff."}"#;
        let enriched = enrich_cycle_input_with_checkout(input, &root).expect("checkout enrichment");
        let value: Value = serde_json::from_str(&enriched).unwrap();
        let design = value.get("design").and_then(Value::as_str).unwrap();
        assert!(design.contains("BENCHMARK CHECKOUT CONTEXT"));
        assert!(design.contains("no-tools/artifact-only"));
        assert!(design.contains("You cannot run ls, cat, find, grep"));
        assert!(design.contains("return only a unified diff candidate patch"));
        assert!(design.contains("src/lib.rs"));
        assert!(design.contains("pub fn answer"));
        assert!(!design.contains("must not leak"));
        assert!(!design.contains("secrets.yaml"));
        assert!(!design.contains("credentials.json"));
        assert!(!design.contains("checkout: /"));
        assert!(
            value
                .get(BENCHMARK_CHECKOUT_CONTEXT_ARTIFACT)
                .and_then(Value::as_str)
                .unwrap()
                .contains("a2d.benchmark-checkout-context.v1")
        );
        let artifacts = input_artifacts_from_request(&enriched);
        assert!(
            String::from_utf8_lossy(artifacts.get(&art("design")).unwrap())
                .contains("pub fn answer")
        );
        assert!(
            artifacts
                .get(&art(BENCHMARK_CHECKOUT_CONTEXT_ARTIFACT))
                .is_some()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn cycle_input_checkout_context_rejects_root_symlink_and_skips_file_symlinks() {
        use std::os::unix::fs::symlink;

        let root = env::temp_dir().join(format!(
            "a2d-cycle-input-symlink-checkout-{}",
            unique_suffix()
        ));
        let outside = env::temp_dir().join(format!(
            "a2d-cycle-input-symlink-outside-{}",
            unique_suffix()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(root.join("README.md"), "local checkout context\n").unwrap();
        fs::write(outside.join("secret.rs"), "must not leak\n").unwrap();
        symlink(outside.join("secret.rs"), root.join("linked.rs")).unwrap();
        let context = build_benchmark_checkout_context(&root).expect("context skips file symlink");
        assert!(context.contains("README.md"));
        assert!(!context.contains("linked.rs"));
        assert!(!context.contains("must not leak"));

        let root_link =
            env::temp_dir().join(format!("a2d-cycle-input-root-link-{}", unique_suffix()));
        symlink(&root, &root_link).unwrap();
        assert!(
            build_benchmark_checkout_context(&root_link)
                .expect_err("root symlink rejected")
                .contains("must not be a symlink")
        );
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
        let _ = fs::remove_file(&root_link);
    }

    #[test]
    fn cycle_input_checkout_context_rejects_missing_empty_and_spoofed_context() {
        let root = env::temp_dir().join(format!(
            "a2d-cycle-input-empty-checkout-{}",
            unique_suffix()
        ));
        fs::create_dir_all(&root).unwrap();
        assert!(
            enrich_cycle_input_with_checkout(r#"{"requirements":"x"}"#, &root)
                .expect_err("empty checkout rejected")
                .contains("no bounded UTF-8")
        );
        assert!(
            enrich_cycle_input_with_checkout(r#"{"requirements":"x"}"#, &root.join("missing"))
                .expect_err("missing checkout rejected")
                .contains("failed to read checkout")
        );
        assert!(
            validate_cycle_input_bundle(
                r#"{"requirements":"x","benchmark_checkout_context":"fake"}"#
            )
            .expect_err("user-supplied checkout context is reserved")
            .contains(BENCHMARK_CHECKOUT_CONTEXT_ARTIFACT)
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cycle_output_artifact_export_writes_cumulative_manifest_and_outputs() {
        let mut outputs = BTreeMap::new();
        outputs.insert(art("code"), b"diff --git a/lib.rs b/lib.rs\n".to_vec());
        let mut first_entry = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: outputs.clone(),
        });
        first_entry.enzyme_id = EnzymeId::from("coder");
        first_entry.outputs = outputs.clone();
        let first_report = CycleReport {
            cycle: 1,
            lineage: vec![first_entry],
            ..Default::default()
        };
        let mut second_entry = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: outputs.clone(),
        });
        second_entry.cycle = 2;
        second_entry.workcell_id = a2d_core::workcell::WorkcellId("wc-0002".to_string());
        second_entry.enzyme_id = EnzymeId::from("coder");
        second_entry.outputs = outputs;
        let second_report = CycleReport {
            cycle: 2,
            lineage: vec![second_entry],
            ..Default::default()
        };
        let dir = env::temp_dir().join(format!(
            "a2d-cycle-output-artifacts-test-{}",
            unique_suffix()
        ));
        let mut records = Vec::new();
        let mut reserved_paths = BTreeSet::new();

        let first_paths =
            export_cycle_output_artifacts(&first_report, &dir, &mut records, &mut reserved_paths)
                .expect("first output artifact export");
        let second_paths =
            export_cycle_output_artifacts(&second_report, &dir, &mut records, &mut reserved_paths)
                .expect("second output artifact export");
        write_cycle_output_artifact_manifest(&dir, &records).expect("manifest write");

        assert_eq!(first_paths.len() + second_paths.len(), 2);
        assert!(first_paths[0].exists());
        assert!(second_paths[0].exists());
        assert_eq!(
            fs::read(&first_paths[0]).unwrap(),
            b"diff --git a/lib.rs b/lib.rs\n"
        );
        let manifest: Value =
            serde_json::from_slice(&fs::read(dir.join("manifest.json")).expect("manifest exists"))
                .expect("manifest is JSON");
        assert_eq!(manifest["schema_version"], "a2d.cycle-output-artifacts.v1");
        assert_eq!(manifest["artifacts"].as_array().unwrap().len(), 2);
        assert_eq!(manifest["artifacts"][0]["artifact_type"], "code");
        assert_eq!(manifest["artifacts"][1]["report_cycle"], 2);
        assert_eq!(manifest["artifacts"][0]["enzyme_id"], "coder");
        assert_eq!(
            manifest["artifacts"][0]["git_object_hash"],
            git_hash_object_bytes(b"diff --git a/lib.rs b/lib.rs\n").unwrap()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cycle_output_artifact_export_rejects_collisions_and_existing_manifest() {
        let mut outputs = BTreeMap::new();
        outputs.insert(art("code"), b"candidate".to_vec());
        let mut entry = topology_entry(a2d_core::workcell::WorkcellOutcome::Success {
            outputs: outputs.clone(),
        });
        entry.enzyme_id = EnzymeId::from("coder");
        entry.outputs = outputs;
        let report = CycleReport {
            cycle: 1,
            lineage: vec![entry],
            ..Default::default()
        };
        let dir = env::temp_dir().join(format!(
            "a2d-cycle-output-artifacts-collision-test-{}",
            unique_suffix()
        ));
        let mut records = Vec::new();
        let mut reserved_paths = BTreeSet::new();
        export_cycle_output_artifacts(&report, &dir, &mut records, &mut reserved_paths)
            .expect("first export succeeds");
        assert!(
            export_cycle_output_artifacts(&report, &dir, &mut records, &mut reserved_paths)
                .expect_err("second export rejects collision")
                .contains("collides")
        );
        write_cycle_output_artifact_manifest(&dir, &records).expect("manifest write");
        assert!(
            write_cycle_output_artifact_manifest(&dir, &records)
                .expect_err("manifest overwrite rejected")
                .contains("already exists")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cycle_input_bundle_validation_rejects_reserved_runtime_artifacts() {
        validate_cycle_input_bundle(r#"{"requirements":"REQS","design":"DESIGN","plan":"PLAN"}"#)
            .expect("ordinary cycle input artifacts are allowed");

        assert!(
            validate_cycle_input_bundle("not json")
                .expect_err("cycle-input requires JSON")
                .contains("JSON object")
        );
        assert!(
            validate_cycle_input_bundle(r#"["not", "object"]"#)
                .expect_err("cycle-input requires object")
                .contains("JSON object")
        );
        assert!(
            validate_cycle_input_bundle(r#"{"requirements":"REQS","fitness_report":{}}"#)
                .expect_err("cycle-input rejects reserved runtime evidence")
                .contains("reserved runtime artifact")
        );
        assert!(
            validate_cycle_input_bundle(r#"{"requirements":"REQS","failure_report":"fake"}"#)
                .expect_err("cycle-input rejects reserved failure reports")
                .contains("failure_report")
        );
    }

    #[test]
    fn senior_swe_bench_cycle_input_can_seed_cycle_artifacts() {
        let input = json!({
            "requirements": "Senior SWE-Bench policy: Do not search GitHub. Deliverable: produce a unified diff candidate patch.",
            "design": "Use the local checkout and local tests only.",
            "plan": "Return only a unified diff candidate patch.",
            "benchmark_context": {"task_id": "task-hard", "repo": "owner/repo"},
            "evaluation": {"status": "not_evaluated", "fitness": null}
        })
        .to_string();

        let artifacts = input_artifacts_from_request(&input);

        assert!(
            String::from_utf8_lossy(artifacts.get(&art("requirements")).unwrap())
                .contains("unified diff candidate patch")
        );
        assert_eq!(
            artifacts.get(&art("design")).unwrap(),
            b"Use the local checkout and local tests only."
        );
        assert!(
            String::from_utf8_lossy(artifacts.get(&art("plan")).unwrap())
                .contains("Return only a unified diff")
        );
        assert!(
            String::from_utf8_lossy(artifacts.get(&art("benchmark_context")).unwrap())
                .contains("task-hard")
        );
        assert!(
            String::from_utf8_lossy(artifacts.get(&art("evaluation")).unwrap())
                .contains("not_evaluated")
        );
    }

    #[test]
    fn senior_swe_bench_candidate_patch_extractor_accepts_diff_and_fenced_diff_only() {
        let diff = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new";
        assert_eq!(
            extract_senior_swe_bench_candidate_patch(diff).unwrap(),
            "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\n"
        );

        let fenced = "Here is the patch:\n```diff\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\n```\nDo not claim fitness.";
        assert_eq!(
            extract_senior_swe_bench_candidate_patch(fenced).unwrap(),
            "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\n"
        );

        let unterminated_fenced = "```diff\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new";
        assert_eq!(
            extract_senior_swe_bench_candidate_patch(unterminated_fenced).unwrap(),
            "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\n"
        );

        assert!(
            extract_senior_swe_bench_candidate_patch("```text\nunterminated prose without a diff")
                .unwrap_err()
                .contains("unified diff")
        );
        assert!(
            extract_senior_swe_bench_candidate_patch("explanation without a diff")
                .unwrap_err()
                .contains("unified diff")
        );
        assert!(
            extract_senior_swe_bench_candidate_patch(
                "```diff\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+https://github.com/org/repo/pull/1\n```"
            )
            .unwrap_err()
            .contains("GitHub solution")
        );
        assert!(
            extract_senior_swe_bench_candidate_patch(
                "```diff\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+HTTPS://GitHub.com/org/repo/PuLl/1\n```"
            )
            .unwrap_err()
            .contains("GitHub solution")
        );
        assert!(
            extract_senior_swe_bench_candidate_patch(
                "Copied from https://github.com/org/repo/pull/1\n```diff\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\n```"
            )
            .unwrap_err()
            .contains("GitHub solution")
        );
        assert!(
            extract_senior_swe_bench_candidate_patch(
                "```text\nsee /commit/deadbeef\n```\n```diff\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\n```"
            )
            .unwrap_err()
            .contains("GitHub solution")
        );
        assert!(
            extract_senior_swe_bench_candidate_patch(
                "Copied from https://raw.githubusercontent.com/org/repo/main/fix.diff\n```diff\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\n```"
            )
            .unwrap_err()
            .contains("GitHub solution")
        );
        assert!(
            extract_senior_swe_bench_candidate_patch(
                "Copied from github[.]com/org/repo/issues/1\n```diff\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\n```"
            )
            .unwrap_err()
            .contains("GitHub solution")
        );
        assert!(
            extract_senior_swe_bench_candidate_patch(
                "Copied from github dot com/org/repo/pull/1\n```diff\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\n```"
            )
            .unwrap_err()
            .contains("GitHub solution")
        );
        assert!(
            extract_senior_swe_bench_candidate_patch(
                "Copied from github . com/org/repo/commit/deadbeef\n```diff\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\n```"
            )
            .unwrap_err()
            .contains("GitHub solution")
        );
    }

    #[test]
    fn senior_swe_bench_candidate_patch_artifact_rejects_mismatched_extracted_patch() {
        let root = env::temp_dir().join(format!(
            "a2d-senior-swe-bench-artifact-patch-{}",
            unique_suffix()
        ));
        fs::create_dir_all(&root).unwrap();
        let artifact = root.join("coder-output.md");
        let extracted = root.join("candidate.diff");
        fs::write(
            &artifact,
            "```diff\n--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+new\n```\n",
        )
        .unwrap();
        fs::write(
            &extracted,
            "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-old\n+different\n",
        )
        .unwrap();
        let mut config = SeniorSweBenchEvaluateConfig {
            task_package: Some(root.join("task.json")),
            task_cycle_input: None,
            candidate_patch: extracted,
            candidate_patch_artifact: Some(artifact),
            extracted_candidate_patch: Some(root.join("candidate.diff")),
            checkout: root.join("checkout"),
            output: None,
            apply_candidate_patch: true,
            official_evaluator_manifest: None,
            official_evaluator_manifest_inspection: None,
            command: vec!["sh".to_string(), "evaluator.sh".to_string()],
        };

        let error = materialize_senior_swe_bench_candidate_patch_artifact(&mut config)
            .expect_err("mismatched extracted patch is rejected before evaluator execution");

        assert!(error.contains("does not match candidate patch artifact"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn plain_text_request_backfills_design_and_plan() {
        let artifacts = input_artifacts_from_request("Implement hello world");

        assert_eq!(
            artifacts.get(&art("requirements")).unwrap(),
            b"Implement hello world"
        );
        assert_eq!(
            artifacts.get(&art("design")).unwrap(),
            b"Implement hello world"
        );
        assert_eq!(
            artifacts.get(&art("plan")).unwrap(),
            b"Implement hello world"
        );
    }

    #[test]
    fn autopilot_args_parse_dry_run_iterations_and_dirty_override() {
        let args = vec![
            "--iterations".to_string(),
            "3".to_string(),
            "--dry-run".to_string(),
            "--allow-dirty".to_string(),
            "--repair-attempts".to_string(),
            "2".to_string(),
            "--repair-provider".to_string(),
            "opencode/opencode/deepseek-v4-flash-free".to_string(),
            "--source-fitness-evidence".to_string(),
            "runs/evidence.json".to_string(),
        ];

        let config = AutopilotConfig::parse(&args);

        assert_eq!(config.iterations, 3);
        assert!(config.dry_run);
        assert!(config.allow_dirty);
        assert_eq!(config.repair_attempts, 2);
        assert_eq!(
            config.repair_provider.as_deref(),
            Some("opencode/opencode/deepseek-v4-flash-free")
        );
        assert_eq!(
            config.source_fitness_evidence.as_deref(),
            Some(Path::new("runs/evidence.json"))
        );
    }

    #[test]
    fn autopilot_fault_injection_only_targets_configured_attempt() {
        assert_eq!(
            autopilot_fault_injection_for_attempt(Some("attempt0_parse_failure"), 0),
            Some("attempt0_parse_failure")
        );
        assert_eq!(
            autopilot_fault_injection_for_attempt(Some("parse-attempt0"), 0),
            Some("attempt0_parse_failure")
        );
        assert_eq!(
            autopilot_fault_injection_for_attempt(Some("attempt0_parse_failure"), 1),
            None
        );
        assert_eq!(autopilot_fault_injection_for_attempt(Some("off"), 0), None);
        assert_eq!(autopilot_fault_injection_for_attempt(None, 0), None);
    }

    #[test]
    fn repair_prompt_carries_failure_output_and_original_context() {
        let prompt = build_repair_prompt(
            "ORIGINAL TASK",
            "not json",
            "patchset parse failed: EOF",
            "primary_provider: pi/default\nattempted_provider: opencode/kimi-for-coding/k2p6",
        );

        assert!(prompt.contains("PROVIDER_ATTEMPT_METADATA"));
        assert!(prompt.contains("primary_provider: pi/default"));
        assert!(prompt.contains("attempted_provider: opencode/kimi-for-coding/k2p6"));
        assert!(prompt.contains("FAILURE_REPORT"));
        assert!(prompt.contains("patchset parse failed"));
        assert!(prompt.contains("PREVIOUS_OUTPUT"));
        assert!(prompt.contains("not json"));
        assert!(prompt.contains("ORIGINAL_TASK_AND_CONTEXT"));
        assert!(prompt.contains("ORIGINAL TASK"));
        assert!(prompt.contains("ProjectPatchset JSON"));
        assert!(prompt.contains("Do not return replacements: []"));
        assert!(prompt.contains("at least one complete file replacement"));
    }

    #[test]
    fn maintainer_prompt_forbids_empty_replacements() {
        let state = ProjectState {
            handoff_preview: String::new(),
            todos: vec![ProjectDoc {
                path: "todos/example.md".to_string(),
                title: "Example".to_string(),
                body: "# Example\n\n- [ ] Do docs work".to_string(),
                body_preview: "# Example".to_string(),
            }],
            plans: Vec::new(),
            git_status: String::new(),
            a2d_status: String::new(),
        };
        let task = ProjectTask {
            source_path: "todos/example.md".to_string(),
            objective: "Advance docs".to_string(),
            acceptance_gates: vec!["typed project_patchset JSON only".to_string()],
            allows_self_modification: false,
        };

        let system = maintainer_system_prompt();
        let prompt = build_maintainer_prompt(&state, &task);

        assert!(system.contains("replacements MUST contain at least one"));
        assert!(prompt.contains("replacements array MUST NOT be empty"));
        assert!(prompt.contains("replace source_path"));
    }

    #[test]
    fn autopilot_task_selector_prefers_outer_loop_and_allows_self_modification() {
        let state = ProjectState {
            handoff_preview: String::new(),
            todos: vec![
                ProjectDoc {
                    path: "todos/provider-policy-topology-gate.md".to_string(),
                    title: "Provider Policy Gate".to_string(),
                    body: "Gate provider policies".to_string(),
                    body_preview: "Gate provider policies".to_string(),
                },
                ProjectDoc {
                    path: "todos/autonomous-project-loop.md".to_string(),
                    title: "Autonomous Project Loop".to_string(),
                    body: "Allow gated self-modification of crates/ source".to_string(),
                    body_preview: "Allow gated self-modification of crates/ source".to_string(),
                },
            ],
            plans: Vec::new(),
            git_status: String::new(),
            a2d_status: String::new(),
        };

        let task = select_project_task(&state).unwrap();

        assert_eq!(task.source_path, "todos/autonomous-project-loop.md");
        assert!(task.allows_self_modification);
        assert!(
            task.acceptance_gates
                .iter()
                .any(|gate| gate.contains("self-modification"))
        );
    }

    #[test]
    fn autopilot_task_selector_skips_completed_checkbox_todos() {
        let state = ProjectState {
            handoff_preview: String::new(),
            todos: vec![
                ProjectDoc {
                    path: "todos/autonomous-project-loop.md".to_string(),
                    title: "Autonomous Project Loop".to_string(),
                    body: "# Done\n\n- [x] first\n- [x] second\n".to_string(),
                    body_preview: "# Done\n\n- [x] first\n- [x] second\n".to_string(),
                },
                ProjectDoc {
                    path: "todos/provider-policy-topology-gate.md".to_string(),
                    title: "Provider Policy Gate".to_string(),
                    body: "# Provider Policy Gate\n\n- [ ] implement bounded gate\n".to_string(),
                    body_preview: "# Provider Policy Gate\n\n- [ ] implement bounded gate\n"
                        .to_string(),
                },
            ],
            plans: Vec::new(),
            git_status: String::new(),
            a2d_status: String::new(),
        };

        let task = select_project_task(&state).unwrap();

        assert_eq!(task.source_path, "todos/provider-policy-topology-gate.md");
    }

    #[test]
    fn autopilot_logger_writes_jsonl_events_and_artifacts() {
        let root =
            std::env::temp_dir().join(format!("a2d-autopilot-logger-test-{}", unix_millis()));
        std::fs::create_dir_all(&root).unwrap();
        let logger = AutopilotLogger::new(&root);

        logger.event("test_event", json!({"ok": true}));
        let artifact_path = logger.artifact("iteration-1/output.txt", "MODEL OUTPUT");

        let events = std::fs::read_to_string(&logger.run_log).unwrap();
        assert!(events.contains("test_event"));
        assert!(events.contains("artifact_written"));
        assert_eq!(
            std::fs::read_to_string(artifact_path).unwrap(),
            "MODEL OUTPUT"
        );

        let aggregate = std::fs::read_to_string(&logger.aggregate_log).unwrap();
        assert!(aggregate.contains(&logger.run_id));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn project_patchset_parser_accepts_fenced_json() {
        let patchset = parse_project_patchset(
            r##"```json
            {
              "commit_message": "Autopilot: test",
              "validation_commands": ["cargo test"],
              "handoff_update": "updated",
              "replacements": [{"path":"todos/autonomous-project-loop.md","new_content":"# Updated\n"}]
            }
            ```"##,
        )
        .unwrap();

        assert_eq!(patchset.commit_message, "Autopilot: test");
        assert_eq!(patchset.replacements.len(), 1);
        assert_eq!(patchset.validation_commands, vec!["cargo test"]);
        assert_eq!(patchset.handoff_update, "updated");
    }

    #[test]
    fn validation_command_allowlist_rejects_shell_commands() {
        assert!(parse_allowed_validation_command("cargo test").is_some());
        assert!(parse_allowed_validation_command("cargo test -p a2d").is_some());
        assert!(parse_allowed_validation_command("rm -rf /").is_none());
        assert!(parse_allowed_validation_command("cargo test; rm -rf /").is_none());
    }

    #[test]
    fn temp_worktree_validation_applies_docs_patch_without_cargo() {
        let root = std::env::temp_dir().join(format!(
            "a2d-autopilot-docs-validation-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("docs/plans")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(root.join("docs/plans/example.md"), "# Old\n").unwrap();
        let patchset = ProjectPatchset {
            commit_message: "Autopilot: docs".to_string(),
            validation_commands: Vec::new(),
            handoff_update: String::new(),
            replacements: vec![ProjectFileReplacement {
                path: "docs/plans/example.md".to_string(),
                new_content: "# New\n".to_string(),
            }],
        };
        let gate = validate_project_patchset_paths(&patchset);

        let report = validate_project_patchset_in_temp_worktree(&root, &patchset, &gate);

        assert!(report.accepted, "{:?}", report.errors);
        assert!(report.command_results.is_empty());
        assert_eq!(
            std::fs::read_to_string(report.worktree_path.join("docs/plans/example.md")).unwrap(),
            "# New\n"
        );
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(report.worktree_path);
    }

    #[test]
    fn temp_worktree_validation_accepts_existing_markdown_repo_references() {
        let root = std::env::temp_dir().join(format!(
            "a2d-autopilot-doc-ref-validation-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("docs/plans")).unwrap();
        std::fs::create_dir_all(root.join("crates/a2d-core/src")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(root.join("docs/plans/example.md"), "# Old\n").unwrap();
        std::fs::write(root.join("crates/a2d-core/src/metabolism.rs"), "").unwrap();
        let patchset = ProjectPatchset {
            commit_message: "Autopilot: docs".to_string(),
            validation_commands: Vec::new(),
            handoff_update: "Validated docs/plans/example.md.".to_string(),
            replacements: vec![ProjectFileReplacement {
                path: "docs/plans/example.md".to_string(),
                new_content: "# New\n\nSee `crates/a2d-core/src/metabolism.rs:42` and [this plan](docs/plans/example.md#validation).\n".to_string(),
            }],
        };
        let gate = validate_project_patchset_paths(&patchset);

        let report = validate_project_patchset_in_temp_worktree(&root, &patchset, &gate);

        assert!(report.accepted, "{:?}", report.errors);
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(report.worktree_path);
    }

    #[test]
    fn temp_worktree_validation_rejects_missing_markdown_repo_references() {
        let root = std::env::temp_dir().join(format!(
            "a2d-autopilot-missing-doc-ref-validation-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("docs/plans")).unwrap();
        std::fs::create_dir_all(root.join("crates/a2d-core/src")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        std::fs::write(root.join("docs/plans/example.md"), "# Old\n").unwrap();
        std::fs::write(root.join("crates/a2d-core/src/metabolism.rs"), "").unwrap();
        let patchset = ProjectPatchset {
            commit_message: "Autopilot: docs".to_string(),
            validation_commands: Vec::new(),
            handoff_update: String::new(),
            replacements: vec![ProjectFileReplacement {
                path: "docs/plans/example.md".to_string(),
                new_content: "# New\n\nNext touch `crates/a2d-core/src/metabolism_workcell.rs` before `crates/a2d-core/src/provider_registry.rs`.\n".to_string(),
            }],
        };
        let gate = validate_project_patchset_paths(&patchset);

        let report = validate_project_patchset_in_temp_worktree(&root, &patchset, &gate);

        assert!(!report.accepted);
        assert!(
            report.errors.iter().any(|error| error.contains(
                "referenced repo path does not exist: crates/a2d-core/src/metabolism_workcell.rs"
            )),
            "{:?}",
            report.errors
        );
        assert!(
            report.errors.iter().any(|error| error.contains(
                "referenced repo path does not exist: crates/a2d-core/src/provider_registry.rs"
            )),
            "{:?}",
            report.errors
        );
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(report.worktree_path);
    }

    #[test]
    fn temp_worktree_validation_rejects_missing_source_target() {
        let root = std::env::temp_dir().join(format!(
            "a2d-autopilot-source-validation-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("crates/a2d-cli/src")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        let patchset = ProjectPatchset {
            commit_message: "Autopilot: source".to_string(),
            validation_commands: Vec::new(),
            handoff_update: String::new(),
            replacements: vec![ProjectFileReplacement {
                path: "crates/a2d-cli/src/main.rs".to_string(),
                new_content: "fn main() {}\n".to_string(),
            }],
        };
        let gate = validate_project_patchset_paths(&patchset);

        let report = validate_project_patchset_in_temp_worktree(&root, &patchset, &gate);

        assert!(!report.accepted);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("target does not exist"))
        );
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(report.worktree_path);
    }

    #[test]
    fn project_patch_gate_rejects_absolute_traversal_and_protected_paths() {
        let patchset = ProjectPatchset {
            commit_message: "Autopilot: bad".to_string(),
            validation_commands: Vec::new(),
            handoff_update: String::new(),
            replacements: vec![
                ProjectFileReplacement {
                    path: "/tmp/evil".to_string(),
                    new_content: "x".to_string(),
                },
                ProjectFileReplacement {
                    path: "../outside.md".to_string(),
                    new_content: "x".to_string(),
                },
                ProjectFileReplacement {
                    path: "crates/a2d-core/src/benchmark.rs".to_string(),
                    new_content: "x".to_string(),
                },
            ],
        };

        let report = validate_project_patchset_paths(&patchset);

        assert!(!report.accepted);
        assert_eq!(report.rejected.len(), 3);
        assert!(
            report
                .rejected
                .iter()
                .any(|reason| reason.contains("protected"))
        );
    }

    #[test]
    fn project_patch_gate_accepts_docs_without_cargo_test() {
        let patchset = ProjectPatchset {
            commit_message: "Autopilot: docs".to_string(),
            validation_commands: Vec::new(),
            handoff_update: String::new(),
            replacements: vec![ProjectFileReplacement {
                path: "docs/plans/autonomous-project-loop.md".to_string(),
                new_content: "# Updated\n".to_string(),
            }],
        };

        let report = validate_project_patchset_paths(&patchset);

        assert!(report.accepted, "{:?}", report.rejected);
        assert!(!report.requires_cargo_test);
    }

    #[test]
    fn project_patch_gate_accepts_handoff_update_path() {
        let patchset = ProjectPatchset {
            commit_message: "Autopilot: handoff".to_string(),
            validation_commands: Vec::new(),
            handoff_update: String::new(),
            replacements: vec![ProjectFileReplacement {
                path: "docs/HANDOFF.md".to_string(),
                new_content: "# Handoff\n".to_string(),
            }],
        };

        let report = validate_project_patchset_paths(&patchset);

        assert!(report.accepted, "{:?}", report.rejected);
        assert!(!report.requires_cargo_test);
    }

    #[test]
    fn project_patch_gate_rejects_disallowed_validation_command_during_temp_validation() {
        let root = std::env::temp_dir().join(format!(
            "a2d-autopilot-invalid-command-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("docs/plans")).unwrap();
        std::fs::write(root.join("docs/plans/example.md"), "# Old\n").unwrap();
        let patchset = ProjectPatchset {
            commit_message: "Autopilot: docs".to_string(),
            validation_commands: vec!["rm -rf /".to_string()],
            handoff_update: String::new(),
            replacements: vec![ProjectFileReplacement {
                path: "docs/plans/example.md".to_string(),
                new_content: "# New\n".to_string(),
            }],
        };
        let gate = validate_project_patchset_paths(&patchset);

        let report = validate_project_patchset_in_temp_worktree(&root, &patchset, &gate);

        assert!(!report.accepted);
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("not in the allowlist"))
        );
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(report.worktree_path);
    }

    fn write_retry_next_cycle_fixture(
        name: &str,
        unsafe_cycle_input: bool,
    ) -> (PathBuf, PathBuf, PathBuf, Vec<String>) {
        let root = env::temp_dir().join(format!(
            "a2d-retry-next-cycle-{name}-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let _ = fs::remove_dir_all(&root);
        let work_dir = root.join("work");
        let attempt0_dir = work_dir.join("attempt-0");
        let attempt1_output_dir = work_dir.join("attempt-1/cycle-output-artifacts");
        let checkout = root.join("checkout");
        fs::create_dir_all(&attempt0_dir).unwrap();
        fs::create_dir_all(&checkout).unwrap();
        let next_cycle_input = attempt0_dir.join("next-cycle-input.json");
        let search_allowed = if unsafe_cycle_input { "true" } else { "false" };
        fs::write(
            &next_cycle_input,
            format!(
                r#"{{
  "requirements": "Do not search GitHub. Return a unified diff candidate patch.",
  "design": "Use local checkout context only.",
  "plan": "Return only a diff.",
  "benchmark_context": {{
    "schema_version": "a2d.senior-swe-bench-task-package.v1",
    "task_id": "task-hard",
    "repo": "owner/repo",
    "github_solution_search_allowed": {search_allowed}
  }},
  "evaluation": {{
    "status": "not_evaluated",
    "evaluator": "official_senior_swe_bench",
    "fitness": null
  }}
}}
"#
            ),
        )
        .unwrap();
        let expected_manifest = attempt1_output_dir.join("manifest.json");
        let argv = vec![
            "cycle-input".to_string(),
            next_cycle_input.to_string_lossy().to_string(),
            "1".to_string(),
            "--checkout".to_string(),
            checkout.to_string_lossy().to_string(),
            "--output-artifacts".to_string(),
            attempt1_output_dir.to_string_lossy().to_string(),
        ];
        let next_cycle_command = json!({
            "command": "a2d",
            "argv": argv,
            "expected_manifest_path": expected_manifest.to_string_lossy(),
            "provider_invocations_started": false,
            "fitness_claim_allowed_before_evidence": false,
        });
        let retry_execution = json!({
            "schema_version": "a2d.senior-swe-bench-retry-execution.v1",
            "status": "failed",
            "stop_reason": "precomputed_attempt_manifests_exhausted",
            "task_id": "task-hard",
            "repo": "owner/repo",
            "max_attempts": 2,
            "attempts_executed": 1,
            "attempts": [{
                "attempt_index": 0,
                "next_cycle_input_path": next_cycle_input.to_string_lossy(),
                "next_cycle_command": next_cycle_command,
                "retry_step_decision": "build_next_cycle_input",
                "provider_invocations_started": false,
                "fitness_evidence_inspection_passed": false
            }],
            "provider_invocations_started": false,
            "evaluator_invocations_started": true,
            "fitness_evidence_inspection_passed": false,
            "fitness_claim_allowed_before_evidence": false,
            "fitness_claim_allowed_after_evidence_inspection": false,
            "github_solution_search_allowed": false,
            "next_cycle_command": next_cycle_command,
        });
        let retry_execution_path = work_dir.join("retry-execution.json");
        fs::write(
            &retry_execution_path,
            serde_json::to_vec_pretty(&retry_execution).unwrap(),
        )
        .unwrap();
        (root, retry_execution_path, expected_manifest, argv)
    }

    #[test]
    fn retry_run_next_cycle_invokes_persisted_boundary_once_and_persists_summary() {
        let (root, retry_execution, expected_manifest, expected_argv) =
            write_retry_next_cycle_fixture("success", false);
        let config = SeniorSweBenchRetryRunNextCycleConfig { retry_execution };
        let mut calls = Vec::new();
        let execution =
            build_senior_swe_bench_retry_next_cycle_execution_with_runner(&config, |argv| {
                calls.push(argv.to_vec());
                fs::create_dir_all(expected_manifest.parent().unwrap()).unwrap();
                let artifact_path = expected_manifest
                    .parent()
                    .unwrap()
                    .join("candidate.artifact");
                let artifact_bytes = b"candidate patch";
                fs::write(&artifact_path, artifact_bytes).unwrap();
                fs::write(
                    &expected_manifest,
                    serde_json::to_vec_pretty(&json!({
                        "schema_version": "a2d.cycle-output-artifacts.v1",
                        "artifacts": [{
                            "path": artifact_path.to_string_lossy(),
                            "git_object_hash": git_hash_object_bytes(artifact_bytes).unwrap(),
                            "bytes": artifact_bytes.len()
                        }]
                    }))
                    .unwrap(),
                )
                .unwrap();
                Ok(RetryNextCycleCommandOutput {
                    exit_code: Some(0),
                    stdout: b"cycle ok".to_vec(),
                    stderr: Vec::new(),
                    spawn_error: None,
                    timed_out: false,
                })
            })
            .unwrap();

        assert_eq!(calls, vec![expected_argv]);
        assert_eq!(
            execution["schema_version"].as_str(),
            Some("a2d.senior-swe-bench-retry-next-cycle-execution.v1")
        );
        assert_eq!(execution["status"].as_str(), Some("success"));
        assert_eq!(
            execution["cycle_output_manifest"].as_str(),
            Some(expected_manifest.to_str().unwrap())
        );
        assert_eq!(execution["cycle_output_artifact_count"].as_u64(), Some(1));
        assert_eq!(
            execution["cycle_input_command_started"].as_bool(),
            Some(true)
        );
        assert_eq!(
            execution["provider_invocations_started_by_this_command"].as_bool(),
            Some(true)
        );
        assert_eq!(
            execution["evaluator_invocations_started"].as_bool(),
            Some(false)
        );
        assert_eq!(
            execution["fitness_claim_allowed_after_cycle"].as_bool(),
            Some(false)
        );
        assert!(
            root.join("work/attempt-1/retry-next-cycle-execution.json")
                .is_file()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_next_gate_execution_surfaces_github_solution_search_policy_from_inputs() {
        let before_status_safe = json!({
            "github_solution_search_allowed": false,
        });
        let child_safe = json!({
            "schema_version": "a2d.test-child.v1",
            "status": "success",
            "github_solution_search_allowed": false,
        });
        let safe = retry_next_gate_execution_value(
            "retry_resume_attempt_plan",
            &before_status_safe,
            &child_safe,
            Path::new("child.json"),
            Path::new("controller.json"),
        );
        assert_eq!(
            safe["github_solution_search_allowed"].as_bool(),
            Some(false)
        );

        let before_status_unsafe = json!({
            "github_solution_search_allowed": true,
        });
        let unsafe_before = retry_next_gate_execution_value(
            "retry_resume_attempt_plan",
            &before_status_unsafe,
            &child_safe,
            Path::new("child.json"),
            Path::new("controller.json"),
        );
        assert_eq!(
            unsafe_before["github_solution_search_allowed"].as_bool(),
            Some(true),
            "controller metadata must not launder unsafe before_status policy to false"
        );

        let child_unsafe = json!({
            "schema_version": "a2d.test-child.v1",
            "status": "success",
            "github_solution_search_allowed": true,
        });
        let unsafe_child = retry_next_gate_execution_value(
            "retry_resume_attempt_plan",
            &before_status_safe,
            &child_unsafe,
            Path::new("child.json"),
            Path::new("controller.json"),
        );
        assert_eq!(
            unsafe_child["github_solution_search_allowed"].as_bool(),
            Some(true),
            "controller metadata must not launder unsafe child policy to false"
        );

        let controller_artifact = unique_temp_path("a2d-next-gate-policy-surfacing", "json");
        let persisted_from_controller_path = write_retry_next_gate_execution_artifact(
            "retry_resume_attempt_plan",
            &before_status_unsafe,
            &child_safe,
            Path::new("child.json"),
            &controller_artifact,
        )
        .unwrap();
        assert_eq!(
            persisted_from_controller_path["github_solution_search_allowed"].as_bool(),
            Some(true),
            "production controller artifact writer must preserve unsafe policy metadata"
        );
        let persisted: Value =
            serde_json::from_slice(&fs::read(&controller_artifact).unwrap()).unwrap();
        assert_eq!(
            persisted["github_solution_search_allowed"].as_bool(),
            Some(true),
            "persisted controller artifact must preserve unsafe policy metadata"
        );
        let _ = fs::remove_file(controller_artifact);
    }

    #[test]
    fn retry_run_next_gate_runs_only_next_cycle_boundary_without_chaining() {
        let (root, retry_execution, expected_manifest, expected_argv) =
            write_retry_next_cycle_fixture("next-gate-cycle", false);
        let config = SeniorSweBenchRetryRunNextGateConfig::FromRetryExecution(
            SeniorSweBenchRetryRunNextCycleConfig { retry_execution },
        );
        let mut calls = Vec::new();
        let execution =
            build_senior_swe_bench_retry_next_gate_execution_with_runner(&config, |argv| {
                calls.push(argv.to_vec());
                fs::create_dir_all(expected_manifest.parent().unwrap()).unwrap();
                let artifact_path = expected_manifest
                    .parent()
                    .unwrap()
                    .join("candidate.artifact");
                let artifact_bytes = b"candidate patch";
                fs::write(&artifact_path, artifact_bytes).unwrap();
                fs::write(
                    &expected_manifest,
                    serde_json::to_vec_pretty(&json!({
                        "schema_version": "a2d.cycle-output-artifacts.v1",
                        "artifacts": [{
                            "path": artifact_path.to_string_lossy(),
                            "git_object_hash": git_hash_object_bytes(artifact_bytes).unwrap(),
                            "bytes": artifact_bytes.len()
                        }]
                    }))
                    .unwrap(),
                )
                .unwrap();
                Ok(RetryNextCycleCommandOutput {
                    exit_code: Some(0),
                    stdout: b"cycle ok".to_vec(),
                    stderr: Vec::new(),
                    spawn_error: None,
                    timed_out: false,
                })
            })
            .unwrap();

        assert_eq!(calls, vec![expected_argv]);
        assert_eq!(
            execution["schema_version"].as_str(),
            Some("a2d.senior-swe-bench-retry-next-gate-execution.v1")
        );
        assert_eq!(
            execution["executed_gate"].as_str(),
            Some("retry_run_next_cycle")
        );
        assert_eq!(execution["status"].as_str(), Some("success"));
        assert_eq!(
            execution["child_schema"].as_str(),
            Some("a2d.senior-swe-bench-retry-next-cycle-execution.v1")
        );
        assert_eq!(
            execution["provider_invocations_started_by_this_command"].as_bool(),
            Some(true)
        );
        assert_eq!(
            execution["evaluator_invocations_started_by_this_command"].as_bool(),
            Some(false)
        );
        assert_eq!(
            execution["fitness_evidence_inspection_started_by_this_command"].as_bool(),
            Some(false)
        );
        assert_eq!(
            execution["fitness_claim_allowed_after_gate"].as_bool(),
            Some(false)
        );
        assert!(
            root.join("work/attempt-1/retry-next-cycle-execution.json")
                .is_file()
        );
        assert!(
            root.join("work/attempt-1/retry-next-gate-run-next-cycle.json")
                .is_file()
        );
        assert!(
            !root.join("work/attempt-1/retry-attempt-plan.json").exists(),
            "next-gate controller must not chain into resume planning in the same invocation"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_run_next_gate_rejects_legacy_retry_execution_without_pre_evidence_flag() {
        let (root, retry_execution, _, _) =
            write_retry_next_cycle_fixture("legacy-next-gate", false);
        let mut value: Value =
            serde_json::from_slice(&fs::read(&retry_execution).unwrap()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("fitness_claim_allowed_before_evidence");
        fs::write(&retry_execution, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        let config = SeniorSweBenchRetryRunNextGateConfig::FromRetryExecution(
            SeniorSweBenchRetryRunNextCycleConfig { retry_execution },
        );
        let mut called = false;
        let error = build_senior_swe_bench_retry_next_gate_execution_with_runner(&config, |_| {
            called = true;
            unreachable!("next-gate runner must not be called for legacy pre-evidence shape")
        })
        .expect_err("legacy retry execution rejected");
        assert!(error.contains("must forbid pre-evidence fitness claims"));
        assert!(!called);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_run_next_gate_rejects_existing_controller_artifact_before_child_side_effects() {
        let (root, retry_execution, _, _) =
            write_retry_next_cycle_fixture("next-gate-existing-controller", false);
        let controller_artifact = root.join("work/attempt-1/retry-next-gate-run-next-cycle.json");
        fs::create_dir_all(controller_artifact.parent().unwrap()).unwrap();
        fs::write(&controller_artifact, "{\"stale\":true}\n").unwrap();
        let config = SeniorSweBenchRetryRunNextGateConfig::FromRetryExecution(
            SeniorSweBenchRetryRunNextCycleConfig { retry_execution },
        );
        let mut called = false;
        let error = build_senior_swe_bench_retry_next_gate_execution_with_runner(&config, |_| {
            called = true;
            unreachable!("next-gate runner must not be called when controller artifact exists")
        })
        .expect_err("pre-existing controller artifact rejected");
        assert!(error.contains("already exists before child side effects"));
        assert!(!called);
        assert_eq!(
            fs::read_to_string(&controller_artifact).unwrap(),
            "{\"stale\":true}\n"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_run_next_cycle_rejects_existing_manifest_before_runner() {
        let (root, retry_execution, expected_manifest, _) =
            write_retry_next_cycle_fixture("existing-manifest", false);
        fs::create_dir_all(expected_manifest.parent().unwrap()).unwrap();
        fs::write(&expected_manifest, "{}\n").unwrap();
        let config = SeniorSweBenchRetryRunNextCycleConfig { retry_execution };
        let mut called = false;
        let error = build_senior_swe_bench_retry_next_cycle_execution_with_runner(&config, |_| {
            called = true;
            unreachable!("runner must not be called when manifest already exists")
        })
        .expect_err("pre-existing manifest rejected");
        assert!(error.contains("manifest already exists before run"));
        assert!(!called);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_run_next_cycle_rejects_unsafe_cycle_input_before_runner() {
        let (root, retry_execution, _, _) = write_retry_next_cycle_fixture("unsafe", true);
        let config = SeniorSweBenchRetryRunNextCycleConfig { retry_execution };
        let mut called = false;
        let error = build_senior_swe_bench_retry_next_cycle_execution_with_runner(&config, |_| {
            called = true;
            unreachable!("runner must not be called for unsafe cycle input")
        })
        .expect_err("unsafe cycle input rejected");
        assert!(error.contains("solution search"));
        assert!(!called);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_next_cycle_pre_provider_classifier_covers_all_validation_prefixes() {
        let pre_provider_cases = [
            "cycle-input --checkout found no bounded UTF-8 source/context files under checkout",
            "cycle-input --checkout must not be a symlink: checkout",
            "cycle-input --checkout must be a directory: checkout",
            "failed to read checkout src/lib.rs: No such file or directory",
            "failed to canonicalize checkout checkout: No such file or directory",
            "cycle-input cannot seed reserved runtime artifact: fitness_report",
            "cycle-input requires a JSON object artifact bundle",
        ];
        for stderr in pre_provider_cases {
            assert!(
                cycle_input_failure_happened_before_provider("", stderr),
                "expected pre-provider classifier to accept validation prefix: {stderr}"
            );
            assert!(
                cycle_input_failure_happened_before_provider("\n", &format!("  {stderr}")),
                "expected classifier to ignore leading stderr whitespace: {stderr}"
            );
        }

        for stderr in [
            "provider emitted text resembling failed to read checkout after invocation",
            "failed to read checkoutafter missing delimiter",
            "failed to canonicalize checkoutafter missing delimiter",
            "some other cycle-input failure",
        ] {
            assert!(
                !cycle_input_failure_happened_before_provider("", stderr),
                "unexpected pre-provider classification for stderr: {stderr}"
            );
        }

        assert!(
            !cycle_input_failure_happened_before_provider(
                "A²D Catalytic Cycle (1 cycle(s))\nRunning cycle 1/1...",
                "failed to read checkout src/lib.rs: provider stderr after invocation"
            ),
            "catalytic-cycle phase marker must make provider activity possible"
        );
    }

    #[test]
    fn retry_run_next_cycle_nonzero_exit_does_not_claim_fitness() {
        let (root, retry_execution, _, _) = write_retry_next_cycle_fixture("nonzero", false);
        let config = SeniorSweBenchRetryRunNextCycleConfig { retry_execution };
        let execution =
            build_senior_swe_bench_retry_next_cycle_execution_with_runner(&config, |_| {
                Ok(RetryNextCycleCommandOutput {
                    exit_code: Some(2),
                    stdout: "A²D Catalytic Cycle (1 cycle(s))\nRunning cycle 1/1..."
                        .as_bytes()
                        .to_vec(),
                    stderr:
                        b"provider emitted text resembling failed to read checkout after invocation"
                            .to_vec(),
                    spawn_error: None,
                    timed_out: false,
                })
            })
            .unwrap();
        assert_eq!(execution["status"].as_str(), Some("failed"));
        assert_eq!(
            execution["stop_reason"].as_str(),
            Some("cycle_input_command_failed")
        );
        assert_eq!(
            execution["cycle_input_failed_before_provider"].as_bool(),
            Some(false)
        );
        assert_eq!(
            execution["provider_invocations_started_by_this_command"].as_bool(),
            Some(true)
        );
        assert_eq!(
            execution["provider_invocation_observation"].as_str(),
            Some(
                "cycle-input subprocess spawned; provider activity inside that child is possible and not separately instrumented by the controller"
            )
        );
        assert_eq!(
            execution["fitness_claim_allowed_after_cycle"].as_bool(),
            Some(false)
        );
        assert_eq!(
            execution["fitness_evidence_inspection_started"].as_bool(),
            Some(false)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_run_next_cycle_spawn_failure_persists_summary_without_fitness_claim() {
        let (root, retry_execution, _, _) = write_retry_next_cycle_fixture("spawn-fail", false);
        let config = SeniorSweBenchRetryRunNextCycleConfig { retry_execution };
        let execution =
            build_senior_swe_bench_retry_next_cycle_execution_with_runner(&config, |_| {
                Err("spawn denied".to_string())
            })
            .unwrap();
        assert_eq!(execution["status"].as_str(), Some("failed"));
        assert_eq!(
            execution["stop_reason"].as_str(),
            Some("cycle_input_command_spawn_failed")
        );
        assert_eq!(execution["cycle_input_exit_code"], Value::Null);
        assert_eq!(
            execution["cycle_input_command_started"].as_bool(),
            Some(false)
        );
        assert_eq!(
            execution["cycle_input_failed_before_provider"].as_bool(),
            Some(false)
        );
        assert_eq!(
            execution["provider_invocations_started_by_this_command"].as_bool(),
            Some(false)
        );
        assert_eq!(
            execution["provider_invocation_observation"].as_str(),
            Some("cycle-input subprocess did not spawn")
        );
        assert_eq!(
            execution["fitness_claim_allowed_after_cycle"].as_bool(),
            Some(false)
        );
        assert!(
            root.join("work/attempt-1/retry-next-cycle-execution.json")
                .is_file()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_run_next_cycle_timeout_persists_summary_without_fitness_claim() {
        let (root, retry_execution, _, _) = write_retry_next_cycle_fixture("timeout", false);
        let config = SeniorSweBenchRetryRunNextCycleConfig { retry_execution };
        let execution =
            build_senior_swe_bench_retry_next_cycle_execution_with_runner(&config, |_| {
                Ok(RetryNextCycleCommandOutput {
                    exit_code: None,
                    stdout: Vec::new(),
                    stderr: b"timed out".to_vec(),
                    spawn_error: None,
                    timed_out: true,
                })
            })
            .unwrap();
        assert_eq!(execution["status"].as_str(), Some("failed"));
        assert_eq!(
            execution["stop_reason"].as_str(),
            Some("cycle_input_command_timed_out")
        );
        assert_eq!(
            execution["cycle_input_command_started"].as_bool(),
            Some(true)
        );
        assert_eq!(
            execution["cycle_input_command_timed_out"].as_bool(),
            Some(true)
        );
        assert_eq!(
            execution["fitness_claim_allowed_after_cycle"].as_bool(),
            Some(false)
        );
        assert!(
            root.join("work/attempt-1/retry-next-cycle-execution.json")
                .is_file()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_run_next_cycle_rejects_invalid_manifest_artifact_shape_after_runner() {
        let (root, retry_execution, expected_manifest, _) =
            write_retry_next_cycle_fixture("invalid-manifest-artifact", false);
        let config = SeniorSweBenchRetryRunNextCycleConfig { retry_execution };
        let execution =
            build_senior_swe_bench_retry_next_cycle_execution_with_runner(&config, |_| {
                fs::create_dir_all(expected_manifest.parent().unwrap()).unwrap();
                fs::write(
                    &expected_manifest,
                    serde_json::to_vec_pretty(&json!({
                        "schema_version": "a2d.cycle-output-artifacts.v1",
                        "artifacts": [{"path": "missing.artifact"}]
                    }))
                    .unwrap(),
                )
                .unwrap();
                Ok(RetryNextCycleCommandOutput {
                    exit_code: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    spawn_error: None,
                    timed_out: false,
                })
            })
            .unwrap();
        assert_eq!(execution["status"].as_str(), Some("failed"));
        assert_eq!(
            execution["stop_reason"].as_str(),
            Some("cycle_output_manifest_missing_or_invalid")
        );
        assert_eq!(execution["cycle_output_artifact_count"].as_u64(), Some(0));
        assert_eq!(
            execution["fitness_claim_allowed_after_cycle"].as_bool(),
            Some(false)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_run_next_cycle_rejects_task_mismatch_before_runner() {
        let (root, retry_execution, _, _) = write_retry_next_cycle_fixture("task-mismatch", false);
        let mut value: Value =
            serde_json::from_slice(&fs::read(&retry_execution).unwrap()).unwrap();
        value["task_id"] = json!("different-task");
        fs::write(&retry_execution, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        let config = SeniorSweBenchRetryRunNextCycleConfig { retry_execution };
        let mut called = false;
        let error = build_senior_swe_bench_retry_next_cycle_execution_with_runner(&config, |_| {
            called = true;
            unreachable!("runner must not be called for task mismatch")
        })
        .expect_err("task mismatch rejected");
        assert!(error.contains("does not match next cycle input"));
        assert!(!called);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_run_next_cycle_rejects_exhausted_retry_budget_before_runner() {
        let (root, retry_execution, _, _) =
            write_retry_next_cycle_fixture("budget-exhausted", false);
        let mut value: Value =
            serde_json::from_slice(&fs::read(&retry_execution).unwrap()).unwrap();
        value["attempts_executed"] = json!(2);
        let mut second = value["attempts"][0].clone();
        second["attempt_index"] = json!(1);
        value["attempts"] = json!([value["attempts"][0].clone(), second]);
        fs::write(&retry_execution, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        let config = SeniorSweBenchRetryRunNextCycleConfig { retry_execution };
        let mut called = false;
        let error = build_senior_swe_bench_retry_next_cycle_execution_with_runner(&config, |_| {
            called = true;
            unreachable!("runner must not be called after retry budget exhaustion")
        })
        .expect_err("exhausted budget rejected");
        assert!(error.contains("must be below max_attempts"));
        assert!(!called);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn retry_run_next_cycle_requires_last_attempt_build_next_cycle_decision() {
        let (root, retry_execution, _, _) = write_retry_next_cycle_fixture("wrong-decision", false);
        let mut value: Value =
            serde_json::from_slice(&fs::read(&retry_execution).unwrap()).unwrap();
        value["attempts"][0]["retry_step_decision"] = json!("stop");
        fs::write(&retry_execution, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        let config = SeniorSweBenchRetryRunNextCycleConfig { retry_execution };
        let mut called = false;
        let error = build_senior_swe_bench_retry_next_cycle_execution_with_runner(&config, |_| {
            called = true;
            unreachable!("runner must not be called for wrong retry step decision")
        })
        .expect_err("wrong retry-step decision rejected");
        assert!(error.contains("requires last attempt to build next cycle input"));
        assert!(!called);
        let _ = fs::remove_dir_all(root);
    }

    fn init_minimal_autopilot_source_repo(name: &str) -> PathBuf {
        let repo = std::env::temp_dir().join(format!(
            "a2d-autopilot-source-fitness-{name}-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(repo.join("crates/a2d-cli/src")).unwrap();
        std::fs::write(
            repo.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a2d-cli\"]\n",
        )
        .unwrap();
        std::fs::write(
            repo.join("crates/a2d-cli/Cargo.toml"),
            "[package]\nname = \"a2d\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        std::fs::write(repo.join("crates/a2d-cli/src/main.rs"), "fn main() {}\n").unwrap();
        for args in [
            vec!["init"],
            vec!["config", "user.email", "a2d@example.invalid"],
            vec!["config", "user.name", "A2D Test"],
            vec!["add", "."],
            vec!["commit", "-m", "initial"],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        repo
    }

    fn write_autopilot_source_fitness_evidence_for_current_diff(root: &Path) -> PathBuf {
        let mut evidence = complete_fitness_evidence(0);
        evidence["source_revision"] = json!(git_scope_revision_at(root, "crates").unwrap());
        evidence["source_tree_dirty"] =
            json!(!git_status_for_scope_at(root, "crates").unwrap().is_empty());
        evidence["source_diff_scope"] = json!("crates");
        evidence["source_diff_hash"] = json!(git_diff_hash_for_scope_at(root, "crates").unwrap());
        evidence["evidence_command"] = json!("score-artifact sudoku good-sudoku-artifact.rs");
        let path = root.join("source-fitness-evidence.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        path
    }

    #[test]
    fn autopilot_source_patch_requires_fitness_evidence_before_commit() {
        let repo = init_minimal_autopilot_source_repo("missing-evidence");
        let patchset = ProjectPatchset {
            commit_message: "Autopilot: source".to_string(),
            validation_commands: Vec::new(),
            handoff_update: String::new(),
            replacements: vec![ProjectFileReplacement {
                path: "crates/a2d-cli/src/main.rs".to_string(),
                new_content: "fn main() { println!(\"patched\"); }\n".to_string(),
            }],
        };
        let gate = validate_project_patchset_paths(&patchset);
        assert!(gate.requires_cargo_test);

        let report = apply_validated_patchset_to_real_tree(&repo, &patchset, &gate, None);

        assert!(!report.accepted);
        assert!(!report.committed);
        assert!(report.fitness_evidence_required);
        assert!(report.fitness_evidence_path.is_none());
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("source-bound a2d.fitness-evidence.v1"))
        );
        let report_json = project_apply_report_json(&report);
        assert_eq!(report_json["fitness_evidence_required"], json!(true));
        assert_eq!(report_json["fitness_evidence_path"], Value::Null);
        assert_eq!(
            std::fs::read_to_string(repo.join("crates/a2d-cli/src/main.rs")).unwrap(),
            "fn main() {}\n"
        );
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn autopilot_source_fitness_evidence_accepts_current_source_bound_hidden_status() {
        let repo = init_minimal_autopilot_source_repo("valid-evidence");
        let patched = "fn main() { println!(\"patched\"); }\n";
        let source_path = repo.join("crates/a2d-cli/src/main.rs");
        std::fs::write(&source_path, patched).unwrap();
        let evidence_path = write_autopilot_source_fitness_evidence_for_current_diff(&repo);
        std::fs::write(&source_path, "fn main() {}\n").unwrap();
        let output = Command::new("git")
            .args(["checkout", "--", "crates/a2d-cli/src/main.rs"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(output.status.success());
        let patchset = ProjectPatchset {
            commit_message: "Autopilot: source".to_string(),
            validation_commands: Vec::new(),
            handoff_update: String::new(),
            replacements: vec![ProjectFileReplacement {
                path: "crates/a2d-cli/src/main.rs".to_string(),
                new_content: patched.to_string(),
            }],
        };
        let gate = validate_project_patchset_paths(&patchset);

        let report =
            apply_validated_patchset_to_real_tree(&repo, &patchset, &gate, Some(&evidence_path));

        assert!(report.accepted, "{:?}", report.errors);
        assert!(report.committed);
        assert!(report.fitness_evidence_required);
        assert_eq!(
            report.fitness_evidence_path.as_deref(),
            Some(evidence_path.to_string_lossy().as_ref())
        );
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn autopilot_source_fitness_evidence_rejects_stale_diff_hash() {
        let repo = init_minimal_autopilot_source_repo("stale-evidence");
        let patched = "fn main() { println!(\"patched\"); }\n";
        let source_path = repo.join("crates/a2d-cli/src/main.rs");
        std::fs::write(&source_path, patched).unwrap();
        let evidence_path = write_autopilot_source_fitness_evidence_for_current_diff(&repo);
        let mut evidence: Value =
            serde_json::from_slice(&std::fs::read(&evidence_path).unwrap()).unwrap();
        evidence["source_diff_hash"] = json!("0123456789abcdef0123456789abcdef01234567");
        std::fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();

        let error = validate_autopilot_source_fitness_evidence(&repo, &evidence_path).unwrap_err();

        assert!(error.contains("source_diff_hash"));
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn autopilot_source_fitness_evidence_rejects_stale_or_non_actual_provenance() {
        let repo = init_minimal_autopilot_source_repo("invalid-provenance");
        let patched = "fn main() { println!(\"patched\"); }\n";
        let source_path = repo.join("crates/a2d-cli/src/main.rs");
        std::fs::write(&source_path, patched).unwrap();
        let evidence_path = write_autopilot_source_fitness_evidence_for_current_diff(&repo);
        let valid: Value = serde_json::from_slice(&std::fs::read(&evidence_path).unwrap()).unwrap();

        for (field, replacement, expected) in [
            ("source_revision", json!("deadbee"), "source_revision"),
            ("source_tree_dirty", json!(false), "source_tree_dirty"),
            ("source_diff_scope", json!("docs"), "source_diff_scope"),
            ("evidence_command", json!(""), "evidence_command"),
            (
                "actual_tests_evaluated",
                json!(false),
                "actual_tests_evaluated",
            ),
            ("non_regressing", json!(false), "non_regressing"),
        ] {
            let mut evidence = valid.clone();
            evidence[field] = replacement;
            std::fs::write(
                &evidence_path,
                serde_json::to_vec_pretty(&evidence).unwrap(),
            )
            .unwrap();

            let error =
                validate_autopilot_source_fitness_evidence(&repo, &evidence_path).unwrap_err();

            assert!(
                error.contains(expected),
                "expected {expected} in error for {field}, got {error}"
            );
        }
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn real_tree_apply_rolls_back_when_validation_fails() {
        let root = std::env::temp_dir().join(format!(
            "a2d-autopilot-rollback-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("docs/plans")).unwrap();
        std::fs::write(root.join("docs/plans/example.md"), "# Old\n").unwrap();
        let patchset = ProjectPatchset {
            commit_message: "Autopilot: rollback".to_string(),
            validation_commands: vec!["cargo run -q -p a2d -- status".to_string()],
            handoff_update: String::new(),
            replacements: vec![ProjectFileReplacement {
                path: "docs/plans/example.md".to_string(),
                new_content: "# New\n".to_string(),
            }],
        };
        let gate = validate_project_patchset_paths(&patchset);

        let report = apply_validated_patchset_to_real_tree(&root, &patchset, &gate, None);

        assert!(!report.accepted);
        assert!(!report.committed);
        assert_eq!(
            std::fs::read_to_string(root.join("docs/plans/example.md")).unwrap(),
            "# Old\n"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn real_tree_apply_commits_only_project_scoped_paths_when_parent_repo_has_staged_noise() {
        let repo = std::env::temp_dir().join(format!(
            "a2d-autopilot-scoped-commit-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        let project = repo.join("a2d");
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(project.join("docs/plans")).unwrap();
        std::fs::write(project.join("docs/plans/example.md"), "# Old\n").unwrap();

        for args in [
            vec!["init"],
            vec!["config", "user.email", "a2d@example.invalid"],
            vec!["config", "user.name", "A2D Test"],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap();
            assert!(output.status.success());
        }
        let output = Command::new("git")
            .args(["add", "a2d/docs/plans/example.md"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(output.status.success());
        let output = Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(output.status.success());

        std::fs::write(repo.join("sibling.md"), "# Sibling\n").unwrap();
        let output = Command::new("git")
            .args(["add", "sibling.md"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(output.status.success());
        let scoped_status = command_stdout(&project, "git", &["status", "--short", "--", "."]);
        assert_eq!(scoped_status.trim(), "");

        let patchset = ProjectPatchset {
            commit_message: "Autopilot: scoped docs".to_string(),
            validation_commands: Vec::new(),
            handoff_update: String::new(),
            replacements: vec![ProjectFileReplacement {
                path: "docs/plans/example.md".to_string(),
                new_content: "# New\n".to_string(),
            }],
        };
        let gate = validate_project_patchset_paths(&patchset);

        let report = apply_validated_patchset_to_real_tree(&project, &patchset, &gate, None);

        assert!(report.accepted, "{:?}", report.errors);
        assert!(report.committed);
        let show = Command::new("git")
            .args(["show", "--name-only", "--format=", "HEAD"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(show.status.success());
        let committed_paths = String::from_utf8_lossy(&show.stdout);
        assert!(
            committed_paths.contains("a2d/docs/plans/example.md"),
            "{committed_paths}"
        );
        assert!(
            !committed_paths.contains("sibling.md"),
            "out-of-scope staged sibling was committed: {committed_paths}"
        );
        let full_status = command_stdout(&project, "git", &["status", "--short"]);
        assert!(
            full_status.contains("A  ../sibling.md"),
            "out-of-scope staged sibling should remain staged outside autopilot commit: {full_status}"
        );
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn project_patch_gate_accepts_eligible_source_self_modification_with_cargo_test() {
        let patchset = ProjectPatchset {
            commit_message: "Autopilot: source".to_string(),
            validation_commands: vec!["cargo test".to_string()],
            handoff_update: String::new(),
            replacements: vec![ProjectFileReplacement {
                path: "crates/a2d-cli/src/main.rs".to_string(),
                new_content: "fn main() {}\n".to_string(),
            }],
        };

        let report = validate_project_patchset_paths(&patchset);

        assert!(report.accepted, "{:?}", report.rejected);
        assert!(report.requires_cargo_test);
    }
}
