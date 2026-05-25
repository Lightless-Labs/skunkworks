//! a2ctl — CLI for A² Autopoietic Autocatalysis.
//!
//! Stage 0 commands:
//!   a2ctl task "title" "description"   — create and run a task
//!   a2ctl run < tasks.txt              — run stdin tasks sequentially
//!                                        (plain text or JSONL with problem_statement)
//!   a2ctl bench                        — run the A² benchmark suite
//!   a2ctl sentinel                     — run the seed sentinel suite
//!   a2ctl hello                        — print a one-line greeting
//!   a2ctl status                       — show system health

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "a2ctl", version, about = "A² — Autopoietic Autocatalysis")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create and run a task through the governor pipeline.
    Task {
        /// Task title.
        title: String,
        /// Task description.
        description: String,
        /// Maximum token budget.
        #[arg(long, default_value = "50000")]
        max_tokens: u64,
        /// Maximum wall-clock time per task in seconds.
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Model provider/model (e.g., "claude" or "gemini").
        #[arg(long, default_value = "claude")]
        model: String,
        /// Dry run: create task but don't execute.
        #[arg(long)]
        dry_run: bool,
        /// Auto-apply promoted patches via git apply.
        #[arg(long)]
        apply: bool,
    },
    /// Read task descriptions from stdin and run them sequentially.
    /// Accepts plain text lines or JSONL tasks with `problem_statement`.
    Run {
        /// Maximum token budget per task.
        #[arg(long, default_value = "50000")]
        max_tokens: u64,
        /// Maximum wall-clock time per task in seconds.
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Provider(s) to use. Comma-separated list for round-robin cycling
        /// across tasks (e.g. "claude,gemini,codex,opencode").
        /// Available: claude, gemini, codex, opencode
        #[arg(long, default_value = "claude")]
        provider: String,
        /// Auto-apply promoted patches via git apply.
        #[arg(long)]
        apply: bool,
        /// Benchmark ablation: disable the anti-repeat retry prompt motif while
        /// keeping prior lineage and verifier-derived retry context enabled.
        #[arg(long)]
        disable_anti_repeat_retry: bool,
    },
    /// Continuously pick project work, execute workcells, verify, and log evidence.
    Autopilot {
        /// Workspace root path (defaults to current directory).
        #[arg(long, default_value = ".")]
        workspace: String,
        /// Provider(s) to use. Comma-separated list for round-robin cycling.
        #[arg(long, default_value = "pi/zai/glm-5.1")]
        provider: String,
        /// Maximum autopilot iterations before stopping.
        #[arg(long, default_value = "3")]
        max_iterations: usize,
        /// Maximum token budget per task.
        #[arg(long, default_value = "100000")]
        max_tokens: u64,
        /// Maximum wall-clock time per task in seconds.
        #[arg(long, default_value = "1800")]
        timeout: u64,
        /// Auto-apply promoted patches via git apply.
        #[arg(long)]
        apply: bool,
        /// Explicit task to run instead of discovering project work. May be repeated.
        #[arg(long)]
        task: Vec<String>,
        /// File containing an explicit task to run instead of discovering project work. May be repeated.
        #[arg(long)]
        task_file: Vec<String>,
        /// Only discover and log candidate work; do not call a model.
        #[arg(long)]
        dry_run: bool,
        /// Directory for durable autopilot logs, relative to workspace unless absolute.
        #[arg(long, default_value = ".a2/autopilot")]
        log_dir: String,
    },
    /// Scan the workspace for TODO/FIXME comments and emit task descriptions.
    /// With --run, pipe discoveries directly into the run loop.
    Scan {
        /// Workspace root path (defaults to current directory).
        #[arg(long, default_value = ".")]
        workspace: String,
        /// Execute discovered tasks through the run loop instead of printing them.
        #[arg(long)]
        run: bool,
        /// Provider(s) to use when --run is set (comma-separated for round-robin).
        #[arg(long, default_value = "claude")]
        provider: String,
        /// Maximum token budget per task when --run is set.
        #[arg(long, default_value = "50000")]
        max_tokens: u64,
        /// Maximum wall-clock time per task in seconds when --run is set.
        #[arg(long, default_value = "300")]
        timeout: u64,
        /// Auto-apply promoted patches when --run is set.
        #[arg(long)]
        apply: bool,
    },
    /// Run the seed sentinel suite.
    Sentinel {
        /// Workspace root path (defaults to current directory).
        #[arg(long, default_value = ".")]
        workspace: String,
    },
    /// Run the A² benchmark suite from bench/tasks.
    Bench {
        /// Model provider/model (e.g., "claude" or "gemini").
        #[arg(long, default_value = "claude")]
        model: String,
    },
    /// Print a one-line greeting.
    Hello,
    /// Show system status and health.
    Status,
}

struct RunSummaryRow {
    title: String,
    model: String,
    tokens: u64,
    duration_secs: f64,
    decision: String,
}

struct BenchSummaryRow {
    title: String,
    model: String,
    tokens: u64,
    duration_secs: f64,
    promoted: bool,
}

#[derive(Debug, Deserialize)]
struct BenchTaskFile {
    task: BenchTaskSpec,
    verify: BenchVerifySpec,
    setup: BenchSetupSpec,
}

#[derive(Debug, Deserialize)]
struct BenchTaskSpec {
    title: String,
    description: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BenchVerifySpec {
    command: String,
    expect_exit: i32,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BenchSetupSpec {
    test_file: String,
    test_content: String,
}

#[allow(dead_code)]
struct BenchTaskCase {
    path: PathBuf,
    task: BenchTaskSpec,
    verify: BenchVerifySpec,
    setup: BenchSetupSpec,
}

// ---------------------------------------------------------------------------
// Autopilot run summary — persisted as run_summary.json per autopilot run.
// ---------------------------------------------------------------------------

/// Aggregated summary of an entire autopilot run, written to
/// `<log_dir>/runs/run-<timestamp>/run_summary.json` on completion.
#[derive(Serialize)]
struct AutopilotRunSummary {
    run_id: String,
    workspace: String,
    provider: String,
    max_iterations: usize,
    started_at: String,
    completed_at: String,
    total_iterations: usize,
    total_tokens: u64,
    total_duration_secs: f64,
    patches_produced: usize,
    applied_count: usize,
    verified_count: usize,
    iterations: Vec<AutopilotIterationSummary>,
}

/// Per-iteration detail within an autopilot run.
#[derive(Serialize)]
struct AutopilotIterationSummary {
    iteration: usize,
    task_id: String,
    candidate_id: String,
    candidate_source: String,
    candidate_title: String,
    model: String,
    tokens: u64,
    duration_secs: f64,
    decision: String,
    patch_produced: bool,
    patch_stats: Option<PatchStats>,
    verifier_focus: Vec<String>,
    apply_ok: bool,
    verify_ok: bool,
    apply_note: Option<String>,
}

/// Patch statistics extracted from the candidate diff.
#[derive(Serialize)]
struct PatchStats {
    files_touched: Vec<String>,
    diff_lines: usize,
    diff_bytes: usize,
}

fn extract_patch_stats(diff: &str) -> PatchStats {
    let files = extract_diff_files(diff);
    PatchStats {
        files_touched: files,
        diff_lines: diff.lines().count(),
        diff_bytes: diff.len(),
    }
}

fn extract_diff_files(diff: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            let path = rest.strip_prefix("b/").unwrap_or(rest).trim();
            if !path.is_empty()
                && path != "/dev/null"
                && path != "dev/null"
                && !files.iter().any(|f| f == path)
            {
                files.push(path.to_string());
            }
        }
    }
    files
}

/// Extract verifier failure focus and failing test names from the lineage
/// record and the candidate patch's worktree verifications.
fn extract_verifier_focus(outcome: &a2d::GovernorOutcome) -> Vec<String> {
    let mut focus = Vec::new();
    let push_unique = |focus: &mut Vec<String>, item: String| {
        if !item.trim().is_empty() && !focus.iter().any(|f| f == &item) {
            focus.push(item);
        }
    };
    for verification in outcome.lineage.external_verifications.iter().rev() {
        if !verification.passed {
            for item in verification.failure_focus.iter() {
                push_unique(&mut focus, item.clone());
            }
            for test in verification.failing_tests.iter() {
                push_unique(&mut focus, test.clone());
            }
        }
    }
    if let Some(patch) = &outcome.result.patch {
        for verification in patch.worktree_verifications.iter().rev() {
            if !verification.passed {
                for item in verification.failure_focus.iter() {
                    push_unique(&mut focus, item.clone());
                }
                for test in verification.failing_tests.iter() {
                    push_unique(&mut focus, test.clone());
                }
            }
        }
    }
    focus
}

#[derive(Debug, Deserialize)]
struct RunInputTask {
    problem_statement: String,
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    verification_commands: Vec<RunVerificationSpec>,
}

#[derive(Debug, Deserialize)]
struct RunVerificationSpec {
    command: String,
    #[serde(default)]
    expect_exit: i32,
}

