//! A²D CLI: run the catalytic cycle.
//!
//! Usage:
//!   a2d cycle              Run one catalytic cycle
//!   a2d status             Show RAF closure status
//!   a2d enzymes            List enzymes in the germline

use a2d_core::benchmark::seed_benchmark;
use a2d_core::challenges;
use a2d_core::germline::Germline;
use a2d_core::lineage::LineageArchive;
use a2d_core::metabolism::{CycleReport, InvocationLineage, Metabolism, fitness_evidence_artifact};
use a2d_core::provider::{InvocationRequest, Provider, ProviderPolicy, ProviderRegistry};
use a2d_core::self_sandbox;
use a2d_core::types::{ArtifactType, EnzymeDef, EnzymeId};
use a2d_providers::cli::CliProvider;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
                "Usage: a2d <cycle|challenge|score-artifact|compare-topologies|compare-provider-policy|compare-role-providers|validate-escalation|autopilot|status|enzymes|lineage>"
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
    "You are a Rust programmer. You receive three complementary artifacts:\n\
     - design: concrete structure or reference implementation details\n\
     - plan: the intended architecture or implementation steps\n\
     - requirements: the user-visible contract to satisfy\n\n\
     Synthesize all three into a SINGLE complete Rust source file.\n\
     Prefer the most specific, most constrained instructions when the artifacts differ.\n\
     The file MUST:\n\
     1. Contain a main() function\n\
     2. Include #[cfg(test)] mod tests with at least 3 test cases\n\
     3. Use Result<T, E> for error handling where appropriate\n\
     4. Include /// doc comments on public functions\n\
     5. Compile with `rustc --edition 2024`\n\
     6. Do NOT define a module named `a2d_acceptance` — that module will be appended by the system. If you define it, compilation will fail with a duplicate definition error.\n\
     7. Place all your tests in a module named `tests` (i.e. `#[cfg(test)] mod tests { ... }`), NOT `a2d_acceptance`.\n\n\
     Output ONLY the Rust source code inside ```rust fences. No explanation."
        .to_string()
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

        let needs_prompt_upgrade = coder
            .prompt_template
            .as_deref()
            .is_none_or(|template| !(template.contains("design") && template.contains("plan")));
        if needs_prompt_upgrade {
            coder.prompt_template = Some(coder_prompt_template());
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
        "{}-{}",
        unix_millis(),
        UNIQUE_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
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
            let apply_report = apply_validated_patchset_to_real_tree(&root, &patchset, &gate);
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
         Do not claim repo paths that are absent: markdown replacements and handoff_update fail validation when referenced crates/..., docs/..., todos/..., examples/..., or research/... paths do not exist after the patch.",
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
) -> ProjectApplyReport {
    let mut errors = Vec::new();
    let mut command_results = Vec::new();
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
        "command_results": report.command_results.iter().map(|result| json!({
            "command": result.command,
            "success": result.success,
            "status": result.status,
            "stdout_preview": result.stdout_preview,
            "stderr_preview": result.stderr_preview,
        })).collect::<Vec<_>>(),
    })
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

fn run_cycle(num_cycles: usize, requirements: &str) {
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

fn export_score_artifact_fitness_evidence(
    report: &a2d_core::benchmark::FitnessReport,
    export_dir: &Path,
    challenge_name: &str,
) -> Result<PathBuf, String> {
    let bytes = fitness_evidence_artifact(0, report, 0.0);
    let value = validate_exportable_fitness_evidence(&bytes, 0)?;
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
    let allowed_fields = BTreeSet::from([
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
    for field in object.keys() {
        if !allowed_fields.contains(field.as_str()) {
            return Err(format!(
                "fitness evidence contains unreviewed field: {field}"
            ));
        }
    }
    for field in allowed_fields {
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

    Ok(value)
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

        let report = apply_validated_patchset_to_real_tree(&root, &patchset, &gate);

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

        let report = apply_validated_patchset_to_real_tree(&project, &patchset, &gate);

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
