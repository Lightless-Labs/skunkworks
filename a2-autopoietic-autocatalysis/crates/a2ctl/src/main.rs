//! a2ctl — CLI for A² Autopoietic Autocatalysis.
//!
//! Stage 0 commands:
//!   a2ctl task "title" "description"   — create and run a task
//!   a2ctl sentinel                     — run the seed sentinel suite
//!   a2ctl status                       — show system health

use clap::{Parser, Subcommand};

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
    /// Run the seed sentinel suite.
    Sentinel {
        /// Workspace root path (defaults to current directory).
        #[arg(long, default_value = ".")]
        workspace: String,
    },
    /// Show system status and health.
    Status,
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
            let budget = a2_core::protocol::Budget {
                max_tokens,
                max_duration_secs: 300,
                max_calls: 20,
            };

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

            // Build model provider.
            let provider: Box<dyn a2_core::traits::ModelProvider> = match model.as_str() {
                "claude" => match a2_broker::broker::ClaudeProvider::new("claude-sonnet-4-6").await
                {
                    Ok(p) => Box::new(a2_broker::adapt::CoreAdapter::new(p)),
                    Err(e) => {
                        eprintln!("Failed to init Claude provider: {e}");
                        std::process::exit(1);
                    }
                },
                "gemini" => {
                    match a2_broker::broker::GeminiProvider::new("gemini-3.1-pro-preview").await {
                        Ok(p) => Box::new(a2_broker::adapt::CoreAdapter::new(p)),
                        Err(e) => {
                            eprintln!("Failed to init Gemini provider: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                "codex" => match a2_broker::broker::CodexProvider::new("gpt-5.4").await {
                    Ok(p) => Box::new(a2_broker::adapt::CoreAdapter::new(p)),
                    Err(e) => {
                        eprintln!("Failed to init Codex provider: {e}");
                        std::process::exit(1);
                    }
                },
                "opencode" => {
                    match a2_broker::broker::OpenCodeProvider::new("zai-coding-plan/glm-5.1").await
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
            };

            // Build components.
            let catalyst = a2_workcell::catalyst::GeneralistCatalyst::new();
            let evaluator = a2_eval::seed::SeedEvaluator::new(max_tokens);

            // Run through governor.
            println!("Executing...");
            println!();

            let governor = a2d::Governor::new(
                a2_core::id::GermlineVersion::new(),
                budget,
            );

            match governor
                .run_task(task, &catalyst, provider.as_ref(), &evaluator)
                .await
            {
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