const DEFAULT_STAGNATION_WINDOW: usize = 3;
const DEFAULT_BENCH_MAX_TOKENS: u64 = 100_000;
const DEFAULT_BENCH_TIMEOUT_SECS: u64 = 1800;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("a2=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Task {
            title,
            description,
            max_tokens,
            timeout,
            model,
            dry_run,
            apply,
        } => {
            let budget = build_budget(max_tokens, timeout);

            let ingester = a2_sensorium::ingest::Ingester::new(budget.clone());
            let task = ingester.from_human(&title, &description);

            println!("A² Task: {}", task.id);
            println!("Title: {title}");
            println!("Model: {model}");
            println!("Budget: {max_tokens} tokens");
            println!();

            if dry_run {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&task)
                        .unwrap_or_else(|_| "serialization error".into())
                );
                println!();
                println!("[dry run — task not executed]");
                return;
            }

            let provider = build_provider(&model).await;
            let workspace_root = workspace_root();
            let catalyst =
                a2_workcell::worktree_catalyst::WorktreeCatalyst::new(workspace_root.clone());
            let evaluator = a2_eval::seed::SeedEvaluator::new(max_tokens);
            let governor = a2d::Governor::with_stagnation_detector(
                a2_core::id::GermlineVersion::new(),
                budget,
                a2d::StagnationDetector::new(DEFAULT_STAGNATION_WINDOW),
            );

            println!("Executing...");
            println!();

            match run_task(&governor, task, &catalyst, provider.as_ref(), &evaluator).await {
                Ok(outcome) => {
                    println!("--- Result ---");
                    println!("Workcell: {}", outcome.workcell_id);
                    println!(
                        "Tokens: {} | Duration: {:.1}s",
                        outcome.result.tokens_used, outcome.result.duration_secs
                    );
                    println!();

                    match &outcome.result.patch {
                        Some(patch) => {
                            println!("--- Diff ---");
                            println!("{}", patch.diff);
                            println!();
                            println!("--- Rationale ---");
                            println!("{}", patch.rationale);
                        }
                        None => {
                            println!("[no patch produced]");
                        }
                    }

                    println!();
                    println!("--- Promotion Decision ---");
                    println!("{:?}", outcome.decision);

                    if apply
                        && let a2_core::protocol::PromotionDecision::PromoteGermline { .. } =
                            &outcome.decision
                        && let Some(patch) = &outcome.result.patch
                    {
                        match try_apply_patch(&patch.diff, &workspace_root).and_then(|applied| {
                            if applied {
                                verify_and_rebuild().map_err(|e| e.to_string())
                            } else {
                                Ok(false)
                            }
                        }) {
                            Ok(true) => println!("--- Applied and rebuilt ---"),
                            Ok(false) => println!("[empty diff, nothing to apply]"),
                            Err(e) => eprintln!("[apply/rebuild failed: {e}]"),
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Task failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Run {
            max_tokens,
            timeout,
            provider,
            apply,
            disable_anti_repeat_retry,
        } => {
            let budget = build_budget(max_tokens, timeout);
            let ingester = a2_sensorium::ingest::Ingester::new(budget.clone());

            let provider_names: Vec<&str> = provider
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            let mut providers: Vec<Box<dyn a2_core::traits::ModelProvider>> = Vec::new();
            for name in &provider_names {
                providers.push(build_provider(name).await);
            }
            if providers.is_empty() {
                eprintln!("No valid providers specified.");
                std::process::exit(1);
            }

            let workspace_root = workspace_root();
            let catalyst =
                a2_workcell::worktree_catalyst::WorktreeCatalyst::new(workspace_root.clone());
            let evaluator = a2_eval::seed::SeedEvaluator::new(max_tokens);
            let lineage_db = workspace_root.join("lineage.sqlite");
            let governor = match rusqlite::Connection::open(&lineage_db)
                .map_err(|e| format!("open lineage db: {e}"))
                .and_then(|conn| {
                    a2_archive::SqliteLineageStore::new(conn)
                        .map_err(|e| format!("init lineage store: {e}"))
                }) {
                Ok(store) => {
                    eprintln!("[lineage store: {}]", lineage_db.display());
                    a2d::Governor::with_stagnation_detector(
                        a2_core::id::GermlineVersion::new(),
                        budget,
                        a2d::StagnationDetector::new(DEFAULT_STAGNATION_WINDOW),
                    )
                    .with_lineage_store(std::sync::Arc::new(store))
                }
                Err(e) => {
                    eprintln!("[lineage store unavailable: {e}]");
                    a2d::Governor::with_stagnation_detector(
                        a2_core::id::GermlineVersion::new(),
                        budget,
                        a2d::StagnationDetector::new(DEFAULT_STAGNATION_WINDOW),
                    )
                }
            }
            .with_anti_repeat_retry(!disable_anti_repeat_retry);

            let mut rows = Vec::new();
            let mut task_index: usize = 0;

            for line in io::stdin().lock().lines() {
                let raw_line = match line {
                    Ok(line) => line,
                    Err(e) => {
                        eprintln!("Failed to read stdin: {e}");
                        std::process::exit(1);
                    }
                };

                let raw_line = raw_line.trim();
                if raw_line.is_empty() {
                    continue;
                }

                let task = task_from_run_input(&ingester, parse_run_input(raw_line));

                let title = task.title.clone();

                // Check stagnation and advance provider if needed.
                let strategy = governor.suggest_strategy_change();
                if strategy == a2d::StrategyChange::SwitchModel && providers.len() > 1 {
                    task_index += 1;
                    eprintln!(
                        "[stagnation: switching to {}]",
                        providers[task_index % providers.len()].model_id()
                    );
                }

                let p = providers[task_index % providers.len()].as_ref();
                task_index += 1;

                match run_task(&governor, task, &catalyst, p, &evaluator).await {
                    Ok(outcome) => {
                        let mut apply_ok = false;
                        let mut verify_ok = false;
                        if apply
                            && let a2_core::protocol::PromotionDecision::PromoteGermline { .. } =
                                &outcome.decision
                            && let Some(patch) = &outcome.result.patch
                        {
                            let apply_outcome =
                                apply_and_verify_patch(&patch.diff, &workspace_root);
                            apply_ok = apply_outcome.applied;
                            verify_ok = apply_outcome.verified;

                            if let Err(e) = governor
                                .reconcile_lineage_apply_outcome(
                                    &outcome.lineage.id,
                                    apply_outcome.applied,
                                    apply_outcome.verified,
                                    apply_outcome.note.clone(),
                                    apply_outcome.external_verification.clone(),
                                )
                                .await
                            {
                                eprintln!("[lineage reconciliation failed for {title}: {e}]");
                            }

                            if apply_outcome.verified {
                                eprintln!("[applied and rebuilt: {title}]");
                            } else {
                                eprintln!(
                                    "[apply/rebuild failed for {title}: {}]",
                                    apply_outcome.note
                                );
                            }
                        }
                        governor.record_apply_outcome(apply_ok, verify_ok);
                        rows.push(run_summary_row(&title, p, &outcome));
                    }
                    Err(e) => rows.push(RunSummaryRow {
                        title,
                        model: requested_model(p),
                        tokens: 0,
                        duration_secs: 0.0,
                        decision: format!("error: {e}"),
                    }),
                }
            }

            if rows.is_empty() {
                eprintln!("No task descriptions provided on stdin.");
                std::process::exit(1);
            }

            print!("{}", render_summary_table(&rows));
        }
        Commands::Autopilot {
            workspace,
            provider,
            max_iterations,
            max_tokens,
            timeout,
            apply,
            task,
            task_file,
            dry_run,
            log_dir,
        } => {
            if max_iterations == 0 {
                eprintln!("--max-iterations must be greater than zero");
                std::process::exit(1);
            }

            let workspace_root = PathBuf::from(&workspace);
            let candidates = if task.is_empty() && task_file.is_empty() {
                match collect_autopilot_candidates(&workspace_root) {
                    Ok(candidates) => candidates,
                    Err(e) => {
                        eprintln!("Autopilot discovery failed: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                match explicit_autopilot_candidates(&workspace_root, &task, &task_file) {
                    Ok(candidates) => candidates,
                    Err(e) => {
                        eprintln!("Autopilot explicit task setup failed: {e}");
                        std::process::exit(1);
                    }
                }
            };
            let run_dir = autopilot_run_dir(&workspace_root, Path::new(&log_dir));
            if let Err(e) = fs::create_dir_all(&run_dir) {
                eprintln!("Autopilot log setup failed: {e}");
                std::process::exit(1);
            }
            log_autopilot_event(
                &run_dir,
                "run_started",
                serde_json::json!({
                    "workspace": workspace_root.display().to_string(),
                    "provider": provider,
                    "max_iterations": max_iterations,
                    "max_tokens": max_tokens,
                    "timeout": timeout,
                    "apply": apply,
                    "dry_run": dry_run,
                }),
            );
            log_autopilot_event(
                &run_dir,
                "candidates_discovered",
                serde_json::json!({
                    "count": candidates.len(),
                    "candidates": candidates.iter().map(autopilot_candidate_json).collect::<Vec<_>>(),
                }),
            );

            println!("A² Autopilot run: {}", run_dir.display());
            println!("Discovered {} candidate tasks", candidates.len());
            if candidates.is_empty() {
                println!("No candidate work found; stopping.");
                return;
            }

            if dry_run {
                for candidate in candidates.iter().take(max_iterations) {
                    println!("- {} [{}]", candidate.title, candidate.source);
                }
                println!("[dry run — no model calls]");
                return;
            }

            if apply {
                match tracked_workspace_changes(&workspace_root) {
                    Ok(changes) if !changes.trim().is_empty() => {
                        eprintln!(
                            "Autopilot apply requires a clean tracked workspace. Current changes:\n{changes}"
                        );
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("Autopilot dirty-check failed: {e}");
                        std::process::exit(1);
                    }
                    _ => {}
                }
            }

            let budget = build_budget(max_tokens, timeout);
            let ingester = a2_sensorium::ingest::Ingester::new(budget.clone());
            let provider_names: Vec<&str> = provider
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            let mut providers: Vec<Box<dyn a2_core::traits::ModelProvider>> = Vec::new();
            for name in &provider_names {
                providers.push(build_provider(name).await);
            }
            if providers.is_empty() {
                eprintln!("No valid providers specified.");
                std::process::exit(1);
            }

            let catalyst =
                a2_workcell::worktree_catalyst::WorktreeCatalyst::new(workspace_root.clone());
            let evaluator = a2_eval::seed::SeedEvaluator::new(max_tokens);
            let lineage_db = workspace_root.join("lineage.sqlite");
            let governor = match rusqlite::Connection::open(&lineage_db)
                .map_err(|e| format!("open lineage db: {e}"))
                .and_then(|conn| {
                    a2_archive::SqliteLineageStore::new(conn)
                        .map_err(|e| format!("init lineage store: {e}"))
                }) {
                Ok(store) => a2d::Governor::with_stagnation_detector(
                    a2_core::id::GermlineVersion::new(),
                    budget,
                    a2d::StagnationDetector::new(DEFAULT_STAGNATION_WINDOW),
                )
                .with_lineage_store(std::sync::Arc::new(store)),
                Err(e) => {
                    eprintln!("[lineage store unavailable: {e}]");
                    a2d::Governor::with_stagnation_detector(
                        a2_core::id::GermlineVersion::new(),
                        budget,
                        a2d::StagnationDetector::new(DEFAULT_STAGNATION_WINDOW),
                    )
                }
            };

            let started_at = chrono::Utc::now().to_rfc3339();
            let mut iteration_summaries: Vec<AutopilotIterationSummary> = Vec::new();
            let mut rows = Vec::new();
            for (iteration, candidate) in candidates.iter().take(max_iterations).enumerate() {
                let mut task = ingester.ingest(a2_sensorium::ingest::RawSignal {
                    origin: "autopilot".into(),
                    content: candidate.description.clone(),
                    risk_tier: a2_sensorium::ingest::RiskTier::Low,
                    metadata: vec![("source".into(), candidate.source.clone())],
                });
                task.id = a2_core::id::TaskId::from_external_key(&candidate.id);
                task.title = candidate.title.clone();

                let provider = providers[iteration % providers.len()].as_ref();
                log_autopilot_event(
                    &run_dir,
                    "iteration_started",
                    serde_json::json!({
                        "iteration": iteration + 1,
                        "task_id": task.id.to_string(),
                        "candidate": autopilot_candidate_json(candidate),
                        "model": requested_model(provider),
                    }),
                );

                match run_task(&governor, task, &catalyst, provider, &evaluator).await {
                    Ok(outcome) => {
                        let mut apply_ok = false;
                        let mut verify_ok = false;
                        let mut apply_note = None;
                        if apply
                            && let a2_core::protocol::PromotionDecision::PromoteGermline { .. } =
                                &outcome.decision
                            && let Some(patch) = &outcome.result.patch
                        {
                            let apply_outcome =
                                apply_and_verify_patch(&patch.diff, &workspace_root);
                            apply_ok = apply_outcome.applied;
                            verify_ok = apply_outcome.verified;
                            apply_note = Some(apply_outcome.note.clone());
                            if let Err(e) = governor
                                .reconcile_lineage_apply_outcome(
                                    &outcome.lineage.id,
                                    apply_outcome.applied,
                                    apply_outcome.verified,
                                    apply_outcome.note,
                                    apply_outcome.external_verification,
                                )
                                .await
                            {
                                eprintln!("[lineage reconciliation failed: {e}]");
                            }
                        }
                        governor.record_apply_outcome(apply_ok, verify_ok);
                        let patch_stats = outcome
                            .result
                            .patch
                            .as_ref()
                            .map(|p| extract_patch_stats(&p.diff));
                        let patch_stats_json = patch_stats.as_ref().map(|s| {
                            serde_json::json!({
                                "files_touched": &s.files_touched,
                                "diff_lines": s.diff_lines,
                                "diff_bytes": s.diff_bytes,
                            })
                        });
                        let verifier_focus = extract_verifier_focus(&outcome);
                        let model_attr = outcome
                            .result
                            .patch
                            .as_ref()
                            .map(|p| {
                                format!(
                                    "{}/{}",
                                    p.model_attribution.provider, p.model_attribution.model
                                )
                            })
                            .unwrap_or_else(|| requested_model(provider));
                        let decision_str = format_promotion_decision(&outcome.decision);

                        iteration_summaries.push(AutopilotIterationSummary {
                            iteration: iteration + 1,
                            task_id: outcome.task_id.to_string(),
                            candidate_id: candidate.id.clone(),
                            candidate_source: candidate.source.clone(),
                            candidate_title: candidate.title.clone(),
                            model: model_attr.clone(),
                            tokens: outcome.result.tokens_used,
                            duration_secs: outcome.result.duration_secs,
                            decision: decision_str.clone(),
                            patch_produced: outcome.result.patch.is_some(),
                            patch_stats,
                            verifier_focus: verifier_focus.clone(),
                            apply_ok,
                            verify_ok,
                            apply_note: apply_note.clone(),
                        });

                        log_autopilot_event(
                            &run_dir,
                            "iteration_completed",
                            serde_json::json!({
                                "iteration": iteration + 1,
                                "task_id": outcome.task_id.to_string(),
                                "candidate_id": candidate.id,
                                "candidate_source": candidate.source,
                                "model": model_attr,
                                "decision": decision_str,
                                "tokens": outcome.result.tokens_used,
                                "duration_secs": outcome.result.duration_secs,
                                "patch_produced": outcome.result.patch.is_some(),
                                "patch_stats": patch_stats_json,
                                "verifier_focus": verifier_focus,
                                "apply_ok": apply_ok,
                                "verify_ok": verify_ok,
                                "apply_note": apply_note,
                            }),
                        );
                        rows.push(run_summary_row(&candidate.title, provider, &outcome));
                    }
                    Err(e) => {
                        let model_attr = requested_model(provider);
                        let decision_str = format!("error: {e}");
                        log_autopilot_event(
                            &run_dir,
                            "iteration_failed",
                            serde_json::json!({
                                "iteration": iteration + 1,
                                "candidate_id": candidate.id,
                                "candidate_source": candidate.source,
                                "candidate": autopilot_candidate_json(candidate),
                                "model": &model_attr,
                                "error": e.to_string(),
                            }),
                        );
                        iteration_summaries.push(AutopilotIterationSummary {
                            iteration: iteration + 1,
                            task_id: String::new(),
                            candidate_id: candidate.id.clone(),
                            candidate_source: candidate.source.clone(),
                            candidate_title: candidate.title.clone(),
                            model: model_attr.clone(),
                            tokens: 0,
                            duration_secs: 0.0,
                            decision: decision_str.clone(),
                            patch_produced: false,
                            patch_stats: None,
                            verifier_focus: Vec::new(),
                            apply_ok: false,
                            verify_ok: false,
                            apply_note: None,
                        });
                        rows.push(RunSummaryRow {
                            title: candidate.title.clone(),
                            model: model_attr,
                            tokens: 0,
                            duration_secs: 0.0,
                            decision: decision_str,
                        });
                    }
                }
            }

            let completed_at = chrono::Utc::now().to_rfc3339();
            let total_tokens: u64 = iteration_summaries.iter().map(|s| s.tokens).sum();
            let total_duration_secs: f64 =
                iteration_summaries.iter().map(|s| s.duration_secs).sum();
            let patches_produced = iteration_summaries
                .iter()
                .filter(|s| s.patch_produced)
                .count();
            let applied_count = iteration_summaries.iter().filter(|s| s.apply_ok).count();
            let verified_count = iteration_summaries.iter().filter(|s| s.verify_ok).count();

            let run_summary = AutopilotRunSummary {
                run_id: run_dir
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                workspace: workspace_root.display().to_string(),
                provider: provider.clone(),
                max_iterations,
                started_at,
                completed_at,
                total_iterations: iteration_summaries.len(),
                total_tokens,
                total_duration_secs,
                patches_produced,
                applied_count,
                verified_count,
                iterations: iteration_summaries,
            };

            let summary_path = run_dir.join("run_summary.json");
            match serde_json::to_string_pretty(&run_summary) {
                Ok(json) => {
                    if let Err(e) = fs::write(&summary_path, json) {
                        eprintln!("[failed to write run summary: {e}]");
                    }
                }
                Err(e) => eprintln!("[failed to serialize run summary: {e}]"),
            }

            log_autopilot_event(
                &run_dir,
                "run_completed",
                serde_json::json!({
                    "iterations": run_summary.total_iterations,
                    "total_tokens": run_summary.total_tokens,
                    "total_duration_secs": run_summary.total_duration_secs,
                    "patches_produced": run_summary.patches_produced,
                    "applied_count": run_summary.applied_count,
                    "verified_count": run_summary.verified_count,
                }),
            );
            print!("{}", render_summary_table(&rows));
            println!("Autopilot log: {}", run_dir.display());
        }
        Commands::Scan {
            workspace,
            run,
            provider,
            max_tokens,
            timeout,
            apply,
        } => {
            let tasks = match scan_workspace(Path::new(&workspace)) {
                Ok(tasks) => tasks,
                Err(e) => {
                    eprintln!("Scan failed: {e}");
                    std::process::exit(1);
                }
            };

            if !run {
                for task in tasks {
                    println!("{task}");
                }
            } else {
                if tasks.is_empty() {
                    eprintln!("No TODO/FIXME items found.");
                    return;
                }

                let budget = build_budget(max_tokens, timeout);
                let ingester = a2_sensorium::ingest::Ingester::new(budget.clone());

                let provider_names: Vec<&str> = provider
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut providers: Vec<Box<dyn a2_core::traits::ModelProvider>> = Vec::new();
                for name in &provider_names {
                    providers.push(build_provider(name).await);
                }
                if providers.is_empty() {
                    eprintln!("No valid providers specified.");
                    std::process::exit(1);
                }

                let workspace_root = workspace_root();
                let catalyst =
                    a2_workcell::worktree_catalyst::WorktreeCatalyst::new(workspace_root.clone());
                let evaluator = a2_eval::seed::SeedEvaluator::new(max_tokens);
                let governor = a2d::Governor::with_stagnation_detector(
                    a2_core::id::GermlineVersion::new(),
                    budget,
                    a2d::StagnationDetector::new(DEFAULT_STAGNATION_WINDOW),
                );

                let mut rows = Vec::new();
                for (task_index, description) in tasks.iter().enumerate() {
                    let task = ingester.ingest(a2_sensorium::ingest::RawSignal {
                        origin: "scan".into(),
                        content: description.clone(),
                        risk_tier: a2_sensorium::ingest::RiskTier::Low,
                        metadata: vec![],
                    });

                    let title = task.title.clone();
                    let p = providers[task_index % providers.len()].as_ref();

                    match run_task(&governor, task, &catalyst, p, &evaluator).await {
                        Ok(outcome) => {
                            if apply
                                && let a2_core::protocol::PromotionDecision::PromoteGermline {
                                    ..
                                } = &outcome.decision
                                && let Some(patch) = &outcome.result.patch
                            {
                                match try_apply_patch(&patch.diff, &workspace_root).and_then(
                                    |applied| {
                                        if applied {
                                            verify_and_rebuild().map_err(|e| e.to_string())
                                        } else {
                                            Ok(false)
                                        }
                                    },
                                ) {
                                    Ok(true) => eprintln!("[applied and rebuilt: {title}]"),
                                    Ok(false) => {}
                                    Err(e) => {
                                        eprintln!("[apply/rebuild failed for {title}: {e}]")
                                    }
                                }
                            }
                            rows.push(run_summary_row(&title, p, &outcome));
                        }
                        Err(e) => rows.push(RunSummaryRow {
                            title,
                            model: requested_model(p),
                            tokens: 0,
                            duration_secs: 0.0,
                            decision: format!("error: {e}"),
                        }),
                    }
                }

                print!("{}", render_summary_table(&rows));
            }
        }
        Commands::Sentinel { workspace } => {
            println!("A² Seed Sentinel Suite");
            println!("Workspace: {workspace}");
            println!();

            let suite =
                a2_eval::sentinel::SentinelSuite::seed_suite(std::path::PathBuf::from(&workspace));
            let result = suite.run_all();

            for r in &result.results {
                let icon = if r.passed { "PASS" } else { "FAIL" };
                println!("  [{icon}] {}: {}", r.name, r.detail);
            }

            println!();
            println!(
                "Score: {:.0}% ({}/{})",
                result.score * 100.0,
                result.results.iter().filter(|r| r.passed).count(),
                result.results.len()
            );

            if result.all_passed {
                println!("Sentinel gate: PASS");
            } else {
                println!("Sentinel gate: FAIL");
                std::process::exit(1);
            }
        }
        Commands::Bench { model } => {
            if let Err(e) = run_benchmark_suite(&model).await {
                eprintln!("Benchmark suite failed: {e}");
                std::process::exit(1);
            }
        }
        Commands::Hello => {
            println!("Hello from A².");
        }
        Commands::Status => {
            println!("A² — Autopoietic Autocatalysis");
            println!("Version: {}", env!("CARGO_PKG_VERSION"));
            println!("Stage: 0 (bootstrap)");
            println!("Profile: B0 (human-gated)");
            println!();
            println!("Crates:");
            println!("  a2_core         — core types and traits");
            println!("  a2_constitution — constitutional kernel");
            println!("  a2_workcell     — workcell runtime");
            println!("  a2_membrane     — policy engine");
            println!("  a2_broker       — model routing");
            println!("  a2_eval         — seed evaluator + sentinels");
            println!("  a2_archive      — lineage store");
            println!("  a2_sensorium    — input ingestion");
            println!("  a2_raf          — causal graph diagnostics");
            println!("  a2d             — control plane daemon");
            println!("  a2ctl           — this CLI");
        }
    }
}

fn build_budget(max_tokens: u64, timeout_secs: u64) -> a2_core::protocol::Budget {
    a2_core::protocol::Budget {
        max_tokens,
        max_duration_secs: timeout_secs,
        max_calls: 20,
    }
}

async fn build_provider(model: &str) -> Box<dyn a2_core::traits::ModelProvider> {
    match model {
        "claude" => match a2_broker::broker::ClaudeProvider::new("claude-sonnet-4-6").await {
            Ok(p) => Box::new(a2_broker::adapt::CoreAdapter::new(p)),
            Err(e) => {
                eprintln!("Failed to init Claude provider: {e}");
                std::process::exit(1);
            }
        },
        "gemini" => match a2_broker::broker::GeminiProvider::new("gemini-3.1-pro-preview").await {
            Ok(p) => Box::new(a2_broker::adapt::CoreAdapter::new(p)),
            Err(e) => {
                eprintln!("Failed to init Gemini provider: {e}");
                std::process::exit(1);
            }
        },
        "codex" => match a2_broker::broker::CodexProvider::new("gpt-5.4").await {
            Ok(p) => Box::new(a2_broker::adapt::CoreAdapter::new(p)),
            Err(e) => {
                eprintln!("Failed to init Codex provider: {e}");
                std::process::exit(1);
            }
        },
        "opencode" => {
            match a2_broker::broker::OpenCodeProvider::new(
                a2_broker::broker::OpenCodeProvider::DEFAULT_MODEL_ID,
            )
            .await
            {
                Ok(p) => Box::new(a2_broker::adapt::CoreAdapter::new(p)),
                Err(e) => {
                    eprintln!("Failed to init OpenCode provider: {e}");
                    std::process::exit(1);
                }
            }
        }
        "pi" => {
            match a2_broker::broker::PiProvider::new(
                a2_broker::broker::PiProvider::DEFAULT_MODEL_ID,
            )
            .await
            {
                Ok(p) => Box::new(a2_broker::adapt::CoreAdapter::new(p)),
                Err(e) => {
                    eprintln!("Failed to init Pi provider: {e}");
                    std::process::exit(1);
                }
            }
        }
        other if other.starts_with("opencode/") => {
            let model_id = &other["opencode/".len()..];
            if model_id.is_empty() {
                eprintln!(
                    "Provider 'opencode/' requires a model id after the slash (e.g. \
                     'opencode/zai-coding-plan/glm-5.1', 'opencode/kimi-for-coding/k2p5', \
                     'opencode/minimax-coding-plan/MiniMax-M2.7')."
                );
                std::process::exit(1);
            }
            match a2_broker::broker::OpenCodeProvider::new(model_id).await {
                Ok(p) => Box::new(a2_broker::adapt::CoreAdapter::new(p)),
                Err(e) => {
                    eprintln!("Failed to init OpenCode provider ({model_id}): {e}");
                    std::process::exit(1);
                }
            }
        }
        other if other.starts_with("pi/") => {
            let model_id = &other["pi/".len()..];
            if model_id.is_empty() {
                eprintln!(
                    "Provider 'pi/' requires a model id after the slash (e.g. \
                     'pi/zai/glm-5.1')."
                );
                std::process::exit(1);
            }
            match a2_broker::broker::PiProvider::new(model_id).await {
                Ok(p) => Box::new(a2_broker::adapt::CoreAdapter::new(p)),
                Err(e) => {
                    eprintln!("Failed to init Pi provider ({model_id}): {e}");
                    std::process::exit(1);
                }
            }
        }
        other => {
            eprintln!("Unknown model provider: {other}");
            eprintln!(
                "Available: claude, gemini, codex, opencode, opencode/<model_id>, pi, pi/<model_id> \
                 (e.g. opencode/zai-coding-plan/glm-5.1, opencode/kimi-for-coding/k2p5, \
                 opencode/minimax-coding-plan/MiniMax-M2.7, pi/zai/glm-5.1)"
            );
            std::process::exit(1);
        }
    }
}

async fn run_benchmark_suite(model: &str) -> Result<(), String> {
    let bench_root = workspace_root().join("bench/tasks");
    let bench_tasks = load_benchmark_tasks(&bench_root)?;
    if bench_tasks.is_empty() {
        return Err(format!(
            "no benchmark tasks found in {}",
            bench_root.display()
        ));
    }

    let budget = build_budget(DEFAULT_BENCH_MAX_TOKENS, DEFAULT_BENCH_TIMEOUT_SECS);
    let ingester = a2_sensorium::ingest::Ingester::new(budget.clone());
    let provider = build_provider(model).await;
    let workspace = workspace_root();
    // Use bench-baseline tag so worktrees start from a known clean state.
    // The benchmark is purely observational — it never mutates the workspace.
    let catalyst = a2_workcell::worktree_catalyst::WorktreeCatalyst::with_base_ref(
        workspace.clone(),
        "bench-baseline",
    );
    let evaluator = a2_eval::seed::SeedEvaluator::new(DEFAULT_BENCH_MAX_TOKENS);
    let lineage_db = workspace.join("lineage.sqlite");
    let governor = match rusqlite::Connection::open(&lineage_db)
        .map_err(|e| format!("open lineage db: {e}"))
        .and_then(|conn| {
            a2_archive::SqliteLineageStore::new(conn)
                .map_err(|e| format!("init lineage store: {e}"))
        }) {
        Ok(store) => a2d::Governor::with_stagnation_detector(
            a2_core::id::GermlineVersion::new(),
            budget,
            a2d::StagnationDetector::new(DEFAULT_STAGNATION_WINDOW),
        )
        .with_lineage_store(std::sync::Arc::new(store)),
        Err(e) => {
            eprintln!("[lineage store unavailable: {e}]");
            a2d::Governor::with_stagnation_detector(
                a2_core::id::GermlineVersion::new(),
                budget,
                a2d::StagnationDetector::new(DEFAULT_STAGNATION_WINDOW),
            )
        }
    };

    let mut rows = Vec::with_capacity(bench_tasks.len());

    for bench_task in bench_tasks {
        eprintln!(
            "[bench] {} ({})",
            bench_task.task.title,
            bench_task.path.display()
        );

        let mut row = BenchSummaryRow {
            title: bench_task.task.title.clone(),
            model: requested_model(provider.as_ref()),
            tokens: 0,
            duration_secs: 0.0,
            promoted: false,
        };

        let mut task = ingester.from_human(&bench_task.task.title, &bench_task.task.description);
        task.verification_commands = vec![a2_core::protocol::TaskVerificationCommand {
            command: bench_task.verify.command.clone(),
            expect_exit: bench_task.verify.expect_exit,
        }];

        match run_task(&governor, task, &catalyst, provider.as_ref(), &evaluator).await {
            Ok(outcome) => {
                row = bench_summary_row(&bench_task.task.title, provider.as_ref(), &outcome);
            }
            Err(e) => {
                eprintln!("[bench run failed for {}: {e}]", bench_task.task.title);
            }
        }

        rows.push(row);
    }

    print!("{}", render_benchmark_summary_table(&rows));
    let promoted = rows.iter().filter(|row| row.promoted).count();
    println!("Score: {promoted}/{} tasks promoted", rows.len());

    Ok(())
}

fn load_benchmark_tasks(root: &Path) -> Result<Vec<BenchTaskCase>, String> {
    let mut entries = std::fs::read_dir(root)
        .map_err(|e| format!("read {}: {e}", root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("read {}: {e}", root.display()))?;
    entries.sort_by_key(|entry| entry.path());

    let mut tasks = Vec::new();
    for entry in entries {
        let path = entry.path();
        let is_toml = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("toml"))
            .unwrap_or(false);
        if !entry
            .file_type()
            .map_err(|e| format!("{}: {e}", path.display()))?
            .is_file()
            || !is_toml
        {
            continue;
        }

        let content =
            std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let parsed = toml::from_str::<BenchTaskFile>(&content)
            .map_err(|e| format!("parse {}: {e}", path.display()))?;
        tasks.push(BenchTaskCase {
            path,
            task: parsed.task,
            verify: parsed.verify,
            setup: parsed.setup,
        });
    }

    Ok(tasks)
}

async fn run_task(
    governor: &a2d::Governor,
    task: a2_core::protocol::TaskContract,
    catalyst: &dyn a2_core::traits::Catalyst,
    provider: &dyn a2_core::traits::ModelProvider,
    evaluator: &dyn a2_core::traits::Evaluator,
) -> a2_core::error::A2Result<a2d::GovernorOutcome> {
    governor.run_task(task, catalyst, provider, evaluator).await
}

fn bench_summary_row(
    title: &str,
    provider: &dyn a2_core::traits::ModelProvider,
    outcome: &a2d::GovernorOutcome,
) -> BenchSummaryRow {
    let model = outcome
        .result
        .patch
        .as_ref()
        .map(|patch| {
            format!(
                "{}/{}",
                patch.model_attribution.provider, patch.model_attribution.model
            )
        })
        .unwrap_or_else(|| requested_model(provider));

    BenchSummaryRow {
        title: title.to_string(),
        model,
        tokens: outcome.result.tokens_used,
        duration_secs: outcome.result.duration_secs,
        promoted: matches!(
            outcome.decision,
            a2_core::protocol::PromotionDecision::PromoteGermline { .. }
        ),
    }
}

fn run_summary_row(
    title: &str,
    provider: &dyn a2_core::traits::ModelProvider,
    outcome: &a2d::GovernorOutcome,
) -> RunSummaryRow {
    let model = outcome
        .result
        .patch
        .as_ref()
        .map(|patch| {
            format!(
                "{}/{}",
                patch.model_attribution.provider, patch.model_attribution.model
            )
        })
        .unwrap_or_else(|| requested_model(provider));

    RunSummaryRow {
        title: title.to_string(),
        model,
        tokens: outcome.result.tokens_used,
        duration_secs: outcome.result.duration_secs,
        decision: format_promotion_decision(&outcome.decision),
    }
}

fn requested_model(provider: &dyn a2_core::traits::ModelProvider) -> String {
    format!("{}/{}", provider.provider_id(), provider.model_id())
}

fn format_promotion_decision(decision: &a2_core::protocol::PromotionDecision) -> String {
    match decision {
        a2_core::protocol::PromotionDecision::Discard { reason } => {
            format!("discard ({reason})")
        }
        a2_core::protocol::PromotionDecision::MergeSomatic => "merge_somatic".into(),
        a2_core::protocol::PromotionDecision::PromoteGermline { mutation_scope } => {
            format!("promote_germline::{mutation_scope:?}")
        }
        a2_core::protocol::PromotionDecision::Rollback { target, reason } => {
            format!("rollback to {target} ({reason})")
        }
    }
}

fn render_summary_table(rows: &[RunSummaryRow]) -> String {
    let title_width = rows
        .iter()
        .map(|row| row.title.len())
        .max()
        .unwrap_or(5)
        .max("Title".len());
    let model_width = rows
        .iter()
        .map(|row| row.model.len())
        .max()
        .unwrap_or(5)
        .max("Model".len());
    let tokens_width = rows
        .iter()
        .map(|row| row.tokens.to_string().len())
        .max()
        .unwrap_or(6)
        .max("Tokens".len());
    let duration_width = rows
        .iter()
        .map(|row| format!("{:.1}s", row.duration_secs).len())
        .max()
        .unwrap_or(8)
        .max("Duration".len());
    let decision_width = rows
        .iter()
        .map(|row| row.decision.len())
        .max()
        .unwrap_or(8)
        .max("Decision".len());

    let mut out = String::new();
    out.push_str(&format!(
        "{:<title_width$}  {:<model_width$}  {:>tokens_width$}  {:>duration_width$}  {:<decision_width$}\n",
        "Title",
        "Model",
        "Tokens",
        "Duration",
        "Decision",
    ));
    out.push_str(&format!(
        "{}  {}  {}  {}  {}\n",
        "-".repeat(title_width),
        "-".repeat(model_width),
        "-".repeat(tokens_width),
        "-".repeat(duration_width),
        "-".repeat(decision_width),
    ));

    for row in rows {
        out.push_str(&format!(
            "{:<title_width$}  {:<model_width$}  {:>tokens_width$}  {:>duration_width$}  {:<decision_width$}\n",
            row.title,
            row.model,
            row.tokens,
            format!("{:.1}s", row.duration_secs),
            row.decision,
        ));
    }

    out
}

fn render_benchmark_summary_table(rows: &[BenchSummaryRow]) -> String {
    let title_width = rows
        .iter()
        .map(|row| row.title.len())
        .max()
        .unwrap_or(5)
        .max("Title".len());
    let model_width = rows
        .iter()
        .map(|row| row.model.len())
        .max()
        .unwrap_or(5)
        .max("Model".len());
    let tokens_width = rows
        .iter()
        .map(|row| row.tokens.to_string().len())
        .max()
        .unwrap_or(6)
        .max("Tokens".len());
    let duration_width = rows
        .iter()
        .map(|row| format!("{:.1}s", row.duration_secs).len())
        .max()
        .unwrap_or(8)
        .max("Duration".len());
    let promoted_width = "Promoted".len();

    let mut out = String::new();
    out.push_str(&format!(
        "{:<title_width$}  {:<model_width$}  {:>tokens_width$}  {:>duration_width$}  {:<promoted_width$}\n",
        "Title", "Model", "Tokens", "Duration", "Promoted",
    ));
    out.push_str(&format!(
        "{}  {}  {}  {}  {}\n",
        "-".repeat(title_width),
        "-".repeat(model_width),
        "-".repeat(tokens_width),
        "-".repeat(duration_width),
        "-".repeat(promoted_width),
    ));

    for row in rows {
        out.push_str(&format!(
            "{:<title_width$}  {:<model_width$}  {:>tokens_width$}  {:>duration_width$}  {:<promoted_width$}\n",
            row.title,
            row.model,
            row.tokens,
            format!("{:.1}s", row.duration_secs),
            yes_no(row.promoted),
        ));
    }

    out
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

enum ParsedRunInput {
    Plain(String),
    Json(RunInputTask),
}

fn parse_run_input(line: &str) -> ParsedRunInput {
    match serde_json::from_str::<RunInputTask>(line) {
        Ok(task) => ParsedRunInput::Json(task),
        Err(_) => ParsedRunInput::Plain(line.to_string()),
    }
}

fn task_from_run_input(
    ingester: &a2_sensorium::ingest::Ingester,
    input: ParsedRunInput,
) -> a2_core::protocol::TaskContract {
    match input {
        ParsedRunInput::Plain(description) => ingester.ingest(a2_sensorium::ingest::RawSignal {
            origin: "stdin".into(),
            content: description,
            risk_tier: a2_sensorium::ingest::RiskTier::Low,
            metadata: vec![],
        }),
        ParsedRunInput::Json(input) => {
            let title = input
                .task_id
                .as_deref()
                .filter(|task_id| !task_id.trim().is_empty())
                .unwrap_or_else(|| derive_run_title(&input.problem_statement));
            let mut task = ingester.from_human(title, &input.problem_statement);

            if let Some(task_id) = input.task_id.as_deref().filter(|id| !id.trim().is_empty()) {
                task.id = a2_core::id::TaskId::parse_str(task_id)
                    .unwrap_or_else(|_| a2_core::id::TaskId::from_external_key(task_id));
            }
            task.verification_commands = input
                .verification_commands
                .into_iter()
                .map(|verification| a2_core::protocol::TaskVerificationCommand {
                    command: verification.command,
                    expect_exit: verification.expect_exit,
                })
                .collect();

            task
        }
    }
}

fn derive_run_title(problem_statement: &str) -> &str {
    problem_statement
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("stdin task")
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AutopilotCandidate {
    id: String,
    title: String,
    description: String,
    source: String,
}

fn collect_autopilot_candidates(root: &Path) -> io::Result<Vec<AutopilotCandidate>> {
    let mut candidates = Vec::new();
    for rel_dir in ["todos", "docs/plans"] {
        collect_markdown_checklist_candidates(root, &root.join(rel_dir), &mut candidates)?;
    }

    for (index, task) in scan_workspace(root)?.into_iter().enumerate() {
        candidates.push(AutopilotCandidate {
            id: format!("autopilot:scan:{index}"),
            title: derive_run_title(&task).chars().take(96).collect(),
            description: format!(
                "Resolve the scanned source-code work item.\n\n{task}\n\nUpdate code and tests as needed. Run the smallest relevant verification before finishing."
            ),
            source: "scan".into(),
        });
    }

    Ok(candidates)
}

fn explicit_autopilot_candidates(
    root: &Path,
    tasks: &[String],
    task_files: &[String],
) -> io::Result<Vec<AutopilotCandidate>> {
    let mut candidates = Vec::new();
    for (index, task) in tasks.iter().enumerate() {
        candidates.push(explicit_autopilot_candidate(
            &format!("task:{index}"),
            task,
            &format!("--task[{index}]"),
        ));
    }

    for task_file in task_files {
        let path = PathBuf::from(task_file);
        let full_path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        let content = fs::read_to_string(&full_path)?;
        let source = format!("--task-file:{}", relative_path(root, &full_path));
        candidates.push(explicit_autopilot_candidate(&source, &content, &source));
    }

    Ok(candidates)
}

fn explicit_autopilot_candidate(key: &str, task: &str, source: &str) -> AutopilotCandidate {
    let title = derive_run_title(task).chars().take(96).collect::<String>();
    AutopilotCandidate {
        id: format!("autopilot:explicit:{}", stable_text_fingerprint(key, task)),
        title,
        description: format!(
            "Run the explicit autopilot task below as a self-improvement iteration for this repository.\n\nTask:\n{task}\n\nAs you work, improve the project where needed, update relevant docs/todos when complete, and run the smallest relevant verification before finishing."
        ),
        source: source.into(),
    }
}

fn stable_text_fingerprint(key: &str, value: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for byte in key.bytes().chain(std::iter::once(0)).chain(value.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

fn collect_markdown_checklist_candidates(
    root: &Path,
    dir: &Path,
    candidates: &mut Vec<AutopilotCandidate>,
) -> io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_markdown_checklist_candidates(root, &path, candidates)?;
        } else if file_type.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("md")
        {
            collect_markdown_file_candidates(root, &path, candidates)?;
        }
    }
    Ok(())
}

fn collect_markdown_file_candidates(
    root: &Path,
    path: &Path,
    candidates: &mut Vec<AutopilotCandidate>,
) -> io::Result<()> {
    let content = fs::read_to_string(path)?;
    let rel = relative_path(root, path);
    for (index, line) in content.lines().enumerate() {
        let Some(item) = unchecked_markdown_item(line) else {
            continue;
        };
        let line_number = index + 1;
        candidates.push(AutopilotCandidate {
            id: format!("autopilot:checklist:{}:{line_number}", rel),
            title: item.chars().take(96).collect(),
            description: format!(
                "Resolve unchecked project work item from {rel}:{line_number}.\n\nItem: {item}\n\nImplement the smallest safe improvement, update the checklist or handoff documentation when the work is complete, and run the smallest relevant verification before finishing."
            ),
            source: format!("{rel}:{line_number}"),
        });
    }
    Ok(())
}

fn unchecked_markdown_item(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let item = trimmed
        .strip_prefix("- [ ]")
        .or_else(|| trimmed.strip_prefix("* [ ]"))?
        .trim();
    if item.is_empty() {
        None
    } else {
        Some(item.to_string())
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn autopilot_run_dir(workspace_root: &Path, log_dir: &Path) -> PathBuf {
    let base = if log_dir.is_absolute() {
        log_dir.to_path_buf()
    } else {
        workspace_root.join(log_dir)
    };
    base.join("runs").join(format!(
        "run-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ))
}

fn autopilot_candidate_json(candidate: &AutopilotCandidate) -> serde_json::Value {
    serde_json::json!({
        "id": candidate.id,
        "title": candidate.title,
        "source": candidate.source,
    })
}

fn log_autopilot_event(run_dir: &Path, event: &str, payload: serde_json::Value) {
    if let Err(e) = append_autopilot_event(run_dir, event, payload) {
        eprintln!("[autopilot log failed: {e}]");
    }
}

fn append_autopilot_event(
    run_dir: &Path,
    event: &str,
    payload: serde_json::Value,
) -> io::Result<()> {
    fs::create_dir_all(run_dir)?;
    let path = run_dir.join("events.jsonl");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let record = serde_json::json!({
        "at": chrono::Utc::now().to_rfc3339(),
        "event": event,
        "payload": payload,
    });
    serde_json::to_writer(&mut file, &record).map_err(io::Error::other)?;
    writeln!(file)?;
    Ok(())
}

fn tracked_workspace_changes(root: &Path) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("git status: {e}"))?;
    if !output.status.success() {
        return Err(command_failure_message("git status --porcelain", &output));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn scan_workspace(root: &Path) -> io::Result<Vec<String>> {
    let mut tasks = Vec::new();
    scan_dir(root, root, &mut tasks)?;
    Ok(tasks)
}

fn scan_dir(root: &Path, dir: &Path, tasks: &mut Vec<String>) -> io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            if should_skip_dir(&path) {
                continue;
            }

            scan_dir(root, &path, tasks)?;
            continue;
        }

        if file_type.is_file() {
            scan_file(root, &path, tasks)?;
        }
    }

    Ok(())
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | "target")
    )
}

fn scan_file(root: &Path, path: &Path, tasks: &mut Vec<String>) -> io::Result<()> {
    let bytes = std::fs::read(path)?;
    if bytes.contains(&0) {
        return Ok(());
    }

    let content = String::from_utf8_lossy(&bytes);
    for (index, line) in content.lines().enumerate() {
        if let Some(marker) = find_scan_marker(line) {
            tasks.push(format_scan_task(root, path, index + 1, marker, line));
        }
    }

    Ok(())
}

fn find_scan_marker(line: &str) -> Option<&'static str> {
    let body = comment_body(line)?;

    if starts_with_marker(body, "TODO") {
        Some("TODO")
    } else if starts_with_marker(body, "FIXME") {
        Some("FIXME")
    } else {
        None
    }
}

fn comment_body(line: &str) -> Option<&str> {
    let (index, marker) = find_comment_start(line)?;
    Some(line[index + marker.len()..].trim_start())
}

fn starts_with_marker(body: &str, marker: &str) -> bool {
    body.strip_prefix(marker)
        .map(|rest| {
            rest.is_empty()
                || rest.starts_with(':')
                || rest.starts_with('-')
                || rest.starts_with(' ')
                || rest.starts_with('(')
        })
        .unwrap_or(false)
}

fn find_comment_start(line: &str) -> Option<(usize, &'static str)> {
    let bytes = line.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let markers = ["<!--", "///", "//!", "//", "/*", "--", "#"];

    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];

        if in_single {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'\'' {
                in_single = false;
            }

            index += 1;
            continue;
        }

        if in_double {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_double = false;
            }

            index += 1;
            continue;
        }

        if byte == b'\'' {
            in_single = true;
            index += 1;
            continue;
        }

        if byte == b'"' {
            in_double = true;
            index += 1;
            continue;
        }

        if let Some(marker) = markers.iter().find(|marker| {
            line.get(index..)
                .is_some_and(|tail| tail.starts_with(**marker))
        }) {
            return Some((index, *marker));
        }

        index += 1;
    }

    None
}

