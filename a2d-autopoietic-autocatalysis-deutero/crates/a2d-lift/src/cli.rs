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
  a2d lift manifest <run-dir>        # content-free pre-score anchor (JSON)
  a2d lift pool --out <summary.json> <ledger> <ledger> [...]   # replicate pooling
  a2d lift score --out <run-dir> --suite <dir> --expect-head <ledger-head> [--jobs <n>] [--record-hidden-output]
  a2d lift run --experiment <id> --suite <dir> --model codex/<model> --out <dir>
               --max-invocations <n> --max-wall-clock-secs <s>
               (--registered-at <git-sha> --expect-suite-hash <sha256> | --unregistered)
               [--arms A,B,D,C] [--k <n>] [--replicates <n>] [--jobs <n>]
               [--primary B|D] [--seed <n>] [--min-families <n>] [--max-excluded-share <f>]
               [--effort <level>]
               [--record-hidden-output] [--defer-scoring]
               [--harness <path> | --harness <name>=<path> ...]  # C or C:<name>
               [--d-note <name>=<path> ...]                   # D:<name>
               [--claim <x>><y>,<x>><y>,...]                  # all pairs supported
               # variant names: [a-z0-9][a-z0-9-]*; list each in --arms
  a2d lift compare --out <file> <ledger>:<arm> <ledger>:<arm>   # paired, across runs";

