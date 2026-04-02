//! a2ctl — CLI for A² Autopoietic Autocatalysis.
//!
//! Stage 0 commands:
//!   a2ctl task "title" "description"   — create and run a task
//!   a2ctl run < tasks.txt              — run stdin tasks sequentially
//!   a2ctl bench                        — run the A² benchmark suite
//!   a2ctl sentinel                     — run the seed sentinel suite
//!   a2ctl hello                        — print a one-line greeting
//!   a2ctl status                       — show system health

use clap::{ArgAction, Parser, Subcommand};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::io::{self, BufRead};
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
        /// Auto-apply promoted patches before running verification.
        #[arg(long, action = ArgAction::Set, default_value_t = true)]
        apply: bool,
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
    applied: bool,
    verified: bool,
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

#[derive(Debug, Deserialize)]
struct BenchVerifySpec {
    command: String,
    expect_exit: i32,
}

#[derive(Debug, Deserialize)]
struct BenchSetupSpec {
    test_file: String,
    test_content: String,
}

struct BenchTaskCase {
    path: PathBuf,
    task: BenchTaskSpec,
    verify: BenchVerifySpec,
    setup: BenchSetupSpec,
}

const DEFAULT_STAGNATION_WINDOW: usize = 3;
const DEFAULT_BENCH_MAX_TOKENS: u64 = 50_000;
const DEFAULT_BENCH_TIMEOUT_SECS: u64 = 300;

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
            let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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
                                verify_and_rebuild()
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

            let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let catalyst =
                a2_workcell::worktree_catalyst::WorktreeCatalyst::new(workspace_root.clone());
            let evaluator = a2_eval::seed::SeedEvaluator::new(max_tokens);
            let governor = a2d::Governor::with_stagnation_detector(
                a2_core::id::GermlineVersion::new(),
                budget,
                a2d::StagnationDetector::new(DEFAULT_STAGNATION_WINDOW),
            );

            let mut rows = Vec::new();
            let mut task_index: usize = 0;

            for line in io::stdin().lock().lines() {
                let description = match line {
                    Ok(line) => line,
                    Err(e) => {
                        eprintln!("Failed to read stdin: {e}");
                        std::process::exit(1);
                    }
                };

                let description = description.trim();
                if description.is_empty() {
                    continue;
                }

                let task = ingester.ingest(a2_sensorium::ingest::RawSignal {
                    origin: "stdin".into(),
                    content: description.to_string(),
                    risk_tier: a2_sensorium::ingest::RiskTier::Low,
                    metadata: vec![],
                });

                let title = task.title.clone();

                let p = providers[task_index % providers.len()].as_ref();
                task_index += 1;

                match run_task(&governor, task, &catalyst, p, &evaluator).await {
                    Ok(outcome) => {
                        if apply
                            && let a2_core::protocol::PromotionDecision::PromoteGermline { .. } =
                                &outcome.decision
                            && let Some(patch) = &outcome.result.patch
                        {
                            match try_apply_patch(&patch.diff, &workspace_root).and_then(
                                |applied| {
                                    if applied {
                                        verify_and_rebuild()
                                    } else {
                                        Ok(false)
                                    }
                                },
                            ) {
                                Ok(true) => eprintln!("[applied and rebuilt: {title}]"),
                                Ok(false) => {}
                                Err(e) => eprintln!("[apply/rebuild failed for {title}: {e}]"),
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

            if rows.is_empty() {
                eprintln!("No task descriptions provided on stdin.");
                std::process::exit(1);
            }

            print!("{}", render_summary_table(&rows));
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

                let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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
                                            verify_and_rebuild()
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
        Commands::Bench { model, apply } => {
            if let Err(e) = run_benchmark_suite(&model, apply).await {
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
        other => {
            eprintln!("Unknown model provider: {other}");
            eprintln!("Available: claude, gemini, codex, opencode");
            std::process::exit(1);
        }
    }
}

async fn run_benchmark_suite(model: &str, apply: bool) -> Result<(), String> {
    assert_benchmark_workspace_clean()?;
    let baseline_untracked = workspace_untracked_files()?;

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
    let catalyst = a2_workcell::worktree_catalyst::WorktreeCatalyst::new(workspace.clone());
    let evaluator = a2_eval::seed::SeedEvaluator::new(DEFAULT_BENCH_MAX_TOKENS);
    let governor = a2d::Governor::with_stagnation_detector(
        a2_core::id::GermlineVersion::new(),
        budget,
        a2d::StagnationDetector::new(DEFAULT_STAGNATION_WINDOW),
    );

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
            applied: false,
            verified: false,
        };

        let task_result = append_benchmark_test_content(&bench_task)
            .map(|()| ingester.from_human(&bench_task.task.title, &bench_task.task.description));

        match task_result {
            Ok(task) => match run_task(&governor, task, &catalyst, provider.as_ref(), &evaluator)
                .await
            {
                Ok(outcome) => {
                    row = bench_summary_row(&bench_task.task.title, provider.as_ref(), &outcome);

                    if apply
                        && let a2_core::protocol::PromotionDecision::PromoteGermline { .. } =
                            &outcome.decision
                        && let Some(patch) = &outcome.result.patch
                    {
                        match try_apply_patch(&patch.diff, &workspace) {
                            Ok(true) => {
                                row.applied = true;
                                match run_workspace_shell_command(&bench_task.verify.command) {
                                    Ok(output) => {
                                        let actual_exit = output.status.code().unwrap_or(-1);
                                        if actual_exit == bench_task.verify.expect_exit {
                                            row.verified = true;
                                        } else {
                                            eprintln!(
                                                "[verify failed for {}: expected exit {}, got {}; {}]",
                                                bench_task.task.title,
                                                bench_task.verify.expect_exit,
                                                actual_exit,
                                                command_failure_message(
                                                    &bench_task.verify.command,
                                                    &output
                                                )
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "[verify command failed for {}: {e}]",
                                            bench_task.task.title
                                        );
                                    }
                                }
                            }
                            Ok(false) => {}
                            Err(e) => {
                                eprintln!("[apply failed for {}: {e}]", bench_task.task.title);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[bench run failed for {}: {e}]", bench_task.task.title);
                }
            },
            Err(e) => {
                eprintln!("[bench setup failed for {}: {e}]", bench_task.task.title);
            }
        }

        cleanup_benchmark_workspace(&baseline_untracked)
            .map_err(|e| format!("cleanup after {}: {e}", bench_task.task.title))?;

        rows.push(row);
    }

    print!("{}", render_benchmark_summary_table(&rows));
    let verified = rows.iter().filter(|row| row.verified).count();
    println!("Score: {verified}/{} tasks verified", rows.len());

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

fn append_benchmark_test_content(task: &BenchTaskCase) -> Result<(), String> {
    let relative = Path::new(&task.setup.test_file);
    if relative.is_absolute() {
        return Err(format!(
            "benchmark setup file must be relative: {}",
            task.setup.test_file
        ));
    }

    let path = workspace_root().join(relative);
    let mut content =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&task.setup.test_content);
    if !content.ends_with('\n') {
        content.push('\n');
    }

    std::fs::write(&path, content).map_err(|e| format!("write {}: {e}", path.display()))
}

fn assert_benchmark_workspace_clean() -> Result<(), String> {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no", "--", "."])
        .current_dir(workspace_root())
        .output()
        .map_err(|e| format!("git status --porcelain --untracked-files=no -- .: {e}"))?;

    if !output.status.success() {
        return Err(command_failure_message(
            "git status --porcelain --untracked-files=no -- .",
            &output,
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        Ok(())
    } else {
        Err(
            "bench requires a clean tracked workspace because it resets changes with `git checkout .` between tasks"
                .into(),
        )
    }
}

fn cleanup_benchmark_workspace(baseline_untracked: &BTreeSet<String>) -> Result<(), String> {
    revert_workspace()?;

    let current_untracked = workspace_untracked_files()?;
    let leaked = current_untracked
        .difference(baseline_untracked)
        .cloned()
        .collect::<Vec<_>>();
    if leaked.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "new untracked files remain after cleanup: {}",
            leaked.join(", ")
        ))
    }
}

fn workspace_untracked_files() -> Result<BTreeSet<String>, String> {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all", "--", "."])
        .current_dir(workspace_root())
        .output()
        .map_err(|e| format!("git status --porcelain --untracked-files=all -- .: {e}"))?;

    if !output.status.success() {
        return Err(command_failure_message(
            "git status --porcelain --untracked-files=all -- .",
            &output,
        ));
    }

    Ok(parse_untracked_files(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn parse_untracked_files(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("?? ").map(ToOwned::to_owned))
        .collect()
}

fn run_workspace_shell_command(command: &str) -> Result<std::process::Output, String> {
    std::process::Command::new("sh")
        .args(["-lc", command])
        .current_dir(workspace_root())
        .output()
        .map_err(|e| format!("{command}: {e}"))
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
        applied: false,
        verified: false,
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
    let applied_width = "Applied".len();
    let verified_width = "Verified".len();

    let mut out = String::new();
    out.push_str(&format!(
        "{:<title_width$}  {:<model_width$}  {:>tokens_width$}  {:>duration_width$}  {:<applied_width$}  {:<verified_width$}\n",
        "Title",
        "Model",
        "Tokens",
        "Duration",
        "Applied",
        "Verified",
    ));
    out.push_str(&format!(
        "{}  {}  {}  {}  {}  {}\n",
        "-".repeat(title_width),
        "-".repeat(model_width),
        "-".repeat(tokens_width),
        "-".repeat(duration_width),
        "-".repeat(applied_width),
        "-".repeat(verified_width),
    ));

    for row in rows {
        out.push_str(&format!(
            "{:<title_width$}  {:<model_width$}  {:>tokens_width$}  {:>duration_width$}  {:<applied_width$}  {:<verified_width$}\n",
            row.title,
            row.model,
            row.tokens,
            format!("{:.1}s", row.duration_secs),
            yes_no(row.applied),
            yes_no(row.verified),
        ));
    }

    out
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
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

        if let Some(marker) = markers
            .iter()
            .find(|marker| line[index..].starts_with(**marker))
        {
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

/// Attempt to apply a promoted patch via `git apply`. Falls back to fuzzy
/// apply if strict check fails. Returns Ok(true) if applied,
/// Ok(false) if the diff was empty, Err if all strategies failed.
fn try_apply_patch(diff: &str, dir: &Path) -> Result<bool, String> {
    if diff.trim().is_empty() {
        return Ok(false);
    }

    let tmp = std::env::temp_dir().join(format!("a2_patch_{}.diff", std::process::id()));
    std::fs::write(&tmp, diff).map_err(|e| format!("write temp diff: {e}"))?;

    // The worktree catalyst runs `git diff` from the worktree root, which
    // mirrors the workspace passed to WorktreeCatalyst::new. Diff paths are
    // therefore relative to that workspace root (the `dir` argument here),
    // so git apply must run from that same directory.
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

fn verify_and_rebuild() -> Result<bool, String> {
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

fn run_workspace_command(command: &str, args: &[&str], label: &str) -> Result<(), String> {
    let output = std::process::Command::new(command)
        .args(args)
        .current_dir(workspace_root())
        .output()
        .map_err(|e| format!("{label}: {e}"))?;

    if output.status.success() {
        return Ok(());
    }

    let failure = command_failure_message(label, &output);
    match revert_workspace() {
        Ok(()) => Err(failure),
        Err(revert_error) => Err(format!("{failure}; rollback failed: {revert_error}")),
    }
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

fn command_failure_message(label: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };

    format!("{label} failed: {detail}")
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
            applied: true,
            verified: false,
        }]);

        assert!(output.contains("Applied"));
        assert!(output.contains("Verified"));
        assert!(output.contains("Add fibonacci"));
        assert!(output.contains("321"));
        assert!(output.contains("1.2s"));
        assert!(output.contains("yes"));
        assert!(output.contains("no"));
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
    fn parses_untracked_files_from_porcelain_output() {
        let files = parse_untracked_files(
            "?? a2-autopoietic-autocatalysis/bench/tasks/001_add_function.toml\n?? a2-autopoietic-autocatalysis/bench/tasks/002_error_variant.toml\n",
        );

        assert_eq!(files.len(), 2);
        assert!(files.contains("a2-autopoietic-autocatalysis/bench/tasks/001_add_function.toml"));
        assert!(files.contains("a2-autopoietic-autocatalysis/bench/tasks/002_error_variant.toml"));
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