fn format_scan_task(
    root: &Path,
    path: &Path,
    line_number: usize,
    marker: &str,
    line: &str,
) -> String {
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string();
    let note = scan_note(marker, line);

    if note.is_empty() {
        format!("Resolve {marker} in {relative}:{line_number}")
    } else {
        format!("Resolve {marker} in {relative}:{line_number} - {note}")
    }
}

fn scan_note<'a>(marker: &str, line: &'a str) -> &'a str {
    comment_body(line)
        .and_then(|body| body.strip_prefix(marker))
        .unwrap_or("")
        .trim_start_matches(|c: char| c == ':' || c == '-' || c.is_whitespace())
        .trim()
}

#[derive(Clone, Debug)]
struct VerificationCommandFailure {
    label: String,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    message: String,
}

impl std::fmt::Display for VerificationCommandFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

struct ApplyVerifyOutcome {
    applied: bool,
    verified: bool,
    note: String,
    external_verification: a2_core::protocol::ExternalVerification,
}

fn compact_verification_excerpt(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let mut truncated = trimmed
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

fn extract_failing_tests(value: &str) -> Vec<String> {
    let mut tests = Vec::new();
    for line in value.lines().map(str::trim) {
        if let Some(name) = line
            .strip_prefix("test ")
            .and_then(|rest| rest.split_once(" ... FAILED"))
            .map(|(name, _)| name.trim())
            && !name.is_empty()
            && !tests.iter().any(|existing| existing == name)
        {
            tests.push(name.to_string());
        }
        if let Some(name) = line
            .strip_prefix("---- ")
            .and_then(|rest| rest.split_once(" stdout ----"))
            .map(|(name, _)| name.trim())
            && !name.is_empty()
            && !tests.iter().any(|existing| existing == name)
        {
            tests.push(name.to_string());
        }
    }
    tests
}

fn extract_failure_focus(value: &str, max_lines: usize) -> Vec<String> {
    let mut focused = Vec::new();
    for line in value.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let lower = line.to_ascii_lowercase();
        if lower.contains("failed")
            || lower.contains("failures:")
            || lower.contains("panicked at")
            || lower.contains("assertion failed")
            || lower.contains("assertion `")
            || lower.contains("left:")
            || lower.contains("right:")
        {
            let line = compact_verification_excerpt(line, 300);
            if !focused.iter().any(|existing| existing == &line) {
                focused.push(line);
            }
        }
        if focused.len() >= max_lines {
            break;
        }
    }
    focused
}