pub fn cli_main(args: &[String], arm_c: Option<&dyn ArmC>) -> i32 {
    let result = match args.first().map(String::as_str) {
        Some("hash") => args
            .get(1)
            .ok_or_else(|| USAGE.to_string())
            .and_then(|dir| suite_hash(Path::new(dir)))
            .map(|hash| println!("{hash}")),
        Some("run") => run(&args[1..], arm_c),
        Some("validate") => validate(&args[1..]),
        Some("score") => score(&args[1..]),
        Some("pool") => pool(&args[1..]),
        Some("compare") => compare(&args[1..]),
        Some("manifest") => args
            .get(1)
            .ok_or_else(|| USAGE.to_string())
            .and_then(|dir| manifest(Path::new(dir)))
            .and_then(|m| serde_json::to_string_pretty(&m).map_err(|e| e.to_string()))
            .map(|text| println!("{text}")),
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
    Ok(parse_options(args)?.into_iter().collect())
}

fn parse_options(args: &[String]) -> Result<Vec<(String, String)>, String> {
    let mut flags = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let name = arg
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected argument {arg:?}\n{USAGE}"))?;
        if matches!(
            name,
            "record-hidden-output" | "unregistered" | "defer-scoring"
        ) {
            flags.push((name.to_string(), "true".to_string()));
            continue;
        }
        let value = iter
            .next()
            .ok_or_else(|| format!("--{name} needs a value"))?;
        flags.push((name.to_string(), value.clone()));
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
    let codex = Command::new("codex")
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    json!({"exe_sha256": exe_sha256, "rustc": rustc, "codex": codex})
}

fn parse_arms(spec: &str) -> Result<Vec<Arm>, String> {
    let arms: Vec<Arm> = spec.split(',').map(Arm::parse).collect::<Result<_, _>>()?;
    let mut seen = std::collections::BTreeSet::new();
    if let Some(dup) = arms.iter().find(|arm| !seen.insert((*arm).clone())) {
        return Err(format!("arm {} listed twice", dup.label()));
    }
    Ok(arms)
}

fn run(args: &[String], arm_c: Option<&dyn ArmC>) -> Result<(), String> {
    run_with_provider(args, arm_c, &project_root()?, None)
}

pub(super) fn run_with_provider(
    args: &[String],
    arm_c: Option<&dyn ArmC>,
    root: &Path,
    provider: Option<Arc<dyn Provider>>,
) -> Result<(), String> {
    let options = parse_options(args)?;
    let flags: BTreeMap<String, String> = options.iter().cloned().collect();
    let experiment = required(&flags, "experiment")?;
    let suite_dir = PathBuf::from(required(&flags, "suite")?);
    let model = required(&flags, "model")?;
    let out_dir = PathBuf::from(required(&flags, "out")?);
    let max_invocations: usize = number(&flags, "max-invocations")?;
    let max_wall_clock_secs: u64 = number(&flags, "max-wall-clock-secs")?;
    let effort = flags.get("effort").map(String::as_str);
    let arms = parse_arms(flags.get("arms").map(String::as_str).unwrap_or("A"))?;
    let k: usize = if arms.iter().any(|a| *a != Arm::A) {
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
    let max_excluded_share: f64 = flags
        .get("max-excluded-share")
        .map_or(Ok(crate::DEFAULT_MAX_EXCLUDED_SHARE), |_| {
            number(&flags, "max-excluded-share")
        })?;
    if k == 0 || replicates == 0 || jobs == 0 {
        return Err("--k, --replicates and --jobs must be at least 1".to_string());
    }
    let primary = flags.get("primary").map(|p| Arm::parse(p)).transpose()?;
    if let Some(p) = &primary
        && !matches!(p, Arm::B | Arm::D)
    {
        return Err("--primary must be B or D".to_string());
    }
    if !flags.contains_key("claim")
        && arms.iter().any(Arm::is_c)
        && (arms.contains(&Arm::B) || arms.contains(&Arm::D))
        && primary.as_ref().is_none_or(|p| !arms.contains(p))
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

    let mut harness = json!({});
    let mut harness_sha256 = None;
    let mut harnesses = BTreeMap::new();
    let mut registered_harnesses = BTreeMap::new();
    let mut d_notes = BTreeMap::new();
    let mut registered_notes = BTreeMap::new();
    for (flag, spec) in &options {
        if !matches!(flag.as_str(), "harness" | "d-note") {
            continue;
        }
        let (name, path) = match spec.split_once('=') {
            Some((name, path)) => {
                Arm::validate_name(name)?;
                (Some(name), path)
            }
            None if flag == "harness" => (None, spec.as_str()),
            None => return Err("--d-note expects <name>=<path>".to_string()),
        };
        let arm = match (flag.as_str(), name) {
            ("harness", Some(name)) => Arm::CVariant(name.to_string()),
            ("harness", None) => Arm::C,
            (_, Some(name)) => Arm::DNote(name.to_string()),
            _ => unreachable!(),
        };
        if !arms.contains(&arm) {
            return Err(format!("{} defined but not listed in --arms", arm.label()));
        }
        let bytes = fs::read(path).map_err(|e| format!("read {flag} {path}: {e}"))?;
        let sha256 = crate::ledger::sha256_hex(&bytes);
        if flag == "harness" {
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|e| format!("harness {path} is not JSON: {e}"))?;
            arm_c
                .ok_or("arm C is not wired")?
                .validate_harness(&value)
                .map_err(|e| format!("harness {} rejected: {e}", arm.label()))?;
            if let Some(name) = name {
                if harnesses.insert(name.to_string(), value.clone()).is_some() {
                    return Err(format!("harness {name} defined twice"));
                }
                registered_harnesses.insert(
                    name.to_string(),
                    json!({"harness": value, "harness_sha256": sha256}),
                );
            } else {
                if harness_sha256.is_some() {
                    return Err("plain C harness defined twice".to_string());
                }
                harness = value;
                harness_sha256 = Some(sha256);
            }
        } else {
            let name = name.expect("notes are named");
            let text = String::from_utf8(bytes.clone())
                .map_err(|e| format!("note {path} is not UTF-8: {e}"))?;
            if d_notes.insert(name.to_string(), text).is_some() {
                return Err(format!("D note {name} defined twice"));
            }
            registered_notes.insert(
                name.to_string(),
                json!({"sha256": sha256, "bytes": bytes.len()}),
            );
        }
    }
    for arm in &arms {
        let defined = match arm {
            Arm::CVariant(name) => harnesses.contains_key(name),
            Arm::DNote(name) => d_notes.contains_key(name),
            _ => true,
        };
        if !defined {
            return Err(format!("undefined variant {}", arm.label()));
        }
    }
    if arms.iter().any(Arm::is_c) && arm_c.is_none() {
        return Err("arm C is not wired".to_string());
    }
    let claim = flags
        .get("claim")
        .map(|spec| crate::parse_claim(spec, &arms))
        .transpose()?;
    let git_state = registration_git_state(root, registered_at)?;
    let suite = load_suite(&suite_dir, expected_hash)?;
    let provider = match provider {
        Some(provider) => provider,
        None => model_provider(model, effort)?,
    };

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
                "primary_comparator": primary.as_ref().map(|a| a.label()),
                "k": k,
                "replicates": replicates,
                "jobs": jobs,
                "seed": seed,
                "min_families": min_families,
                "max_excluded_share": max_excluded_share,
                "harness": harness,
                "harness_sha256": harness_sha256,
                "harnesses": registered_harnesses,
                "d_notes": registered_notes,
                "claim": flags.get("claim"),
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
        max_excluded_share,
        record_hidden_output: flags.contains_key("record-hidden-output"),
        harness,
        harnesses,
        d_notes,
        claim,
        defer_scoring: flags.contains_key("defer-scoring"),
    };
    println!(
        "a2d lift {experiment}: {} tasks × {} replicates × arms {:?}, k={k}, model {model}",
        suite.tasks.len(),
        replicates,
        arms.iter().map(|a| a.label()).collect::<Vec<_>>()
    );
    let result = run_arms(&plan, &suite, provider, &meter, &out_dir, arm_c)?;
    if !result.scored {
        println!(
            "generation complete{}; scoring deferred. Commit {} then run: a2d lift score --out {} --suite {}",
            if result.aborted { " (ABORTED)" } else { "" },
            out_dir.display(),
            out_dir.display(),
            suite_dir.display()
        );
        return Ok(());
    }
    report(&result, &plan, &meter, &out_dir)
}

