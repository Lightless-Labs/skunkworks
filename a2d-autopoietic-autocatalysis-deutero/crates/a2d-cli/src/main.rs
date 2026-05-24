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
use a2d_core::metabolism::{CycleReport, InvocationLineage, Metabolism};
use a2d_core::provider::{InvocationRequest, ProviderPolicy, ProviderRegistry};
use a2d_core::self_sandbox;
use a2d_core::types::{ArtifactType, EnzymeDef, EnzymeId};
use a2d_providers::cli::CliProvider;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::Write;
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
        "autopilot" => run_autopilot(AutopilotConfig::parse(&args[2..])),
        "status" => show_status(),
        "enzymes" => list_enzymes(),
        "lineage" => show_lineage(),
        _ => {
            eprintln!(
                "Usage: a2d <cycle|challenge|compare-topologies|autopilot|status|enzymes|lineage>"
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
}

impl TopologyMode {
    fn label(self) -> &'static str {
        match self {
            TopologyMode::Seed => "seed",
            TopologyMode::Evolved => "evolved",
        }
    }
}

fn load_germline_for_topology(mode: TopologyMode) -> Germline {
    match mode {
        TopologyMode::Seed => seed_germline(),
        TopologyMode::Evolved => load_lineage_germline().unwrap_or_else(seed_germline),
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
    let mut registry = build_registry();

    if force_seed_germline(env::var("A2D_GERMLINE").ok().as_deref()) {
        return registry;
    }

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

    registry
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
    // Gemini is intentionally not registered in the default live configuration:
    // repeated Gemini quota failures consumed ~5 minute timeout windows before
    // the provider circuit breaker could route later invocations.
    let coder = CliProvider::opencode("kimi-for-coding/k2p6");
    let mut registry = ProviderRegistry::new(Box::new(coder));

    registry.register(Box::new(CliProvider::opencode(
        "opencode/deepseek-v4-flash-free",
    )));
    let glm = registry.register(Box::new(CliProvider::opencode("zai-coding-plan/glm-5.1")));

    registry.assign(EnzymeId::from("evolver"), 0);
    for enzyme in ["tester", "architect"] {
        registry.assign(EnzymeId::from(enzyme), glm);
    }

    registry
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
    let valid_enzyme_ids = germline
        .enzymes()
        .into_iter()
        .map(|enzyme| enzyme.id.clone())
        .collect::<BTreeSet<_>>();
    registry.apply_policy(policy, &valid_enzyme_ids)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutopilotConfig {
    iterations: usize,
    dry_run: bool,
    allow_dirty: bool,
}

impl AutopilotConfig {
    fn parse(args: &[String]) -> Self {
        let mut config = Self {
            iterations: 1,
            dry_run: false,
            allow_dirty: false,
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
                _ => idx += 1,
            }
        }

        config
    }
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
        }),
    );
    println!("Autopilot run id: {}", logger.run_id);
    println!("Autopilot logs: {}", logger.run_log.to_string_lossy());

    let state = collect_project_state(&root);
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

    for iteration in 1..=config.iterations {
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
        let provider = registry.provider_for(&EnzymeId::from("maintainer"));
        println!("Invoking maintainer via {}...", provider.name());
        logger.event(
            "maintainer_invocation_started",
            json!({"iteration": iteration, "provider": provider.name()}),
        );
        let response = match provider.invoke(&InvocationRequest {
            enzyme_id: EnzymeId::from("maintainer"),
            system: maintainer_system_prompt(),
            prompt,
            max_tokens: 12_000,
        }) {
            Ok(response) => response,
            Err(error) => {
                println!("Maintainer invocation failed: {error}");
                logger.event(
                    "maintainer_invocation_failed",
                    json!({"iteration": iteration, "provider": provider.name(), "error": error.to_string()}),
                );
                return;
            }
        };

        let output_path = logger.artifact(
            &format!("iteration-{iteration}/maintainer-output.txt"),
            &response.text,
        );
        if let Some(raw) = &response.raw_output {
            logger.artifact(
                &format!("iteration-{iteration}/maintainer-raw-output.txt"),
                raw,
            );
        }
        logger.event(
            "maintainer_output_received",
            json!({
                "iteration": iteration,
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
                println!("Maintainer returned malformed project_patchset: {error}");
                println!("Parsed output preview: {}", preview(&response.text, 1200));
                logger.event(
                    "patchset_parse_failed",
                    json!({
                        "iteration": iteration,
                        "error": error.to_string(),
                        "output_artifact": output_path.to_string_lossy(),
                    }),
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
            &format!("iteration-{iteration}/patchset-summary.json"),
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
                "accepted": gate.accepted,
                "rejected": gate.rejected,
                "requires_cargo_test": gate.requires_cargo_test,
                "replacement_paths": patchset.replacements.iter().map(|replacement| replacement.path.clone()).collect::<Vec<_>>(),
            }),
        );
        if !gate.accepted {
            println!("Patchset rejected by path gate:");
            for reason in gate.rejected {
                println!("  - {reason}");
            }
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
            &format!("iteration-{iteration}/validation-report.json"),
            &serde_json::to_string_pretty(&validation_json).unwrap_or_default(),
        );
        logger.event(
            "temp_worktree_validation_completed",
            json!({
                "iteration": iteration,
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
            println!("Temp-worktree validation failed:");
            for error in validation.errors {
                println!("  - {error}");
            }
            return;
        }

        println!(
            "Patchset passed temp-worktree validation. Applying to real tree and committing..."
        );
        logger.event("real_tree_apply_started", json!({"iteration": iteration}));
        let apply_report = apply_validated_patchset_to_real_tree(&root, &patchset, &gate);
        let apply_json = project_apply_report_json(&apply_report);
        logger.artifact(
            &format!("iteration-{iteration}/apply-report.json"),
            &serde_json::to_string_pretty(&apply_json).unwrap_or_default(),
        );
        logger.event(
            "real_tree_apply_completed",
            json!({
                "iteration": iteration,
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
        } else {
            println!("Real-tree application failed and was rolled back:");
            for error in apply_report.errors {
                println!("  - {error}");
            }
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
        .find_map(|path| state.todos.iter().find(|doc| doc.path == *path))
        .or_else(|| state.todos.first())?;

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

fn maintainer_system_prompt() -> String {
    "You are A²D's outer-loop maintainer enzyme. Your job is gated autonomous self-modification of this repository.\n\
     Return JSON only. Do not run shell commands. Do not describe changes outside JSON.\n\
     Produce a project_patchset with: commit_message, validation_commands, handoff_update, replacements.\n\
     replacements must be complete file contents. Source self-modification is allowed for eligible mechanism files; protected files are not.\n\
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
         OUTPUT CONTRACT\n\
         Return exactly one JSON object matching:\n\
         {{\"commit_message\":\"Autopilot: ...\",\"validation_commands\":[\"cargo test\"],\"handoff_update\":\"...\",\"replacements\":[{{\"path\":\"relative/path\",\"new_content\":\"complete file content\"}}]}}",
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
            .join("\n")
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

    match git_commit(root, &patchset.commit_message) {
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

fn git_commit(root: &Path, message: &str) -> Result<String, String> {
    let commit = Command::new("git")
        .args(["commit", "-m", message])
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

        // Fitness-gated persistence: only commit if fitness didn't regress
        let regressed = report.fitness_delta.is_some_and(|d| d < 0.0);
        if report.accepted_mutations > 0 {
            if regressed {
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
            if regressed {
                println!("  ⚠ Fitness regressed — skipping provider policy commit");
            } else if let Some(ref archive) = archive {
                match archive.commit_provider_policy(&metabolism.provider_policy(), &report) {
                    Ok(hash) => println!("  Provider policy lineage: {hash}"),
                    Err(e) => eprintln!("  Provider policy lineage error: {e}"),
                }
            }
        }

        // Apply accepted system patches to the real source tree.
        if report.accepted_patches > 0 {
            apply_accepted_patches(&metabolism);
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
    let mut benchmark = challenge.benchmark;
    benchmark.acceptance_test = challenge.acceptance_test;
    let mut metabolism = apply_runtime_env(
        Metabolism::new(germline, registry)
            .with_benchmark(benchmark)
            .with_project_root(project_root()),
    );

    seed_initial_runtime_artifacts(&mut metabolism, challenge.requirements);

    let lineage_dir = lineage_dir();
    let archive = LineageArchive::init(&lineage_dir).ok();

    let mut best_fitness: f64 = 0.0;

    for cycle_num in 1..=num_cycles {
        println!("\nCycle {cycle_num}/{num_cycles}...");
        let report = metabolism.run_cycle();

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
                    match archive.commit_provider_policy(&metabolism.provider_policy(), &report) {
                        Ok(hash) => print!(" [policy: {hash}]"),
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

        // Apply accepted system patches to the real source tree.
        if report.accepted_patches > 0 {
            apply_accepted_patches(&metabolism);
        }
    }

    println!("\n═══════════════════");
    println!(
        "Challenge: {} | Best fitness: {:.0}%",
        challenge.name,
        best_fitness * 100.0
    );
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

fn run_challenge_for_topology(
    name: &str,
    num_cycles: usize,
    topology: TopologyMode,
) -> TopologyRunSummary {
    let challenge = load_challenge_or_exit(name);
    let challenge_name = challenge.name.to_string();
    let requirements = challenge.requirements;
    let mut benchmark = challenge.benchmark;
    benchmark.acceptance_test = challenge.acceptance_test;

    let germline = load_germline_for_topology(topology);
    let enzyme_count = germline.enzymes().len();
    let registry = if topology == TopologyMode::Evolved {
        build_runtime_registry(&germline)
    } else {
        build_registry()
    };
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

    format!("    [{} via {}] {outcome}", entry.enzyme_id, entry.provider)
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

/// Apply accepted system patches to the real source tree.
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

    fn topology_entry(outcome: a2d_core::workcell::WorkcellOutcome) -> InvocationLineage {
        InvocationLineage {
            cycle: 1,
            workcell_id: a2d_core::workcell::WorkcellId("wc-test".to_string()),
            enzyme_id: EnzymeId::from("evolver"),
            provider: "opencode/kimi-for-coding/k2p6".to_string(),
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
    fn force_seed_germline_accepts_explicit_seed_modes() {
        assert!(force_seed_germline(Some("seed")));
        assert!(force_seed_germline(Some("baseline")));
        assert!(force_seed_germline(Some("4")));
        assert!(force_seed_germline(Some("1")));
        assert!(!force_seed_germline(Some("lineage")));
        assert!(!force_seed_germline(None));
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
        assert!(
            registry
                .providers()
                .contains(&"opencode/opencode/deepseek-v4-flash-free")
        );
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
        ];

        let config = AutopilotConfig::parse(&args);

        assert_eq!(config.iterations, 3);
        assert!(config.dry_run);
        assert!(config.allow_dirty);
    }

    #[test]
    fn autopilot_task_selector_prefers_outer_loop_and_allows_self_modification() {
        let state = ProjectState {
            handoff_preview: String::new(),
            todos: vec![
                ProjectDoc {
                    path: "todos/provider-policy-topology-gate.md".to_string(),
                    title: "Provider Policy Gate".to_string(),
                    body_preview: "Gate provider policies".to_string(),
                },
                ProjectDoc {
                    path: "todos/autonomous-project-loop.md".to_string(),
                    title: "Autonomous Project Loop".to_string(),
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