fn external_verification_from_failure(
    failure: &VerificationCommandFailure,
) -> a2_core::protocol::ExternalVerification {
    let combined = format!(
        "{}\n{}\n{}",
        failure.message, failure.stdout, failure.stderr
    );
    a2_core::protocol::ExternalVerification {
        passed: false,
        command: failure.label.clone(),
        exit_code: failure.exit_code,
        failing_tests: extract_failing_tests(&combined),
        failure_focus: extract_failure_focus(&combined, 12),
        stdout_excerpt: compact_verification_excerpt(&failure.stdout, 4_000),
        stderr_excerpt: compact_verification_excerpt(&failure.stderr, 4_000),
        verified_at: chrono::Utc::now(),
    }
}

fn external_verification_from_note(
    passed: bool,
    command: &str,
    exit_code: Option<i32>,
    note: &str,
) -> a2_core::protocol::ExternalVerification {
    a2_core::protocol::ExternalVerification {
        passed,
        command: command.into(),
        exit_code,
        failing_tests: extract_failing_tests(note),
        failure_focus: extract_failure_focus(note, 12),
        stdout_excerpt: String::new(),
        stderr_excerpt: if passed {
            String::new()
        } else {
            compact_verification_excerpt(note, 4_000)
        },
        verified_at: chrono::Utc::now(),
    }
}