/// `a2d lift score`: hidden scoring for a run generated with --defer-scoring.
fn score(args: &[String]) -> Result<(), String> {
    let flags = parse_flags(args)?;
    let out_dir = PathBuf::from(required(&flags, "out")?);
    let suite_dir = PathBuf::from(required(&flags, "suite")?);
    let jobs: usize = flags
        .get("jobs")
        .map_or(Ok(1), |_| number(&flags, "jobs"))?;
    let expected_head = required(&flags, "expect-head")?;
    // The controller, oracle and treatment stay frozen through scoring.
    let registration = read_registration(&out_dir.join("ledger.jsonl"))?;
    let registered_at = registration["git"]["registered_at"].as_str();
    if registered_at.is_none() && registration["unregistered"] != true {
        return Err("registration records no commit; cannot verify the freeze".to_string());
    }
    registration_git_state(&project_root()?, registered_at)?;
    let (result, plan, meter) = crate::score_deferred(
        &out_dir,
        &suite_dir,
        jobs.max(1),
        flags.contains_key("record-hidden-output"),
        Some(expected_head),
    )?;
    report(&result, &plan, &meter, &out_dir)
}

fn ledger_records(path: &Path) -> Result<Vec<Value>, String> {
    crate::ledger::verify_chain(path)?;
    fs::read_to_string(path)
        .map_err(|e| format!("read {}: {e}", path.display()))?
        .lines()
        .map(|line| serde_json::from_str(line).map_err(|e| e.to_string()))
        .collect()
}

fn read_registration(path: &Path) -> Result<Value, String> {
    ledger_records(path)?
        .into_iter()
        .find(|r| r["kind"] == "registration")
        .ok_or_else(|| "ledger has no registration".to_string())
}

/// Content-free anchor of a generated run: registration (minus argv and
/// paths), ledger head and record count, and one hash per submission.
/// Committed before scoring; `score --expect-head` checks against it.
fn manifest(run_dir: &Path) -> Result<Value, String> {
    let path = run_dir.join("ledger.jsonl");
    let (records_count, head) = crate::ledger::verify_chain(&path)?;
    let records = ledger_records(&path)?;
    if records.iter().any(|r| r["kind"] == "score") {
        return Err("run already has scores; a manifest anchors an unscored run".to_string());
    }
    let mut registration = records
        .iter()
        .find(|r| r["kind"] == "registration")
        .cloned()
        .ok_or("ledger has no registration")?;
    if let Some(object) = registration.as_object_mut() {
        object.remove("argv");
        object.remove("suite_dir");
    }
    let submissions: Vec<Value> = records
        .iter()
        .filter(|r| r["kind"] == "submission")
        .map(|r| {
            json!({
                "arm": r["arm"], "task": r["task"], "replicate": r["replicate"],
                "artifact_sha256": r["artifact_sha256"], "invocations": r["invocations"],
                "void_reason": r["void_reason"], "tool_calls": r["tool_calls"],
            })
        })
        .collect();
    Ok(json!({
        "registration": registration,
        "ledger_records": records_count,
        "ledger_head": head,
        "generation_complete": records.iter().find(|r| r["kind"] == "generation_complete"),
        "submissions": submissions,
    }))
}

/// `a2d lift pool`: classify scored runs of one design as replicates.
fn pool(args: &[String]) -> Result<(), String> {
    let (out, ledgers) = match args {
        [flag, out, rest @ ..] if flag == "--out" && rest.len() >= 2 => (
            PathBuf::from(out),
            rest.iter().map(PathBuf::from).collect::<Vec<_>>(),
        ),
        _ => return Err(USAGE.to_string()),
    };
    let (result, plan) = crate::pool_ledgers(&ledgers)?;
    let mut summary = summarize(&result, &plan);
    summary["pooled_ledgers"] = json!(
        ledgers
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
    );
    let pretty = serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?;
    fs::write(&out, format!("{pretty}\n")).map_err(|e| e.to_string())?;
    println!("{pretty}");
    Ok(())
}

