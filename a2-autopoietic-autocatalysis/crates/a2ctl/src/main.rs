//! a2ctl — CLI for A² Autopoietic Autocatalysis.
//!
//! Stage 0 commands:
//!   a2ctl task "title" "description"   — create and run a task
//!   a2ctl run < tasks.txt              — run stdin tasks sequentially
//!   a2ctl sentinel                     — run the seed sentinel suite
//!   a2ctl hello                        — print a one-line greeting
//!   a2ctl status                       — show system health

use clap::{Parser, Subcommand};
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
    Scan {
        /// Workspace root path (defaults to current directory).
        #[arg(long, default_value = ".")]
        workspace: String,
    },
    /// Run the seed sentinel suite.
    Sentinel {
        /// Workspace root path (defaults to current directory).
        #[arg(long, default_value = ".")]
        workspace: String,
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

const DEFAULT_STAGNATION_WINDOW: usize = 3;

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
            let catalyst = a2_workcell::worktree_catalyst::WorktreeCatalyst::new(workspace_root);
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
                        match try_apply_patch(&patch.diff) {
                            Ok(true) => println!("--- Applied ---"),
                            Ok(false) => println!("[empty diff, nothing to apply]"),
                            Err(e) => eprintln!("[apply failed: {e}]"),
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
            let catalyst = a2_workcell::worktree_catalyst::WorktreeCatalyst::new(workspace_root);
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
                            match try_apply_patch(&patch.diff) {
                                Ok(true) => eprintln!("[applied: {title}]"),
                                Ok(false) => {}
                                Err(e) => eprintln!("[apply failed for {title}: {e}]"),
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
        Commands::Scan { workspace } => match scan_workspace(Path::new(&workspace)) {
            Ok(tasks) => {
                for task in tasks {
                    println!("{task}");
                }
            }
            Err(e) => {
                eprintln!("Scan failed: {e}");
                std::process::exit(1);
            }
        },
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

async fn run_task(
    governor: &a2d::Governor,
    task: a2_core::protocol::TaskContract,
    catalyst: &dyn a2_core::traits::Catalyst,
    provider: &dyn a2_core::traits::ModelProvider,
    evaluator: &dyn a2_core::traits::Evaluator,
) -> a2_core::error::A2Result<a2d::GovernorOutcome> {
    governor.run_task(task, catalyst, provider, evaluator).await
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
fn try_apply_patch(diff: &str) -> Result<bool, String> {
    if diff.trim().is_empty() {
        return Ok(false);
    }

    let tmp = std::env::temp_dir().join(format!("a2_patch_{}.diff", std::process::id()));
    std::fs::write(&tmp, diff).map_err(|e| format!("write temp diff: {e}"))?;

    // Try strict apply first.
    let check = std::process::Command::new("git")
        .args(["apply", "--check"])
        .arg(&tmp)
        .output()
        .map_err(|e| format!("git apply --check: {e}"))?;

    if check.status.success() {
        let apply = std::process::Command::new("git")
            .arg("apply")
            .arg(&tmp)
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
