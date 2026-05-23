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
use a2d_core::provider::ProviderRegistry;
use a2d_core::types::{ArtifactType, EnzymeDef, EnzymeId};
use a2d_providers::cli::CliProvider;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

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
        "status" => show_status(),
        "enzymes" => list_enzymes(),
        "lineage" => show_lineage(),
        _ => {
            eprintln!("Usage: a2d <cycle|challenge|compare-topologies|status|enzymes|lineage>");
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
    let registry = build_registry();
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
        if report.accepted_mutations > 0 {
            let regressed = report.fitness_delta.is_some_and(|d| d < 0.0);

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
    let registry = build_registry();
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
    let registry = build_registry();
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
}
