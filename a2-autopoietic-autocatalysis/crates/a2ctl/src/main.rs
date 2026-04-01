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
        /// Model provider/model (e.g., "claude" or "gemini").
        #[arg(long, default_value = "claude")]
        model: String,
        /// Dry run: create task but don't execute.
        #[arg(long)]
        dry_run: bool,
    },
    /// Read task descriptions from stdin and run them sequentially.
    Run {
        /// Maximum token budget per task.
        #[arg(long, default_value = "50000")]
        max_tokens: u64,
        /// Model provider/model (e.g., "claude" or "gemini").
        #[arg(long, default_value = "claude")]
        model: String,
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
            model,
            dry_run,
        } => {
            let budget = default_budget(max_tokens);

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
            let catalyst = a2_workcell::catalyst::GeneralistCatalyst::new();
            let evaluator = a2_eval::seed::SeedEvaluator::new(max_tokens);
            let governor = a2d::Governor::new(a2_core::id::GermlineVersion::new(), budget);

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
                }
                Err(e) => {
                    eprintln!("Task failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Run { max_tokens, model } => {
            let budget = default_budget(max_tokens);
            let ingester = a2_sensorium::ingest::Ingester::new(budget.clone());
            let provider = build_provider(&model).await;
            let catalyst = a2_workcell::catalyst::GeneralistCatalyst::new();
            let evaluator = a2_eval::seed::SeedEvaluator::new(max_tokens);
            let governor = a2d::Governor::new(a2_core::id::GermlineVersion::new(), budget);

            let mut rows = Vec::new();

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

                match run_task(&governor, task, &catalyst, provider.as_ref(), &evaluator).await {
                    Ok(outcome) => rows.push(run_summary_row(&title, provider.as_ref(), &outcome)),
                    Err(e) => rows.push(RunSummaryRow {
                        title,
                        model: requested_model(provider.as_ref()),
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

fn default_budget(max_tokens: u64) -> a2_core::protocol::Budget {
    a2_core::protocol::Budget {
        max_tokens,
        max_duration_secs: 300,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
