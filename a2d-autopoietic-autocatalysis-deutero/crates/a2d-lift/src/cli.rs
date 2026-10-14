//! `a2d lift run|hash` argument handling, registration checks and model lanes.

use crate::ledger::Ledger;
use crate::meter::Meter;
use crate::task::{load_suite, suite_hash};
use crate::{Arm, ArmC, CONTROLLER_PATHS, RunPlan, run_arms, summarize};
use a2d_core::provider::Provider;
use a2d_providers::cli::CliProvider;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

const USAGE: &str = "\
usage:
  a2d lift hash <suite-dir>
  a2d lift run --experiment <id> --suite <dir> --model codex/<model> --out <dir>
               --max-invocations <n> --max-wall-clock-secs <s>
               [--arms A,B,D,C] [--k <n>] [--replicates <n>] [--effort <level>]
               [--expect-suite-hash <sha256>] [--registered-at <git-sha>]
               [--record-hidden-output]";

/// Entry point for `a2d lift ...`; returns the process exit code.
pub fn cli_main(args: &[String], arm_c: Option<&dyn ArmC>) -> i32 {
    let result = match args.first().map(String::as_str) {
        Some("hash") => args
            .get(1)
            .ok_or_else(|| USAGE.to_string())
            .and_then(|dir| suite_hash(Path::new(dir)))
            .map(|hash| println!("{hash}")),
        Some("run") => run(&args[1..], arm_c),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("a2d lift: {error}");
            1
        }
    }
}

fn parse_flags(args: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut flags = BTreeMap::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let name = arg
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected argument {arg:?}\n{USAGE}"))?;
        if name == "record-hidden-output" {
            flags.insert(name.to_string(), "true".to_string());
            continue;
        }
        let value = iter
            .next()
            .ok_or_else(|| format!("--{name} needs a value"))?;
        flags.insert(name.to_string(), value.clone());
    }
    Ok(flags)
}

fn required<'a>(flags: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, String> {
    flags
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{name}\n{USAGE}"))
}

fn number<T: std::str::FromStr>(flags: &BTreeMap<String, String>, name: &str) -> Result<T, String> {
    required(flags, name)?
        .parse()
        .map_err(|_| format!("--{name} must be a number"))
}

/// Build the coder lane named by `--model`.
pub fn model_provider(spec: &str, effort: Option<&str>) -> Result<Arc<dyn Provider>, String> {
    match spec.split_once('/') {
        Some(("codex", model)) if !model.is_empty() => {
            Ok(Arc::new(CliProvider::codex(model, effort)))
        }
        _ => Err(format!(
            "unsupported --model {spec:?}; supported: codex/<model> (e.g. codex/gpt-6-luna)"
        )),
    }
}