fn apply_and_verify_patch(diff: &str, dir: &Path) -> ApplyVerifyOutcome {
    match try_apply_patch(diff, dir) {
        Ok(true) => match verify_and_rebuild() {
            Ok(true) => {
                let note = "[external verify: PASS] git apply and verify_and_rebuild exited 0.";
                ApplyVerifyOutcome {
                    applied: true,
                    verified: true,
                    note: note.into(),
                    external_verification: external_verification_from_note(
                        true,
                        "verify_and_rebuild",
                        Some(0),
                        note,
                    ),
                }
            }
            Ok(false) => {
                let note = "[external verify: FAIL] verify_and_rebuild exited 0 without reporting success.";
                ApplyVerifyOutcome {
                    applied: true,
                    verified: false,
                    note: note.into(),
                    external_verification: external_verification_from_note(
                        false,
                        "verify_and_rebuild",
                        Some(0),
                        note,
                    ),
                }
            }
            Err(e) => {
                let note = format!("[external verify: FAIL] verify_and_rebuild failed. {e}");
                ApplyVerifyOutcome {
                    applied: true,
                    verified: false,
                    note,
                    external_verification: external_verification_from_failure(&e),
                }
            }
        },
        Ok(false) => {
            let note =
                "[external verify: FAIL] git apply skipped because the patch diff was empty.";
            ApplyVerifyOutcome {
                applied: false,
                verified: false,
                note: note.into(),
                external_verification: external_verification_from_note(
                    false,
                    "git apply",
                    None,
                    note,
                ),
            }
        }
        Err(e) => {
            let note = format!("[external verify: FAIL] git apply failed. {e}");
            ApplyVerifyOutcome {
                applied: false,
                verified: false,
                external_verification: external_verification_from_note(
                    false,
                    "git apply",
                    None,
                    &note,
                ),
                note,
            }
        }
    }
}