/// `a2d lift compare`: paired comparison of one arm in one scored run
/// against one arm in another run on the same suite (e.g. two arm-C
/// harnesses). Pairs by task and replicate; whole-family bootstrap.
fn compare(args: &[String]) -> Result<(), String> {
    let (out, x, y) = match args {
        [flag, out, x, y] if flag == "--out" => (PathBuf::from(out), x, y),
        _ => return Err(USAGE.to_string()),
    };
    let side = |spec: &str| -> Result<(PathBuf, Arm), String> {
        let (path, arm) = spec
            .split_once(".jsonl:")
            .ok_or(format!("expected <ledger>:<arm>, got {spec}"))?;
        Ok((PathBuf::from(format!("{path}.jsonl")), Arm::parse(arm)?))
    };
    let ((x_path, x_arm), (y_path, y_arm)) = (side(x)?, side(y)?);
    let (x_reg, x_out) = crate::load_scored(&x_path)?;
    let (y_reg, y_out) = crate::load_scored(&y_path)?;
    if x_reg["suite_hash"] != y_reg["suite_hash"] {
        return Err("the two runs used different suites".to_string());
    }
    let paired: Vec<crate::stats::PairedOutcome> = x_out
        .iter()
        .filter(|o| o.arm == x_arm && o.void_reason.is_none())
        .filter_map(|ox| {
            y_out
                .iter()
                .find(|oy| {
                    oy.arm == y_arm
                        && oy.task == ox.task
                        && oy.replicate == ox.replicate
                        && oy.void_reason.is_none()
                })
                .map(|oy| crate::stats::PairedOutcome {
                    family: ox.family.clone(),
                    task: ox.task.clone(),
                    replicate: ox.replicate,
                    x_pass: ox.hidden.all_passed,
                    y_pass: oy.hidden.all_passed,
                })
        })
        .collect();
    let s = crate::stats::paired_summary(&paired, 10_000, 0x5eed_a2d0);
    let min_families = x_reg["min_families"].as_u64().unwrap_or(10) as usize;
    let tokens = |outcomes: &[crate::Outcome], arm: &Arm| {
        outcomes
            .iter()
            .filter(|o| &o.arm == arm)
            .map(|o| o.tokens)
            .sum::<usize>()
    };
    let invocations = |outcomes: &[crate::Outcome], arm: &Arm| {
        outcomes
            .iter()
            .filter(|o| &o.arm == arm)
            .map(|o| o.invocations)
            .sum::<usize>()
    };
    let result = json!({
        "x": {"ledger": x_path.display().to_string(), "arm": x_arm.label(), "harness": comparison_harness(&x_reg, &x_arm),
              "tokens": tokens(&x_out, &x_arm), "invocations": invocations(&x_out, &x_arm)},
        "y": {"ledger": y_path.display().to_string(), "arm": y_arm.label(), "harness": comparison_harness(&y_reg, &y_arm),
              "tokens": tokens(&y_out, &y_arm), "invocations": invocations(&y_out, &y_arm)},
        "n": s.n, "families": s.families, "x_rate": s.x_rate, "y_rate": s.y_rate,
        "lift": s.lift, "ci95": [s.ci_low, s.ci_high],
        "discordant_x_only": s.discordant_x_only, "discordant_y_only": s.discordant_y_only,
        "family_sign_flip_p": crate::stats::family_sign_flip_p(&paired, 10_000, 0x5eed_a2d0),
        "class": format!("{:?}", crate::stats::classify_guarded(&s, crate::MIN_EFFECT, min_families)),
    });
    let pretty = serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?;
    fs::write(&out, format!("{pretty}\n")).map_err(|e| e.to_string())?;
    println!("{pretty}");
    Ok(())
}

fn comparison_harness(registration: &Value, arm: &Arm) -> Value {
    match arm {
        Arm::CVariant(name) => registration["harnesses"][name]["harness"].clone(),
        _ => registration["harness"].clone(),
    }
}

/// Print outcomes, write summary.json and append the summary to the ledger.
fn report(
    result: &crate::RunResult,
    plan: &RunPlan,
    meter: &crate::meter::SharedMeter,
    out_dir: &Path,
) -> Result<(), String> {
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
    let summary = summarize(result, plan);
    let pretty = serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?;
    fs::write(out_dir.join("summary.json"), format!("{pretty}\n")).map_err(|e| e.to_string())?;
    let mut m = crate::meter::lock(meter);
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