fn git(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Check the controller against the registered commit. With
/// `--registered-at`, any committed or uncommitted change to
/// [`CONTROLLER_PATHS`] since that commit voids the registration.
fn registration_git_state(registered_at: Option<&str>) -> Result<Value, String> {
    let head = git(&["rev-parse", "HEAD"])?;
    let mut status_args = vec!["status", "--porcelain", "--"];
    status_args.extend(CONTROLLER_PATHS);
    let dirty = git(&status_args)?;
    if let Some(sha) = registered_at {
        let mut diff_args = vec!["diff", "--name-only", sha, "HEAD", "--"];
        diff_args.extend(CONTROLLER_PATHS);
        let changed = git(&diff_args)?;
        if !changed.is_empty() {
            return Err(format!(
                "registration void: controller changed since {sha}:\n{changed}"
            ));
        }
        if !dirty.is_empty() {
            return Err(format!(
                "registration void: uncommitted controller changes:\n{dirty}"
            ));
        }
    }
    Ok(json!({
        "head": head,
        "registered_at": registered_at,
        "controller_paths": CONTROLLER_PATHS,
        "controller_dirty": !dirty.is_empty(),
    }))
}

fn run(args: &[String], arm_c: Option<&dyn ArmC>) -> Result<(), String> {
    let flags = parse_flags(args)?;
    let experiment = required(&flags, "experiment")?;
    let suite_dir = PathBuf::from(required(&flags, "suite")?);
    let model = required(&flags, "model")?;
    let out_dir = PathBuf::from(required(&flags, "out")?);
    let max_invocations: usize = number(&flags, "max-invocations")?;
    let max_wall_clock_secs: u64 = number(&flags, "max-wall-clock-secs")?;
    let effort = flags.get("effort").map(String::as_str);
    let k: usize = flags.get("k").map_or(Ok(1), |_| number(&flags, "k"))?;
    let replicates: u32 = flags
        .get("replicates")
        .map_or(Ok(1), |_| number(&flags, "replicates"))?;
    let mut arms: Vec<Arm> = flags
        .get("arms")
        .map(String::as_str)
        .unwrap_or("A")
        .split(',')
        .map(Arm::parse)
        .collect::<Result<_, _>>()?;
    arms.dedup();
    if k == 0 || replicates == 0 {
        return Err("--k and --replicates must be at least 1".to_string());
    }

    let git_state = registration_git_state(flags.get("registered-at").map(String::as_str))?;
    let suite = load_suite(
        &suite_dir,
        flags.get("expect-suite-hash").map(String::as_str),
    )?;
    let provider = model_provider(model, effort)?;

    fs::create_dir_all(&out_dir).map_err(|e| format!("create {}: {e}", out_dir.display()))?;
    let mut ledger = Ledger::create(&out_dir.join("ledger.jsonl"))
        .map_err(|e| format!("create ledger in {}: {e}", out_dir.display()))?;
    ledger
        .append(
            "registration",
            json!({
                "experiment": experiment,
                "suite_dir": suite_dir.display().to_string(),
                "suite_hash": suite.hash,
                "tasks": suite.tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
                "families": suite.families(),
                "arms": arms.iter().map(|a| a.label()).collect::<Vec<_>>(),
                "k": k,
                "replicates": replicates,
                "model": model,
                "effort": effort,
                "max_invocations": max_invocations,
                "max_wall_clock_secs": max_wall_clock_secs,
                "provider_timeout_secs": std::env::var("A2D_PROVIDER_TIMEOUT_SECS").ok(),
                "git": git_state,
                "argv": args,
            }),
        )
        .map_err(|e| e.to_string())?;
    let meter = Meter::new(
        ledger,
        max_invocations,
        Duration::from_secs(max_wall_clock_secs),
        out_dir.join("invocations"),
    );
    let plan = RunPlan {
        arms: arms.clone(),
        k,
        replicates,
        record_hidden_output: flags.contains_key("record-hidden-output"),
    };
    println!(
        "a2d lift {experiment}: {} tasks × {} replicates × arms {:?}, k={k}, model {model}",
        suite.tasks.len(),
        replicates,
        arms.iter().map(|a| a.label()).collect::<Vec<_>>()
    );
    let outcomes = run_arms(&plan, &suite, provider, &meter, &out_dir, arm_c)?;
    for o in &outcomes {
        println!(
            "  {} {} r{}: {} ({}/{} hidden, {} invocations)",
            o.arm.label(),
            o.task,
            o.replicate,
            if o.hidden.all_passed { "PASS" } else { "FAIL" },
            o.hidden.passed,
            o.hidden.expected,
            o.invocations
        );
    }
    let summary = summarize(&outcomes, &arms);
    let pretty = serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?;
    fs::write(out_dir.join("summary.json"), format!("{pretty}\n")).map_err(|e| e.to_string())?;
    crate::meter::lock(&meter)
        .ledger
        .append("summary", summary)
        .map_err(|e| e.to_string())?;
    println!("{pretty}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_lane_accepts_codex_only() {
        assert_eq!(
            model_provider("codex/gpt-6-luna", Some("low"))
                .unwrap()
                .name(),
            "codex/gpt-6-luna"
        );
        assert!(model_provider("opencode/kimi", None).is_err());
        assert!(model_provider("codex/", None).is_err());
    }

    #[test]
    fn flags_parse_values_and_switches() {
        let args: Vec<String> = ["--k", "4", "--record-hidden-output", "--arms", "A,B"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let flags = parse_flags(&args).unwrap();
        assert_eq!(flags["k"], "4");
        assert_eq!(flags["arms"], "A,B");
        assert!(flags.contains_key("record-hidden-output"));
        assert!(parse_flags(&["--k".to_string()]).is_err());
    }
}