/// Attempt to apply a promoted patch via `git apply`. Falls back to fuzzy
/// apply if strict check fails. Returns Ok(true) if applied,
/// Ok(false) if the diff was empty, Err if all strategies failed.
fn try_apply_patch(diff: &str, dir: &Path) -> Result<bool, String> {
    if diff.trim().is_empty() {
        return Ok(false);
    }

    let tmp = std::env::temp_dir().join(format!("a2_patch_{}.diff", std::process::id()));
    std::fs::write(&tmp, diff).map_err(|e| format!("write temp diff: {e}"))?;

    // The worktree catalyst creates the child worktree from workspace_root,
    // so `git diff` paths are relative to workspace_root. Run `git apply`
    // from workspace_root (the `dir` argument) so paths resolve correctly.
    let apply_dir = dir.to_path_buf();

    // Try strict apply first.
    let check = std::process::Command::new("git")
        .args(["apply", "--check"])
        .arg(&tmp)
        .current_dir(&apply_dir)
        .output()
        .map_err(|e| format!("git apply --check: {e}"))?;

    if check.status.success() {
        let apply = std::process::Command::new("git")
            .arg("apply")
            .arg(&tmp)
            .current_dir(&apply_dir)
            .output()
            .map_err(|e| format!("git apply: {e}"))?;
        let _ = std::fs::remove_file(&tmp);
        return if apply.status.success() {
            Ok(true)
        } else {
            Err(format!(
                "git apply failed: {}",
                String::from_utf8_lossy(&apply.stderr)
            ))
        };
    }

    // Strict failed — try fuzzy apply (tolerates whitespace/offset mismatches).
    let fuzzy = std::process::Command::new("git")
        .args(["apply", "--3way", "--whitespace=fix"])
        .arg(&tmp)
        .current_dir(&apply_dir)
        .output()
        .map_err(|e| format!("git apply --3way: {e}"))?;

    let _ = std::fs::remove_file(&tmp);

    if fuzzy.status.success() {
        Ok(true)
    } else {
        let stderr = String::from_utf8_lossy(&fuzzy.stderr);
        Err(format!("git apply failed (strict + fuzzy): {stderr}"))
    }
}

