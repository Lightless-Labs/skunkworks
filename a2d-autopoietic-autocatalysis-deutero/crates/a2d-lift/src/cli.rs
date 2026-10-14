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
  a2d lift validate <suite-dir> [--jobs <n>]
  a2d lift verify-ledger <ledger.jsonl>
  a2d lift run --experiment <id> --suite <dir> --model codex/<model> --out <dir>
               --max-invocations <n> --max-wall-clock-secs <s>
               (--registered-at <git-sha> --expect-suite-hash <sha256> | --unregistered)
               [--arms A,B,D,C] [--k <n>] [--replicates <n>] [--jobs <n>]
               [--primary B|D] [--seed <n>] [--min-families <n>] [--effort <level>]
               [--record-hidden-output]";

pub fn cli_main(args: &[String], arm_c: Option<&dyn ArmC>) -> i32 {
    let result = match args.first().map(String::as_str) {
        Some("hash") => args
            .get(1)
            .ok_or_else(|| USAGE.to_string())
            .and_then(|dir| suite_hash(Path::new(dir)))
            .map(|hash| println!("{hash}")),
        Some("run") => run(&args[1..], arm_c),
        Some("validate") => validate(&args[1..]),
        Some("verify-ledger") => args
            .get(1)
            .ok_or_else(|| USAGE.to_string())
            .and_then(|path| crate::ledger::verify_chain(Path::new(path)))
            .map(|(records, head)| {
                println!(
                    "ledger OK: {records} records, head {}",
                    head.unwrap_or_default()
                )
            }),
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
        if name == "record-hidden-output" || name == "unregistered" {
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

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
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

/// The A²D project root: `A2D_PROJECT_ROOT`, else the current directory.
/// Must contain the controller, or the git checks would match nothing.
fn project_root() -> Result<PathBuf, String> {
    let root = std::env::var_os("A2D_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir().map_err(|e| e.to_string())?);
    if !root.join("crates/a2d-lift/src/lib.rs").is_file() {
        return Err(format!(
            "{} is not the A²D project root (run from it or set A2D_PROJECT_ROOT)",
            root.display()
        ));
    }
    Ok(root)
}

/// Check the frozen paths against the registered commit. With
/// `--registered-at`, any committed or uncommitted change to
/// [`CONTROLLER_PATHS`] since that commit voids the registration.
fn registration_git_state(root: &Path, registered_at: Option<&str>) -> Result<Value, String> {
    git(
        root,
        &["ls-files", "--error-unmatch", "crates/a2d-lift/src/lib.rs"],
    )
    .map_err(|e| format!("controller is not tracked by git: {e}"))?;
    let head = git(root, &["rev-parse", "HEAD"])?;
    let mut status_args = vec!["status", "--porcelain", "--"];
    status_args.extend(CONTROLLER_PATHS);
    let dirty = git(root, &status_args)?;
    if let Some(sha) = registered_at {
        let mut diff_args = vec!["diff", "--name-only", sha, "HEAD", "--"];
        diff_args.extend(CONTROLLER_PATHS);
        let changed = git(root, &diff_args)?;
        if !changed.is_empty() {
            return Err(format!(
                "registration void: frozen paths changed since {sha}:\n{changed}"
            ));
        }
        if !dirty.is_empty() {
            return Err(format!(
                "registration void: uncommitted changes to frozen paths:\n{dirty}"
            ));
        }
    }
    Ok(json!({
        "head": head,
        "registered_at": registered_at,
        "frozen_paths": CONTROLLER_PATHS,
        "frozen_paths_dirty": !dirty.is_empty(),
    }))
}

/// The running binary and toolchain, for the registration record.
fn build_provenance() -> Value {
    let exe_sha256 = std::env::current_exe()
        .ok()
        .and_then(|path| fs::read(path).ok())
        .map(|bytes| crate::ledger::sha256_hex(&bytes));
    let rustc = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    json!({"exe_sha256": exe_sha256, "rustc": rustc})
}

fn parse_arms(spec: &str) -> Result<Vec<Arm>, String> {
    let arms: Vec<Arm> = spec.split(',').map(Arm::parse).collect::<Result<_, _>>()?;
    let mut seen = std::collections::BTreeSet::new();
    if let Some(dup) = arms.iter().find(|arm| !seen.insert(**arm)) {
        return Err(format!("arm {} listed twice", dup.label()));
    }
    Ok(arms)
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
    let arms = parse_arms(flags.get("arms").map(String::as_str).unwrap_or("A"))?;
    let k: usize = if arms.iter().any(|&a| a != Arm::A) {
        number(&flags, "k").map_err(|e| format!("{e} (required with arms B, C or D)"))?
    } else {
        flags.get("k").map_or(Ok(1), |_| number(&flags, "k"))?
    };
    let replicates: u32 = flags
        .get("replicates")
        .map_or(Ok(1), |_| number(&flags, "replicates"))?;
    let jobs: usize = flags
        .get("jobs")
        .map_or(Ok(1), |_| number(&flags, "jobs"))?;
    let seed: u64 = flags
        .get("seed")
        .map_or(Ok(0), |_| number(&flags, "seed"))?;
    let min_families: usize = flags
        .get("min-families")
        .map_or(Ok(crate::DEFAULT_MIN_FAMILIES), |_| {
            number(&flags, "min-families")
        })?;
    if k == 0 || replicates == 0 || jobs == 0 {
        return Err("--k, --replicates and --jobs must be at least 1".to_string());
    }
    let primary = flags.get("primary").map(|p| Arm::parse(p)).transpose()?;
    if let Some(p) = primary
        && !matches!(p, Arm::B | Arm::D)
    {
        return Err("--primary must be B or D".to_string());
    }
    if arms.contains(&Arm::C)
        && (arms.contains(&Arm::B) || arms.contains(&Arm::D))
        && primary.is_none_or(|p| !arms.contains(&p))
    {
        return Err("--primary B|D (one of the arms run) is required when C runs".to_string());
    }
    let unregistered = flags.contains_key("unregistered");
    let registered_at = flags.get("registered-at").map(String::as_str);
    let expected_hash = flags.get("expect-suite-hash").map(String::as_str);
    if !unregistered && (registered_at.is_none() || expected_hash.is_none()) {
        return Err(
            "--registered-at and --expect-suite-hash are required (or --unregistered for development)"
                .to_string(),
        );
    }

    let root = project_root()?;
    let git_state = registration_git_state(&root, registered_at)?;
    let suite = load_suite(&suite_dir, expected_hash)?;
    let provider = model_provider(model, effort)?;

    fs::create_dir_all(&out_dir).map_err(|e| format!("create {}: {e}", out_dir.display()))?;
    let mut ledger = Ledger::create(&out_dir.join("ledger.jsonl"))
        .map_err(|e| format!("create ledger in {}: {e}", out_dir.display()))?;
    ledger
        .append(
            "registration",
            json!({
                "experiment": experiment,
                "unregistered": unregistered,
                "suite_dir": suite_dir.display().to_string(),
                "suite_hash": suite.hash,
                "tasks": suite.tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
                "families": suite.families(),
                "arms": arms.iter().map(|a| a.label()).collect::<Vec<_>>(),
                "primary_comparator": primary.map(|a| a.label()),
                "k": k,
                "replicates": replicates,
                "jobs": jobs,
                "seed": seed,
                "min_families": min_families,
                "model": model,
                "effort": effort,
                "max_invocations": max_invocations,
                "max_wall_clock_secs": max_wall_clock_secs,
                "provider_timeout_secs": std::env::var("A2D_PROVIDER_TIMEOUT_SECS").ok(),
                "git": git_state,
                "build": build_provenance(),
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
        jobs,
        seed,
        primary,
        min_families,
        record_hidden_output: flags.contains_key("record-hidden-output"),
    };
    println!(
        "a2d lift {experiment}: {} tasks × {} replicates × arms {:?}, k={k}, model {model}",
        suite.tasks.len(),
        replicates,
        arms.iter().map(|a| a.label()).collect::<Vec<_>>()
    );
    let result = run_arms(&plan, &suite, provider, &meter, &out_dir, arm_c)?;
    for o in &result.outcomes {
        println!(
            "  {} {} r{}: {} ({}/{} hidden, {} invocations, {} coder){}",
            o.arm.label(),
            o.task,
            o.replicate,
            if o.hidden.all_passed { "PASS" } else { "FAIL" },
            o.hidden.passed,
            o.hidden.expected,
            o.invocations,
            o.coder_calls,
            o.void_reason
                .as_deref()
                .map(|r| format!(" VOID: {r}"))
                .unwrap_or_default()
        );
    }
    let summary = summarize(&result, &plan);
    let pretty = serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?;
    fs::write(out_dir.join("summary.json"), format!("{pretty}\n")).map_err(|e| e.to_string())?;
    let mut m = crate::meter::lock(&meter);
    m.ledger
        .append("summary", summary)
        .map_err(|e| e.to_string())?;
    println!("{pretty}");
    println!("ledger head: {}", m.ledger.head().unwrap_or_default());
    Ok(())
}

/// `a2d lift validate <suite>`: print per-task oracle verdicts and the suite
/// hash, never test content. Fails if any task is rejected.
fn validate(args: &[String]) -> Result<(), String> {
    let dir = args.first().ok_or_else(|| USAGE.to_string())?;
    let flags = parse_flags(&args[1..])?;
    let jobs: usize = flags
        .get("jobs")
        .map_or(Ok(1), |_| number(&flags, "jobs"))?;
    let report = crate::validate::validate_suite(Path::new(dir), jobs.max(1))?;
    for verdict in &report.verdicts {
        println!("{verdict}");
    }
    let rejected = report.verdicts.iter().filter(|v| !v.accepted()).count();
    println!(
        "suite {} — {} tasks, {} accepted, {} rejected",
        report.hash,
        report.verdicts.len(),
        report.verdicts.len() - rejected,
        rejected
    );
    if rejected > 0 {
        return Err(format!("{rejected} task(s) rejected"));
    }
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