fn verify_and_rebuild() -> Result<bool, VerificationCommandFailure> {
    run_workspace_command("cargo", &["check"], "cargo check")?;
    run_workspace_command("cargo", &["test"], "cargo test")?;
    run_workspace_command(
        "cargo",
        &["clippy", "--all-targets", "--", "-D", "warnings"],
        "cargo clippy --all-targets -- -D warnings",
    )?;
    run_workspace_command(
        "cargo",
        &["build", "--release", "-p", "a2ctl", "-p", "a2d"],
        "cargo build --release -p a2ctl -p a2d",
    )?;
    Ok(true)
}

fn run_workspace_command(
    command: &str,
    args: &[&str],
    label: &str,
) -> Result<(), VerificationCommandFailure> {
    let output = std::process::Command::new(command)
        .args(args)
        .current_dir(workspace_root())
        .output()
        .map_err(|e| VerificationCommandFailure {
            label: label.into(),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            message: format!("{label}: {e}"),
        })?;

    if output.status.success() {
        return Ok(());
    }

    let mut failure = command_failure(label, &output);
    if let Err(revert_error) = revert_workspace() {
        failure
            .message
            .push_str(&format!("; rollback failed: {revert_error}"));
    }
    Err(failure)
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn revert_workspace() -> Result<(), String> {
    let output = std::process::Command::new("git")
        .args(["checkout", "."])
        .current_dir(workspace_root())
        .output()
        .map_err(|e| format!("git checkout .: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(command_failure_message("git checkout .", &output))
    }
}

fn command_failure(label: &str, output: &std::process::Output) -> VerificationCommandFailure {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = match (stderr.is_empty(), stdout.is_empty()) {
        (false, false) => format!("stdout:\n{stdout}\n\nstderr:\n{stderr}"),
        (false, true) => stderr.clone(),
        (true, false) => stdout.clone(),
        (true, true) => format!("exit status {}", output.status),
    };

    VerificationCommandFailure {
        label: label.into(),
        exit_code: output.status.code(),
        stdout,
        stderr,
        message: format!("{label} failed: {detail}"),
    }
}

fn command_failure_message(label: &str, output: &std::process::Output) -> String {
    command_failure(label, output).message
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn formats_promotion_decisions_for_summary_output() {
        let decision = a2_core::protocol::PromotionDecision::PromoteGermline {
            mutation_scope: a2_core::protocol::MutationScope::Prompt,
        };

        assert_eq!(
            format_promotion_decision(&decision),
            "promote_germline::Prompt"
        );
    }

    #[test]
    fn command_failure_message_includes_stdout_and_stderr() {
        let output = std::process::Command::new("sh")
            .args([
                "-c",
                "printf stdout-detail; printf stderr-detail >&2; exit 7",
            ])
            .output()
            .unwrap();

        let message = command_failure_message("test command", &output);

        assert!(message.contains("test command failed"));
        assert!(message.contains("stderr-detail"));
        assert!(message.contains("stdout-detail"));
        assert!(
            message.find("stdout-detail").unwrap() < message.find("stderr-detail").unwrap(),
            "stdout should be rendered first because test assertions usually land there"
        );
    }

    #[test]
    fn external_verification_from_command_failure_keeps_streams_and_failing_tests() {
        let output = std::process::Command::new("sh")
            .args([
                "-c",
                "printf 'running 2 tests\n'; printf 'test tests::hidden_regression ... FAILED\n'; printf 'failures:\n\n'; printf '---- tests::hidden_regression stdout ----\n'; printf 'thread panicked at src/main.rs:1: assertion failed: hidden()\n'; printf 'cargo stderr detail' >&2; exit 101",
            ])
            .output()
            .unwrap();

        let failure = command_failure("cargo test -p a2ctl", &output);
        let verification = external_verification_from_failure(&failure);

        assert!(!verification.passed);
        assert_eq!(verification.command, "cargo test -p a2ctl");
        assert_eq!(verification.exit_code, Some(101));
        assert_eq!(
            verification.failing_tests,
            vec!["tests::hidden_regression".to_string()]
        );
        assert!(
            verification
                .stdout_excerpt
                .contains("tests::hidden_regression")
        );
        assert!(verification.stderr_excerpt.contains("cargo stderr detail"));
        assert!(
            verification
                .failure_focus
                .iter()
                .any(|line| line.contains("assertion failed: hidden()")),
            "focus should preserve assertion lines: {:?}",
            verification.failure_focus
        );
    }

    #[test]
    fn renders_run_summary_table_headers_and_rows() {
        let output = render_summary_table(&[RunSummaryRow {
            title: "Fix auth bug".into(),
            model: "test/noop".into(),
            tokens: 150,
            duration_secs: 0.4,
            decision: "promote_germline::Prompt".into(),
        }]);

        assert!(output.contains("Title"));
        assert!(output.contains("Model"));
        assert!(output.contains("Fix auth bug"));
        assert!(output.contains("150"));
        assert!(output.contains("0.4s"));
    }

    #[test]
    fn renders_benchmark_summary_table_headers_and_rows() {
        let output = render_benchmark_summary_table(&[BenchSummaryRow {
            title: "Add fibonacci".into(),
            model: "claude/claude-sonnet-4-6".into(),
            tokens: 321,
            duration_secs: 1.2,
            promoted: true,
        }]);

        assert!(output.contains("Promoted"));
        assert!(output.contains("Add fibonacci"));
        assert!(output.contains("321"));
        assert!(output.contains("1.2s"));
        assert!(output.contains("yes"));
    }

    #[test]
    fn parses_json_run_input_tasks() {
        match parse_run_input(
            r#"{"task_id":"bench-1","problem_statement":"Implement feature","verification_commands":[{"command":"cargo test -p a2_core fibonacci","expect_exit":0}]}"#,
        ) {
            ParsedRunInput::Json(task) => {
                assert_eq!(task.task_id.as_deref(), Some("bench-1"));
                assert_eq!(task.problem_statement, "Implement feature");
                assert_eq!(task.verification_commands.len(), 1);
                assert_eq!(
                    task.verification_commands[0].command,
                    "cargo test -p a2_core fibonacci"
                );
            }
            ParsedRunInput::Plain(_) => panic!("expected json input"),
        }
    }

    #[test]
    fn json_run_input_task_id_pins_task_contract_id() {
        let ingester = a2_sensorium::ingest::Ingester::new(build_budget(50_000, 300));
        let first = task_from_run_input(
            &ingester,
            parse_run_input(r#"{"task_id":"bench-1","problem_statement":"Implement feature"}"#),
        );
        let second = task_from_run_input(
            &ingester,
            parse_run_input(r#"{"task_id":"bench-1","problem_statement":"Retry feature"}"#),
        );

        assert_eq!(first.id, second.id);
        assert_eq!(first.id, a2_core::id::TaskId::from_external_key("bench-1"));
    }

    #[test]
    fn json_run_input_accepts_existing_task_id_display_form() {
        let ingester = a2_sensorium::ingest::Ingester::new(build_budget(50_000, 300));
        let pinned = a2_core::id::TaskId::new();
        let task = task_from_run_input(
            &ingester,
            parse_run_input(&format!(
                r#"{{"task_id":"{}","problem_statement":"Retry feature"}}"#,
                pinned
            )),
        );

        assert_eq!(task.id, pinned);
    }

    #[test]
    fn json_run_input_sets_task_verification_commands() {
        let ingester = a2_sensorium::ingest::Ingester::new(build_budget(50_000, 300));
        let task = task_from_run_input(
            &ingester,
            parse_run_input(
                r#"{"task_id":"bench-1","problem_statement":"Retry feature","verification_commands":[{"command":"cargo test -p a2ctl hidden_case","expect_exit":0}]}"#,
            ),
        );

        assert_eq!(task.verification_commands.len(), 1);
        assert_eq!(
            task.verification_commands[0].command,
            "cargo test -p a2ctl hidden_case"
        );
        assert_eq!(task.verification_commands[0].expect_exit, 0);
    }

    #[test]
    fn derives_titles_from_problem_statement() {
        assert_eq!(
            derive_run_title("\n\nSolve this task\nwith details"),
            "Solve this task"
        );
        assert_eq!(derive_run_title(""), "stdin task");
    }

    #[test]
    fn parses_benchmark_task_toml() {
        let parsed = toml::from_str::<BenchTaskFile>(
            r#"
[task]
title = "Add a fibonacci function"
description = "Implement fibonacci"

[verify]
command = "cargo test -p a2_core fibonacci"
expect_exit = 0

[setup]
test_file = "crates/a2_core/src/lib.rs"
test_content = """
#[test]
fn test_fibonacci() {
    assert_eq!(fibonacci(10), 55);
}
"""
"#,
        )
        .unwrap();

        assert_eq!(parsed.task.title, "Add a fibonacci function");
        assert_eq!(parsed.verify.expect_exit, 0);
        assert_eq!(parsed.setup.test_file, "crates/a2_core/src/lib.rs");
        assert!(parsed.setup.test_content.contains("test_fibonacci"));
    }

    #[test]
    fn detects_unchecked_markdown_items_for_autopilot() {
        assert_eq!(
            unchecked_markdown_item("- [ ] Design continuous loop").as_deref(),
            Some("Design continuous loop")
        );
        assert_eq!(unchecked_markdown_item("- [x] Done"), None);
        assert_eq!(unchecked_markdown_item("plain text"), None);
    }

    #[test]
    fn autopilot_collects_checklist_and_scan_candidates() {
        let root = unique_test_dir("autopilot");
        std::fs::create_dir_all(root.join("todos")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("todos/work.md"),
            "# Work\n\n- [ ] Add resident loop\n",
        )
        .unwrap();
        std::fs::write(root.join("src/lib.rs"), "// TODO: wire liveness monitor\n").unwrap();

        let candidates = collect_autopilot_candidates(&root).unwrap();

        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.title == "Add resident loop")
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.title.contains("Resolve TODO in src/lib.rs"))
        );
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.id.starts_with("autopilot:"))
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_autopilot_task_becomes_stable_candidate() {
        let root = unique_test_dir("autopilot-explicit");
        let task = "Improve autopilot summaries".to_string();

        let first = explicit_autopilot_candidates(&root, std::slice::from_ref(&task), &[]).unwrap();
        let second = explicit_autopilot_candidates(&root, &[task], &[]).unwrap();

        assert_eq!(first.len(), 1);
        assert_eq!(first[0].title, "Improve autopilot summaries");
        assert_eq!(first[0].source, "--task[0]");
        assert_eq!(first[0].id, second[0].id);
        assert!(first[0].description.contains("explicit autopilot task"));
    }

    #[test]
    fn explicit_autopilot_task_file_becomes_candidate() {
        let root = unique_test_dir("autopilot-task-file");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("task.txt"), "Add task-file support\nwith details").unwrap();

        let candidates = explicit_autopilot_candidates(
            &root,
            &[],
            &[root.join("task.txt").display().to_string()],
        )
        .unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].title, "Add task-file support");
        assert!(candidates[0].source.contains("--task-file:"));
        assert!(candidates[0].description.contains("with details"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn autopilot_event_log_is_jsonl() {
        let root = unique_test_dir("autopilot-log");
        let run_dir = root.join(".a2/autopilot/runs/run-test");

        append_autopilot_event(&run_dir, "test_event", serde_json::json!({"ok": true})).unwrap();

        let content = std::fs::read_to_string(run_dir.join("events.jsonl")).unwrap();
        let line = content.lines().next().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(parsed["event"], "test_event");
        assert_eq!(parsed["payload"]["ok"], true);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extract_diff_files_extracts_touched_paths() {
        let diff = "diff --git a/crates/a2ctl/src/main.rs b/crates/a2ctl/src/main.rs\n\
                    --- a/crates/a2ctl/src/main.rs\n\
                    +++ b/crates/a2ctl/src/main.rs\n\
                    +@@ -1,3 +1,4 @@\n\
                    +use x;\n\
                    diff --git a/crates/a2_core/src/lib.rs b/crates/a2_core/src/lib.rs\n\
                    +++ b/crates/a2_core/src/lib.rs\n\
                    ++new line";

        let files = extract_diff_files(diff);
        assert_eq!(
            files,
            vec![
                "crates/a2ctl/src/main.rs".to_string(),
                "crates/a2_core/src/lib.rs".to_string(),
            ]
        );
    }

    #[test]
    fn extract_diff_files_skips_dev_null_and_duplicates() {
        let diff = "+++ b/dev/null\n\
                    +++ /dev/null\n\
                    +++ b/src/main.rs\n\
                    +++ b/src/main.rs";
        let files = extract_diff_files(diff);
        assert_eq!(files, vec!["src/main.rs".to_string()]);
    }

    #[test]
    fn extract_patch_stats_computes_lines_and_bytes() {
        let diff = "+++ b/foo.rs\n+line1\n+line2";
        let stats = extract_patch_stats(diff);
        assert_eq!(stats.files_touched, vec!["foo.rs".to_string()]);
        assert_eq!(stats.diff_lines, 3);
        assert_eq!(stats.diff_bytes, diff.len());
    }

    #[test]
    fn autopilot_run_summary_serializes_to_json() {
        let summary = AutopilotRunSummary {
            run_id: "run-20260525T120000Z".into(),
            workspace: "/tmp/workspace".into(),
            provider: "claude".into(),
            max_iterations: 3,
            started_at: "2026-05-25T12:00:00Z".into(),
            completed_at: "2026-05-25T12:01:00Z".into(),
            total_iterations: 2,
            total_tokens: 4500,
            total_duration_secs: 30.5,
            patches_produced: 1,
            applied_count: 1,
            verified_count: 0,
            iterations: vec![
                AutopilotIterationSummary {
                    iteration: 1,
                    task_id: "task-abc".into(),
                    candidate_id: "autopilot:scan:0".into(),
                    candidate_source: "scan".into(),
                    candidate_title: "Fix bug".into(),
                    model: "claude/claude-sonnet-4-6".into(),
                    tokens: 3000,
                    duration_secs: 20.0,
                    decision: "promote_germline::Prompt".into(),
                    patch_produced: true,
                    patch_stats: Some(PatchStats {
                        files_touched: vec!["src/main.rs".into()],
                        diff_lines: 12,
                        diff_bytes: 340,
                    }),
                    verifier_focus: vec!["assertion failed: x".into()],
                    apply_ok: true,
                    verify_ok: false,
                    apply_note: Some("[external verify: FAIL] cargo test exited 101".into()),
                },
                AutopilotIterationSummary {
                    iteration: 2,
                    task_id: String::new(),
                    candidate_id: "autopilot:scan:1".into(),
                    candidate_source: "scan".into(),
                    candidate_title: "Add test".into(),
                    model: "claude/claude-sonnet-4-6".into(),
                    tokens: 1500,
                    duration_secs: 10.5,
                    decision: "error: timeout".into(),
                    patch_produced: false,
                    patch_stats: None,
                    verifier_focus: vec![],
                    apply_ok: false,
                    verify_ok: false,
                    apply_note: None,
                },
            ],
        };

        let json = serde_json::to_string_pretty(&summary).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["run_id"], "run-20260525T120000Z");
        assert_eq!(parsed["total_iterations"], 2);
        assert_eq!(parsed["total_tokens"], 4500);
        assert_eq!(parsed["patches_produced"], 1);
        assert_eq!(parsed["iterations"][0]["candidate_source"], "scan");
        assert_eq!(parsed["iterations"][0]["model"], "claude/claude-sonnet-4-6");
        assert_eq!(
            parsed["iterations"][0]["patch_stats"]["files_touched"][0],
            "src/main.rs"
        );
        assert_eq!(parsed["iterations"][0]["patch_stats"]["diff_lines"], 12);
        assert_eq!(
            parsed["iterations"][0]["verifier_focus"][0],
            "assertion failed: x"
        );
        assert_eq!(parsed["iterations"][0]["apply_ok"], true);
        assert_eq!(parsed["iterations"][0]["verify_ok"], false);
        assert!(parsed["iterations"][1]["patch_stats"].is_null());
    }

    #[test]
    fn autopilot_run_summary_writes_to_file() {
        let root = unique_test_dir("autopilot-summary");
        let run_dir = root.join(".a2/autopilot/runs/run-test-summary");
        std::fs::create_dir_all(&run_dir).unwrap();

        let summary = AutopilotRunSummary {
            run_id: "run-test-summary".into(),
            workspace: "/tmp/ws".into(),
            provider: "claude".into(),
            max_iterations: 1,
            started_at: "2026-05-25T12:00:00Z".into(),
            completed_at: "2026-05-25T12:00:30Z".into(),
            total_iterations: 1,
            total_tokens: 500,
            total_duration_secs: 5.0,
            patches_produced: 1,
            applied_count: 1,
            verified_count: 1,
            iterations: vec![AutopilotIterationSummary {
                iteration: 1,
                task_id: "t1".into(),
                candidate_id: "c1".into(),
                candidate_source: "explicit".into(),
                candidate_title: "Test".into(),
                model: "test/noop".into(),
                tokens: 500,
                duration_secs: 5.0,
                decision: "promote_germline::Prompt".into(),
                patch_produced: true,
                patch_stats: None,
                verifier_focus: vec![],
                apply_ok: true,
                verify_ok: true,
                apply_note: None,
            }],
        };

        let summary_path = run_dir.join("run_summary.json");
        let json = serde_json::to_string_pretty(&summary).unwrap();
        std::fs::write(&summary_path, &json).unwrap();

        let content = std::fs::read_to_string(&summary_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["run_id"], "run-test-summary");
        assert_eq!(parsed["iterations"][0]["candidate_source"], "explicit");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scans_workspace_for_todo_and_fixme_comments() {
        let root = unique_test_dir("comments");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "// TODO: tighten parser\n# FIXME remove fallback\n",
        )
        .unwrap();

        let tasks = scan_workspace(&root).unwrap();

        assert_eq!(
            tasks,
            vec![
                "Resolve TODO in src/lib.rs:1 - tighten parser",
                "Resolve FIXME in src/lib.rs:2 - remove fallback",
            ]
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignores_non_task_mentions_inside_comments_and_strings() {
        assert!(find_scan_marker("/// Scan TODO/FIXME comments and emit tasks.").is_none());
        assert!(find_scan_marker("A² text before // TODO: real unicode line").is_some());
        assert!(find_scan_marker("let s = \"// TODO: not a comment\";").is_none());
        assert_eq!(
            find_scan_marker("let x = 1; // TODO: real comment"),
            Some("TODO")
        );
    }

    #[test]
    fn skips_target_and_git_directories_when_scanning() {
        let root = unique_test_dir("skip");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("src/main.rs"), "// TODO: keep me\n").unwrap();
        std::fs::write(root.join("target/generated.rs"), "// TODO: skip me\n").unwrap();
        std::fs::write(root.join(".git/HEAD"), "TODO: skip me\n").unwrap();

        let tasks = scan_workspace(&root).unwrap();

        assert_eq!(tasks, vec!["Resolve TODO in src/main.rs:1 - keep me"]);

        std::fs::remove_dir_all(root).unwrap();
    }

    fn unique_test_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "a2ctl_scan_{label}_{}_{}",
            std::process::id(),
            nonce
        ))
    }
}
