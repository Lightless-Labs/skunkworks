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
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

const AGENT_NETWORK_BOUNDARY_ADVISORY: &str = "  [INFO] agent_network_boundary: not part of the 6/6 sentinel gate; run `python3 bench/agent_network_boundary_check.py --self-test` and `--require-sandbox-runtime`, or `cargo run -p a2ctl -- sentinel --workspace . --require-agent-network-boundary` for an opt-in fail-closed precondition gate, before treating external benchmark evidence as uncontaminated";
const DEFAULT_ARCHIVE_EVIDENCE_JSON: &str = "docs/benchmark-results/self-correction/a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.demo-evidence.json";
const DEFAULT_ARCHIVE_RESULTS_JSONL: &str = "docs/benchmark-results/self-correction/a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.jsonl";
const DEMO_EVIDENCE_ADVISORY: &str = "  [INFO] demo_evidence: not part of the 6/6 sentinel gate; run `python3 bench/self_correction_demo.py verify-demo-docs`, `python3 bench/self_correction_demo.py audit-demo-evidence`, `python3 bench/self_correction_demo.py audit-demo-evidence --json`, and `python3 bench/self_correction_demo.py verify-archive --evidence-json docs/benchmark-results/self-correction/a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.demo-evidence.json` to audit documented archived loop evidence, or `cargo run -p a2ctl -- sentinel --workspace . --require-demo-evidence` for an opt-in combined gate; sentinel default does not refresh or replace those checks";
const DEMO_EVIDENCE_PROOF_STEPS: [&str; 6] = [
    "failed_first_attempt",
    "archived_verifier_failure_evidence",
    "retry_context_from_failure_evidence",
    "later_passing_attempt",
    "lineage_trajectory_recorded",
    "verifier_gated_germline_promotion",
];

fn sentinel_non_gating_advisories() -> [&'static str; 2] {
    [AGENT_NETWORK_BOUNDARY_ADVISORY, DEMO_EVIDENCE_ADVISORY]
}

fn render_sentinel_non_gating_advisory_block() -> String {
    let mut output = String::from("Non-gating advisory checks:\n");
    for advisory in sentinel_non_gating_advisories() {
        output.push_str(advisory);
        output.push('\n');
    }
    output
}

fn render_sentinel_output(workspace: &str, result: &a2_eval::sentinel::SuiteResult) -> String {
    use std::fmt::Write as _;

    let mut output = String::new();
    writeln!(output, "A² Seed Sentinel Suite").expect("write to string should not fail");
    writeln!(output, "Workspace: {workspace}").expect("write to string should not fail");
    writeln!(output).expect("write to string should not fail");

    for r in &result.results {
        let icon = if r.passed { "PASS" } else { "FAIL" };
        writeln!(output, "  [{icon}] {}: {}", r.name, r.detail)
            .expect("write to string should not fail");
    }

    writeln!(output).expect("write to string should not fail");
    writeln!(
        output,
        "Score: {:.0}% ({}/{})",
        result.score * 100.0,
        result.results.iter().filter(|r| r.passed).count(),
        result.results.len()
    )
    .expect("write to string should not fail");
    writeln!(output).expect("write to string should not fail");
    output.push_str(&render_sentinel_non_gating_advisory_block());

    if result.all_passed {
        writeln!(output, "Sentinel gate: PASS").expect("write to string should not fail");
    } else {
        writeln!(output, "Sentinel gate: FAIL").expect("write to string should not fail");
    }

    output
}

fn agent_network_boundary_command_args() -> Vec<String> {
    vec![
        "bench/agent_network_boundary_check.py".to_string(),
        "--require-sandbox-runtime".to_string(),
    ]
}

fn run_agent_network_boundary_gate(workspace: &str) -> Result<String, String> {
    let output = std::process::Command::new("python3")
        .args(agent_network_boundary_command_args())
        .current_dir(workspace)
        .output()
        .map_err(|e| {
            format!("failed to launch agent network boundary verifier in `{workspace}`: {e}")
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(format!(
            "agent network boundary verifier failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status, stdout, stderr
        ));
    }

    Ok(stdout)
}

fn demo_evidence_command_args(archive: &str, evidence_json: &str) -> Vec<String> {
    vec![
        "bench/self_correction_demo.py".to_string(),
        "verify-archive".to_string(),
        "--archive".to_string(),
        archive.to_string(),
        "--evidence-json".to_string(),
        evidence_json.to_string(),
    ]
}

fn validate_demo_evidence_cli_paths(
    workspace: &str,
    archive: &str,
    evidence_json: &str,
) -> Result<(), String> {
    let default_archive_path =
        canonicalize_workspace_path_if_exists(workspace, DEFAULT_ARCHIVE_RESULTS_JSONL)?;
    let default_evidence_path =
        canonicalize_workspace_path_if_exists(workspace, DEFAULT_ARCHIVE_EVIDENCE_JSON)?;
    let archive_path = canonicalize_workspace_path_if_exists(workspace, archive)?;
    let evidence_json_path = canonicalize_workspace_path_if_exists(workspace, evidence_json)?;
    let archive_is_default = archive == DEFAULT_ARCHIVE_RESULTS_JSONL
        || archive_path
            .as_ref()
            .zip(default_archive_path.as_ref())
            .is_some_and(|(path, default)| path == default);
    let evidence_is_default = evidence_json == DEFAULT_ARCHIVE_EVIDENCE_JSON
        || evidence_json_path
            .as_ref()
            .zip(default_evidence_path.as_ref())
            .is_some_and(|(path, default)| path == default);
    if !archive_is_default && evidence_is_default {
        return Err(format!(
            "custom demo archive `{archive}` requires an explicit non-default --demo-evidence-json/--evidence-json path; refusing to rewrite the canonical archived evidence JSON `{DEFAULT_ARCHIVE_EVIDENCE_JSON}`"
        ));
    }
    Ok(())
}

fn run_demo_evidence_contract(
    workspace: &str,
    archive: &str,
    evidence_json: &str,
) -> Result<String, String> {
    validate_demo_evidence_cli_paths(workspace, archive, evidence_json)?;
    let output = std::process::Command::new("python3")
        .args(demo_evidence_command_args(archive, evidence_json))
        .current_dir(workspace)
        .output()
        .map_err(|e| format!("failed to launch demo evidence verifier in `{workspace}`: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(format!(
            "demo evidence verifier failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status, stdout, stderr
        ));
    }

    let summary =
        validate_demo_evidence_contract_artifact_for_archive(workspace, evidence_json, archive)?;
    validate_demo_evidence_contract_output(&stdout, evidence_json, &summary)?;
    Ok(stdout)
}

fn validate_demo_evidence_contract_output(
    output: &str,
    evidence_json: &str,
    summary: &DemoEvidenceContractSummary,
) -> Result<(), String> {
    let proof_chain = DEMO_EVIDENCE_PROOF_STEPS.join(" -> ");
    let contract_pass = format!(
        "PASS evidence JSON matches archived demo contract (requirements={}, demos={})",
        DEMO_EVIDENCE_PROOF_STEPS.len(),
        summary.demos
    );
    let required_fragments = [
        "mode: archived historical provider evidence; no fresh run-id provenance check requested",
        summary.artifact.as_str(),
        evidence_json,
        "PASS complete self-correction demo trajectory found",
        contract_pass.as_str(),
        proof_chain.as_str(),
        "PASS clean-room evidence regeneration",
    ];
    for fragment in required_fragments {
        if !output.contains(fragment) {
            return Err(format!(
                "demo evidence verifier output omitted required fragment: {fragment}"
            ));
        }
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct DemoEvidenceContractSummary {
    artifact: String,
    artifact_sha256: String,
    demos: usize,
}

fn canonicalize_workspace_path(workspace: &str, path: &str) -> Result<PathBuf, String> {
    let resolved = resolve_workspace_path(workspace, path);
    fs::canonicalize(&resolved)
        .map_err(|e| format!("failed to canonicalize `{}`: {e}", resolved.display()))
}

fn canonicalize_workspace_path_if_exists(
    workspace: &str,
    path: &str,
) -> Result<Option<PathBuf>, String> {
    let resolved = resolve_workspace_path(workspace, path);
    match fs::canonicalize(&resolved) {
        Ok(path) => Ok(Some(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "failed to canonicalize `{}`: {error}",
            resolved.display()
        )),
    }
}

#[cfg(test)]
fn validate_demo_evidence_contract_artifact(
    workspace: &str,
    evidence_json: &str,
) -> Result<DemoEvidenceContractSummary, String> {
    validate_demo_evidence_contract_artifact_inner(workspace, evidence_json, None)
}

fn validate_demo_evidence_contract_artifact_for_archive(
    workspace: &str,
    evidence_json: &str,
    expected_archive: &str,
) -> Result<DemoEvidenceContractSummary, String> {
    validate_demo_evidence_contract_artifact_inner(workspace, evidence_json, Some(expected_archive))
}

fn validate_demo_evidence_contract_artifact_inner(
    workspace: &str,
    evidence_json: &str,
    expected_archive: Option<&str>,
) -> Result<DemoEvidenceContractSummary, String> {
    let evidence_path = resolve_workspace_path(workspace, evidence_json);
    let evidence_text = std::fs::read_to_string(&evidence_path)
        .map_err(|e| format!("failed to read `{}`: {e}", evidence_path.display()))?;
    let evidence: serde_json::Value = serde_json::from_str(&evidence_text)
        .map_err(|e| format!("failed to parse `{}` as JSON: {e}", evidence_path.display()))?;
    let summary = validate_demo_evidence_value(&evidence)?;
    let artifact_path = resolve_workspace_path(workspace, &summary.artifact);
    if let Some(expected_archive) = expected_archive {
        let evidence_artifact = canonicalize_workspace_path(workspace, &summary.artifact)?;
        let requested_archive = canonicalize_workspace_path(workspace, expected_archive)?;
        if evidence_artifact != requested_archive {
            return Err(format!(
                "demo evidence artifact `{}` does not match requested archive `{}` after canonicalization",
                summary.artifact, expected_archive
            ));
        }
    }
    let artifact_bytes = std::fs::read(&artifact_path).map_err(|e| {
        format!(
            "failed to read referenced JSONL artifact `{}`: {e}",
            summary.artifact
        )
    })?;
    let actual_hash = format!("{:x}", Sha256::digest(&artifact_bytes));
    if actual_hash != summary.artifact_sha256 {
        return Err(format!(
            "demo evidence artifact hash mismatch for `{}`: expected {}, found {actual_hash}",
            summary.artifact, summary.artifact_sha256
        ));
    }
    let artifact_rows = parse_jsonl_artifact_rows(&artifact_bytes, &summary.artifact)?;
    validate_demo_evidence_rows_against_artifact(&evidence, &artifact_rows)?;
    Ok(summary)
}

fn parse_jsonl_artifact_rows(
    artifact_bytes: &[u8],
    artifact: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let text = std::str::from_utf8(artifact_bytes)
        .map_err(|e| format!("referenced JSONL artifact `{artifact}` is not UTF-8: {e}"))?;
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: serde_json::Value = serde_json::from_str(line).map_err(|e| {
            format!(
                "failed to parse referenced JSONL artifact `{artifact}` line {} as JSON: {e}",
                index + 1
            )
        })?;
        if row.as_object().is_none() {
            return Err(format!(
                "referenced JSONL artifact `{artifact}` line {} must be a JSON object",
                index + 1
            ));
        }
        rows.push(row);
    }
    if rows.is_empty() {
        return Err(format!(
            "referenced JSONL artifact `{artifact}` has no JSON rows"
        ));
    }
    Ok(rows)
}

fn validate_demo_evidence_rows_against_artifact(
    evidence: &serde_json::Value,
    artifact_rows: &[serde_json::Value],
) -> Result<(), String> {
    let demos = require_array(evidence, "demos", "evidence")?;
    for (demo_index, demo) in demos.iter().enumerate() {
        let context = format!("demos[{demo_index}]");
        let chain = require_array(demo, "causal_chain", &context)?;
        for step in chain {
            let requirement = step
                .get("requirement")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<unknown>");
            if let Some(embedded_row) = step.get("evidence_row") {
                let selector = require_object_field(step, "selector", requirement)?;
                validate_embedded_demo_row_against_artifact(
                    artifact_rows,
                    selector,
                    embedded_row,
                    &context,
                    requirement,
                )?;
            }
            if let Some(rows) = step.get("evidence_rows") {
                let rows = rows.as_array().ok_or_else(|| {
                    format!("{context}: {requirement}.evidence_rows must be an array")
                })?;
                for (row_index, embedded_row) in rows.iter().enumerate() {
                    validate_embedded_demo_row_against_artifact(
                        artifact_rows,
                        embedded_row,
                        embedded_row,
                        &context,
                        &format!("{requirement}.evidence_rows[{row_index}]"),
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn validate_embedded_demo_row_against_artifact(
    artifact_rows: &[serde_json::Value],
    selector: &serde_json::Value,
    embedded_row: &serde_json::Value,
    context: &str,
    label: &str,
) -> Result<(), String> {
    let source_row = unique_artifact_row_for_selector(artifact_rows, selector, label)?;
    let normalized_source_row = normalized_demo_evidence_row_from_payload(source_row);
    if normalized_source_row != *embedded_row {
        return Err(format!(
            "{context}: {label} evidence_row does not match the selected JSONL artifact row"
        ));
    }
    Ok(())
}

fn unique_artifact_row_for_selector<'a>(
    artifact_rows: &'a [serde_json::Value],
    selector: &serde_json::Value,
    context: &str,
) -> Result<&'a serde_json::Value, String> {
    let matches: Vec<&serde_json::Value> = artifact_rows
        .iter()
        .filter(|row| artifact_row_matches_selector(row, selector))
        .collect();
    match matches.len() {
        1 => Ok(matches[0]),
        0 => Err(format!(
            "{context}: no matching JSONL artifact row for selector"
        )),
        count => Err(format!(
            "{context}: selector matched {count} JSONL artifact rows; expected exactly one"
        )),
    }
}

fn artifact_row_matches_selector(row: &serde_json::Value, selector: &serde_json::Value) -> bool {
    row.get("run_id").map(selector_string_value)
        == selector.get("run_id").map(selector_string_value)
        && row.get("task_id").map(selector_string_value)
            == selector.get("task_id").map(selector_string_value)
        && optional_i64(row.get("attempt")) == optional_i64(selector.get("attempt"))
}

fn selector_string_value(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn normalized_demo_evidence_row_from_payload(payload: &serde_json::Value) -> serde_json::Value {
    let mut row = serde_json::Map::new();
    row.insert("run_id".into(), string_or_default(payload, "run_id"));
    row.insert("task_id".into(), string_or_default(payload, "task_id"));
    row.insert(
        "attempt".into(),
        serde_json::Value::from(optional_i64(payload.get("attempt")).unwrap_or(1).max(1)),
    );
    row.insert(
        "resolved".into(),
        serde_json::Value::Bool(
            payload.get("resolved").and_then(serde_json::Value::as_bool) == Some(true),
        ),
    );
    row.insert(
        "prior_lineage_present".into(),
        serde_json::Value::Bool(
            payload
                .get("prior_lineage_present")
                .and_then(serde_json::Value::as_bool)
                == Some(true),
        ),
    );
    insert_optional_i64(&mut row, "a2_returncode", payload.get("a2_returncode"));
    insert_optional_i64(
        &mut row,
        "verify_returncode",
        payload.get("verify_returncode"),
    );
    row.insert(
        "verify_command".into(),
        payload
            .get("verify_command")
            .and_then(serde_json::Value::as_str)
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
    );
    row.insert(
        "touched_files".into(),
        serde_json::Value::Array(
            payload
                .get("touched_files")
                .and_then(serde_json::Value::as_array)
                .map(|files| {
                    files
                        .iter()
                        .map(|path| {
                            path.as_str()
                                .map(serde_json::Value::from)
                                .unwrap_or_else(|| serde_json::Value::from(path.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        ),
    );
    insert_optional_i64(
        &mut row,
        "diff_added_lines",
        payload.get("diff_added_lines"),
    );
    insert_optional_i64(
        &mut row,
        "diff_removed_lines",
        payload.get("diff_removed_lines"),
    );
    insert_optional_i64(
        &mut row,
        "lineage_records_before",
        payload.get("lineage_records_before"),
    );
    insert_optional_i64(
        &mut row,
        "lineage_records_after",
        payload.get("lineage_records_after"),
    );
    insert_optional_bool(
        &mut row,
        "lineage_reconciled_by_core",
        payload.get("lineage_reconciled_by_core"),
    );
    row.insert(
        "verifier_failure_evidence_present".into(),
        if payload.get("verifier_failure_evidence_present").is_some() {
            serde_json::Value::Bool(
                payload
                    .get("verifier_failure_evidence_present")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true),
            )
        } else {
            serde_json::Value::Null
        },
    );
    row.insert(
        "verifier_failure_evidence_structured_present".into(),
        serde_json::Value::Bool(payload.get("verifier_failure_evidence_present").is_some()),
    );
    let promotion = payload
        .get("promotion")
        .and_then(serde_json::Value::as_object);
    row.insert(
        "promotion_evidence_present".into(),
        serde_json::Value::Bool(payload_has_promotion_evidence(payload)),
    );
    row.insert(
        "promotion_structured_present".into(),
        serde_json::Value::Bool(promotion.is_some()),
    );
    insert_optional_bool(
        &mut row,
        "promotion_verifier_gated",
        promotion.and_then(|promotion| promotion.get("verifier_gated")),
    );
    insert_optional_bool(
        &mut row,
        "promotion_structured_evidence_present",
        promotion.and_then(|promotion| promotion.get("evidence_present")),
    );
    insert_optional_bool(
        &mut row,
        "promotion_lineage_reconciled_by_core",
        promotion.and_then(|promotion| promotion.get("lineage_reconciled_by_core")),
    );
    insert_optional_i64(
        &mut row,
        "promotion_verify_returncode",
        promotion.and_then(|promotion| promotion.get("verify_returncode")),
    );
    insert_valid_optional_bool(
        &mut row,
        "no_external_solution_search",
        payload.get("no_external_solution_search"),
    );
    insert_valid_optional_string(&mut row, "network_policy", payload.get("network_policy"));
    insert_valid_optional_string(
        &mut row,
        "benchmark_source",
        payload.get("benchmark_source"),
    );
    insert_valid_optional_string(
        &mut row,
        "senior_swe_bench_export_sha256",
        payload.get("senior_swe_bench_export_sha256"),
    );
    insert_valid_optional_positive_i64(
        &mut row,
        "senior_swe_bench_export_row_index",
        payload.get("senior_swe_bench_export_row_index"),
    );
    insert_valid_optional_bool(
        &mut row,
        "audited_sandbox_provider_allowlist_enforced",
        payload.get("audited_sandbox_provider_allowlist_enforced"),
    );
    insert_valid_optional_string(
        &mut row,
        "audited_sandbox_provider_allowlist_status",
        payload.get("audited_sandbox_provider_allowlist_status"),
    );
    insert_valid_optional_object(
        &mut row,
        "audited_sandbox_provider_allowlist_evidence",
        payload.get("audited_sandbox_provider_allowlist_evidence"),
    );
    if payload
        .get("source_head")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|source_head| !source_head.is_empty())
    {
        row.insert("source_head".into(), string_or_null(payload, "source_head"));
        row.insert(
            "source_head_short".into(),
            string_or_null(payload, "source_head_short"),
        );
        row.insert(
            "source_branch".into(),
            string_or_null(payload, "source_branch"),
        );
        insert_optional_bool(&mut row, "source_dirty", payload.get("source_dirty"));
    }
    serde_json::Value::Object(row)
}

fn insert_optional_i64(
    row: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    value: Option<&serde_json::Value>,
) {
    row.insert(
        field.into(),
        optional_i64(value)
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
    );
}

fn insert_optional_bool(
    row: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    value: Option<&serde_json::Value>,
) {
    row.insert(
        field.into(),
        value
            .and_then(serde_json::Value::as_bool)
            .map(serde_json::Value::Bool)
            .unwrap_or(serde_json::Value::Null),
    );
}

fn insert_valid_optional_bool(
    row: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    value: Option<&serde_json::Value>,
) {
    if let Some(value) = value.and_then(serde_json::Value::as_bool) {
        row.insert(field.into(), serde_json::Value::Bool(value));
    }
}

fn insert_valid_optional_string(
    row: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    value: Option<&serde_json::Value>,
) {
    if let Some(value) = value
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
    {
        row.insert(field.into(), serde_json::Value::from(value));
    }
}

fn insert_valid_optional_positive_i64(
    row: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    value: Option<&serde_json::Value>,
) {
    if let Some(value) = value
        .and_then(serde_json::Value::as_i64)
        .filter(|value| *value > 0)
    {
        row.insert(field.into(), serde_json::Value::from(value));
    }
}

fn insert_valid_optional_object(
    row: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    value: Option<&serde_json::Value>,
) {
    if value.and_then(serde_json::Value::as_object).is_some() {
        row.insert(
            field.into(),
            value.cloned().unwrap_or(serde_json::Value::Null),
        );
    }
}

fn optional_i64(value: Option<&serde_json::Value>) -> Option<i64> {
    match value? {
        serde_json::Value::Number(number) => number.as_i64(),
        serde_json::Value::String(text) => text.parse::<i64>().ok(),
        _ => None,
    }
}

fn string_or_default(payload: &serde_json::Value, field: &str) -> serde_json::Value {
    serde_json::Value::from(
        payload
            .get(field)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default(),
    )
}

fn string_or_null(payload: &serde_json::Value, field: &str) -> serde_json::Value {
    payload
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(serde_json::Value::from)
        .unwrap_or(serde_json::Value::Null)
}

fn payload_has_promotion_evidence(payload: &serde_json::Value) -> bool {
    if let Some(promotion) = payload
        .get("promotion")
        .and_then(serde_json::Value::as_object)
    {
        return promotion
            .get("verifier_gated")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
            && promotion
                .get("evidence_present")
                .and_then(serde_json::Value::as_bool)
                == Some(true);
    }
    if let Some(value) = payload.get("promotion_evidence_present") {
        return value.as_bool() == Some(true);
    }
    let output = ["stdout", "stderr"]
        .iter()
        .filter_map(|field| payload.get(*field).and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    output.contains("promote_germline") || output.contains("[applied and rebuilt:")
}

fn resolve_workspace_path(workspace: &str, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(workspace).join(path)
    }
}

fn validate_demo_evidence_value(
    evidence: &serde_json::Value,
) -> Result<DemoEvidenceContractSummary, String> {
    require_bool(evidence, "complete", true, "evidence")?;
    let artifact = require_string(evidence, "artifact", "evidence")?.to_string();
    let artifact_sha256 = require_string(evidence, "artifact_sha256", "evidence")?;
    if artifact_sha256.len() != 64 || !artifact_sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("evidence.artifact_sha256 must be a 64-character hex digest".into());
    }
    let demos = require_array(evidence, "demos", "evidence")?;
    if demos.is_empty() {
        return Err("evidence.demos must contain at least one complete trajectory".into());
    }

    for (demo_index, demo) in demos.iter().enumerate() {
        validate_demo_evidence_demo(demo, artifact_sha256, demo_index)?;
    }

    Ok(DemoEvidenceContractSummary {
        artifact,
        artifact_sha256: artifact_sha256.to_string(),
        demos: demos.len(),
    })
}

fn validate_demo_evidence_demo(
    demo: &serde_json::Value,
    artifact_sha256: &str,
    demo_index: usize,
) -> Result<(), String> {
    let context = format!("demos[{demo_index}]");
    let chain = require_array(demo, "causal_chain", &context)?;
    if chain.len() != DEMO_EVIDENCE_PROOF_STEPS.len() {
        return Err(format!(
            "{context}: causal_chain must contain exactly {} ordered proof steps",
            DEMO_EVIDENCE_PROOF_STEPS.len()
        ));
    }
    for (index, requirement) in DEMO_EVIDENCE_PROOF_STEPS.iter().enumerate() {
        let step = chain.get(index).ok_or_else(|| {
            format!("{context}: causal_chain missing proof step {requirement} at index {index}")
        })?;
        let actual = step
            .get("requirement")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<missing>");
        if actual != *requirement {
            return Err(format!(
                "{context}: causal_chain proof steps must be ordered; expected {requirement} at index {index}, found {actual}"
            ));
        }
        let _ = require_step(chain, requirement, &context)?;
    }
    let failed = require_step(chain, "failed_first_attempt", &context)?;
    let archived = require_step(chain, "archived_verifier_failure_evidence", &context)?;
    let retry = require_step(chain, "retry_context_from_failure_evidence", &context)?;
    let later = require_step(chain, "later_passing_attempt", &context)?;
    let lineage = require_step(chain, "lineage_trajectory_recorded", &context)?;
    let promotion = require_step(chain, "verifier_gated_germline_promotion", &context)?;

    let failed_selector = require_object_field(failed, "selector", "failed_first_attempt")?;
    require_selector_fields(failed_selector, "failed_first_attempt.selector")?;
    let failed_row = require_object_field(failed, "evidence_row", "failed_first_attempt")?;
    let failed_attempt = require_i64(failed_selector, "attempt", "failed_first_attempt.selector")?;
    if !same_selector(failed_selector, failed_row) {
        return Err(format!(
            "{context}: failed evidence row selector does not match failed first attempt"
        ));
    }
    require_bool(
        failed_row,
        "resolved",
        false,
        "failed_first_attempt.evidence_row",
    )?;
    require_positive_i64(
        failed_row,
        "verify_returncode",
        "failed_first_attempt.evidence_row",
    )?;

    let archived_selector =
        require_object_field(archived, "selector", "archived_verifier_failure_evidence")?;
    require_selector_fields(
        archived_selector,
        "archived_verifier_failure_evidence.selector",
    )?;
    if !same_selector(failed_selector, archived_selector) {
        return Err(format!(
            "{context}: archived verifier evidence selector does not match failed first attempt"
        ));
    }
    let archived_fields =
        require_object_field(archived, "fields", "archived_verifier_failure_evidence")?;
    let archived_row = require_object_field(
        archived,
        "evidence_row",
        "archived_verifier_failure_evidence",
    )?;
    require_bool(
        archived_fields,
        "lineage_advanced",
        true,
        "archived_verifier_failure_evidence.fields",
    )?;
    if !same_selector(archived_selector, archived_row) {
        return Err(format!(
            "{context}: archived verifier evidence row selector does not match archived selector"
        ));
    }
    require_bool(
        archived_row,
        "resolved",
        false,
        "archived_verifier_failure_evidence.evidence_row",
    )?;
    require_positive_i64(
        archived_row,
        "verify_returncode",
        "archived_verifier_failure_evidence.evidence_row",
    )?;
    require_increasing_lineage(archived_fields, "archived_verifier_failure_evidence.fields")?;

    let archived_failure_selector = require_object_field(
        retry,
        "archived_failure_selector",
        "retry_context_from_failure_evidence",
    )?;
    require_selector_fields(
        archived_failure_selector,
        "retry_context_from_failure_evidence.archived_failure_selector",
    )?;
    if !same_selector(failed_selector, archived_failure_selector) {
        return Err(format!(
            "{context}: retry context archived failure selector does not match failed first attempt"
        ));
    }
    let retry_artifact_sha256 = require_string(
        retry,
        "archived_failure_artifact_sha256",
        "retry_context_from_failure_evidence",
    )?;
    if retry_artifact_sha256 != artifact_sha256 {
        return Err(format!(
            "{context}: retry context archived failure hash does not match source artifact hash"
        ));
    }
    let retry_fields = require_array(retry, "fields", "retry_context_from_failure_evidence")?;
    if retry_fields.is_empty() {
        return Err(format!(
            "{context}: retry context has no causal field records"
        ));
    }
    let retry_rows = require_array(
        retry,
        "evidence_rows",
        "retry_context_from_failure_evidence",
    )?;
    let retry_selectors = require_array(retry, "selectors", "retry_context_from_failure_evidence")?;
    if retry_rows.len() != retry_fields.len() || retry_selectors.len() != retry_fields.len() {
        return Err(format!(
            "{context}: retry context selectors/evidence_rows must pair with causal field records"
        ));
    }
    for (retry_index, retry_field) in retry_fields.iter().enumerate() {
        let retry_row = &retry_rows[retry_index];
        let retry_selector = &retry_selectors[retry_index];
        require_selector_fields(
            retry_selector,
            "retry_context_from_failure_evidence.selectors[]",
        )?;
        if !same_selector(retry_selector, retry_row) {
            return Err(format!(
                "{context}: retry evidence row selector does not match retry selector"
            ));
        }
        if !same_run_task(failed_selector, retry_row) {
            return Err(format!(
                "{context}: retry evidence row is not in the failed run/task trajectory"
            ));
        }
        let retry_row_attempt = require_i64(
            retry_row,
            "attempt",
            "retry_context_from_failure_evidence.evidence_rows[]",
        )?;
        if retry_row_attempt <= failed_attempt {
            return Err(format!(
                "{context}: retry evidence row must occur after failed first attempt"
            ));
        }
        require_bool(
            retry_row,
            "prior_lineage_present",
            true,
            "retry_context_from_failure_evidence.evidence_rows[]",
        )?;
        require_bool(
            retry_field,
            "derived_from_failed_lineage",
            true,
            "retry_context_from_failure_evidence.fields[]",
        )?;
        require_bool(
            retry_field,
            "archived_verifier_failure_evidence",
            true,
            "retry_context_from_failure_evidence.fields[]",
        )?;
        require_bool(
            retry_field,
            "retry_context_links_archived_failure",
            true,
            "retry_context_from_failure_evidence.fields[]",
        )?;
        require_bool(
            retry_field,
            "prior_lineage_present",
            true,
            "retry_context_from_failure_evidence.fields[]",
        )?;
        require_positive_i64(
            retry_field,
            "failed_verify_returncode",
            "retry_context_from_failure_evidence.fields[]",
        )?;
        let failed_after = require_i64(
            retry_field,
            "failed_lineage_records_after",
            "retry_context_from_failure_evidence.fields[]",
        )?;
        let retry_before = require_i64(
            retry_field,
            "lineage_records_before",
            "retry_context_from_failure_evidence.fields[]",
        )?;
        let retry_row_before = require_i64(
            retry_row,
            "lineage_records_before",
            "retry_context_from_failure_evidence.evidence_rows[]",
        )?;
        if retry_row_before != retry_before {
            return Err(format!(
                "{context}: retry evidence row lineage boundary does not match causal fields"
            ));
        }
        let retry_field_attempt = require_i64(
            retry_field,
            "attempt",
            "retry_context_from_failure_evidence.fields[]",
        )?;
        if retry_field_attempt != retry_row_attempt {
            return Err(format!(
                "{context}: retry evidence row attempt does not match causal fields"
            ));
        }
        if retry_before < failed_after {
            return Err(format!(
                "{context}: retry lineage does not reach archived failed lineage boundary"
            ));
        }
        let retry_failed_selector = require_object_field(
            retry_field,
            "failed_attempt_selector",
            "retry_context_from_failure_evidence.fields[]",
        )?;
        require_selector_fields(
            retry_failed_selector,
            "retry_context_from_failure_evidence.fields[].failed_attempt_selector",
        )?;
        if !same_selector(failed_selector, retry_failed_selector) {
            return Err(format!(
                "{context}: retry field failed_attempt_selector does not match failed first attempt"
            ));
        }
    }

    let later_selector = require_object_field(later, "selector", "later_passing_attempt")?;
    require_selector_fields(later_selector, "later_passing_attempt.selector")?;
    if !same_run_task(failed_selector, later_selector) {
        return Err(format!(
            "{context}: later passing attempt is not in the failed run/task trajectory"
        ));
    }
    let later_attempt = require_i64(later_selector, "attempt", "later_passing_attempt.selector")?;
    if later_attempt <= failed_attempt {
        return Err(format!(
            "{context}: later passing attempt must occur after failed first attempt"
        ));
    }
    let later_row = require_object_field(later, "evidence_row", "later_passing_attempt")?;
    if !same_selector(later_selector, later_row) {
        return Err(format!(
            "{context}: later passing evidence row selector does not match later passing attempt"
        ));
    }
    require_bool(
        later_row,
        "resolved",
        true,
        "later_passing_attempt.evidence_row",
    )?;
    require_i64_equals(
        later_row,
        "verify_returncode",
        0,
        "later_passing_attempt.evidence_row",
    )?;

    let lineage_fields = require_object_field(lineage, "fields", "lineage_trajectory_recorded")?;
    let lineage_rows = require_array(lineage, "evidence_rows", "lineage_trajectory_recorded")?;
    if lineage_rows.len() < 2 {
        return Err(format!(
            "{context}: lineage trajectory evidence_rows must include failed and later attempts"
        ));
    }
    let attempts = require_array(
        lineage_fields,
        "attempts",
        "lineage_trajectory_recorded.fields",
    )?;
    if attempts.len() < 2
        || !attempts
            .iter()
            .any(|attempt| attempt.as_i64() == Some(failed_attempt))
        || !attempts
            .iter()
            .any(|attempt| attempt.as_i64() == Some(later_attempt))
    {
        return Err(format!(
            "{context}: lineage trajectory must span failed and later attempts"
        ));
    }
    let mut lineage_has_failed = false;
    let mut lineage_has_later = false;
    for lineage_row in lineage_rows {
        if !same_run_task(failed_selector, lineage_row) {
            return Err(format!(
                "{context}: lineage evidence row is not in the failed run/task trajectory"
            ));
        }
        match require_i64(
            lineage_row,
            "attempt",
            "lineage_trajectory_recorded.evidence_rows[]",
        )? {
            attempt if attempt == failed_attempt => lineage_has_failed = true,
            attempt if attempt == later_attempt => lineage_has_later = true,
            _ => {}
        }
    }
    if !(lineage_has_failed && lineage_has_later) {
        return Err(format!(
            "{context}: lineage evidence rows must include failed and later attempts"
        ));
    }
    require_increasing_lineage(lineage_fields, "lineage_trajectory_recorded.fields")?;

    let promotion_selector =
        require_object_field(promotion, "selector", "verifier_gated_germline_promotion")?;
    require_selector_fields(
        promotion_selector,
        "verifier_gated_germline_promotion.selector",
    )?;
    if !same_selector(later_selector, promotion_selector) {
        return Err(format!(
            "{context}: verifier-gated promotion selector does not match later passing attempt"
        ));
    }
    let promotion_fields =
        require_object_field(promotion, "fields", "verifier_gated_germline_promotion")?;
    let promotion_row = require_object_field(
        promotion,
        "evidence_row",
        "verifier_gated_germline_promotion",
    )?;
    if !same_selector(promotion_selector, promotion_row) {
        return Err(format!(
            "{context}: verifier-gated promotion evidence row selector does not match promotion selector"
        ));
    }
    require_i64_equals(
        promotion_fields,
        "verify_returncode",
        0,
        "verifier_gated_germline_promotion.fields",
    )?;
    require_i64_equals(
        promotion_row,
        "verify_returncode",
        0,
        "verifier_gated_germline_promotion.evidence_row",
    )?;
    require_bool(
        promotion_fields,
        "lineage_reconciled_by_core",
        true,
        "verifier_gated_germline_promotion.fields",
    )?;
    require_bool(
        promotion_row,
        "lineage_reconciled_by_core",
        true,
        "verifier_gated_germline_promotion.evidence_row",
    )?;
    let legacy_promotion_evidence = promotion_fields
        .get("promotion_evidence_present")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
        && promotion_row
            .get("promotion_evidence_present")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
    let structured_promotion_evidence = promotion_fields
        .get("promotion_verifier_gated")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
        && promotion_row
            .get("promotion_verifier_gated")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        && promotion_fields
            .get("promotion_structured_evidence_present")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        && promotion_row
            .get("promotion_structured_evidence_present")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        && promotion_fields
            .get("promotion_lineage_reconciled_by_core")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        && promotion_row
            .get("promotion_lineage_reconciled_by_core")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        && promotion_fields
            .get("promotion_verify_returncode")
            .and_then(serde_json::Value::as_i64)
            == Some(0)
        && promotion_row
            .get("promotion_verify_returncode")
            .and_then(serde_json::Value::as_i64)
            == Some(0);
    if !(legacy_promotion_evidence || structured_promotion_evidence) {
        return Err(format!(
            "{context}: verifier-gated promotion lacks gated apply evidence in evidence_row"
        ));
    }

    Ok(())
}

fn require_step<'a>(
    chain: &'a [serde_json::Value],
    requirement: &str,
    context: &str,
) -> Result<&'a serde_json::Value, String> {
    let step = chain
        .iter()
        .find(|step| {
            step.get("requirement").and_then(serde_json::Value::as_str) == Some(requirement)
        })
        .ok_or_else(|| format!("{context}: missing proof step {requirement}"))?;
    require_string(step, "status", requirement).and_then(|status| {
        if status == "proved" {
            Ok(status)
        } else {
            Err(format!(
                "{context}: proof step {requirement} has status {status}"
            ))
        }
    })?;
    Ok(step)
}

fn require_object_field<'a>(
    value: &'a serde_json::Value,
    field: &str,
    context: &str,
) -> Result<&'a serde_json::Value, String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_object)
        .map(|_| &value[field])
        .ok_or_else(|| format!("{context}.{field} must be an object"))
}

fn require_array<'a>(
    value: &'a serde_json::Value,
    field: &str,
    context: &str,
) -> Result<&'a Vec<serde_json::Value>, String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{context}.{field} must be an array"))
}

fn require_string<'a>(
    value: &'a serde_json::Value,
    field: &str,
    context: &str,
) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("{context}.{field} must be a string"))
}

fn require_bool(
    value: &serde_json::Value,
    field: &str,
    expected: bool,
    context: &str,
) -> Result<(), String> {
    match value.get(field).and_then(serde_json::Value::as_bool) {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(format!(
            "{context}.{field} expected {expected}, found {actual}"
        )),
        None => Err(format!("{context}.{field} must be a bool")),
    }
}

fn require_i64(value: &serde_json::Value, field: &str, context: &str) -> Result<i64, String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| format!("{context}.{field} must be an integer"))
}

fn require_i64_equals(
    value: &serde_json::Value,
    field: &str,
    expected: i64,
    context: &str,
) -> Result<(), String> {
    let actual = require_i64(value, field, context)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{context}.{field} expected {expected}, found {actual}"
        ))
    }
}

fn require_positive_i64(
    value: &serde_json::Value,
    field: &str,
    context: &str,
) -> Result<(), String> {
    let actual = require_i64(value, field, context)?;
    if actual > 0 {
        Ok(())
    } else {
        Err(format!(
            "{context}.{field} must be positive, found {actual}"
        ))
    }
}

fn require_increasing_lineage(value: &serde_json::Value, context: &str) -> Result<(), String> {
    let before = require_i64(value, "lineage_records_before", context)?;
    let after = require_i64(value, "lineage_records_after", context)?;
    if after > before {
        Ok(())
    } else {
        Err(format!(
            "{context} must advance lineage, found {before}->{after}"
        ))
    }
}

fn require_selector_fields(selector: &serde_json::Value, context: &str) -> Result<(), String> {
    require_string(selector, "run_id", context)?;
    require_string(selector, "task_id", context)?;
    require_i64(selector, "attempt", context)?;
    Ok(())
}

fn same_selector(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    same_run_task(left, right)
        && left.get("attempt").and_then(serde_json::Value::as_i64)
            == right.get("attempt").and_then(serde_json::Value::as_i64)
}

fn same_run_task(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    left.get("run_id").and_then(serde_json::Value::as_str)
        == right.get("run_id").and_then(serde_json::Value::as_str)
        && left.get("task_id").and_then(serde_json::Value::as_str)
            == right.get("task_id").and_then(serde_json::Value::as_str)
}

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
        /// Execution-level network policy for every stdin task. Use `isolated`
        /// to fail closed unless the selected catalyst/provider can enforce an
        /// audited network sandbox, or `allowlist:<url>[,<url>...]` for a future
        /// provider-endpoint allowlist.
        #[arg(long, value_parser = parse_network_policy_arg)]
        network_policy: Option<a2_core::protocol::NetworkPolicy>,
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
        /// Execution-level network policy for every autopilot task. Use
        /// `isolated`/`allowlist:...` only when the selected execution path can
        /// fail closed or run under an audited sandbox.
        #[arg(long, value_parser = parse_network_policy_arg)]
        network_policy: Option<a2_core::protocol::NetworkPolicy>,
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
    /// Run autopilot repeatedly on a fixed interval and keep durable resident logs.
    AutopilotResident {
        /// Workspace root path (defaults to current directory).
        #[arg(long, default_value = ".")]
        workspace: String,
        /// Provider(s) to use. Comma-separated list for round-robin cycling.
        #[arg(long, default_value = "pi/zai/glm-5.1")]
        provider: String,
        /// Maximum autopilot iterations per resident run.
        #[arg(long, default_value = "3")]
        max_iterations: usize,
        /// Maximum token budget per autopilot task.
        #[arg(long, default_value = "100000")]
        max_tokens: u64,
        /// Maximum wall-clock time per autopilot task in seconds.
        #[arg(long, default_value = "1800")]
        timeout: u64,
        /// Seconds to sleep between autopilot runs.
        #[arg(long, default_value = "3600")]
        interval_secs: u64,
        /// Number of resident runs before stopping. Use 0 to run until interrupted.
        #[arg(long, default_value = "0")]
        max_runs: usize,
        /// Auto-apply promoted patches via git apply.
        #[arg(long)]
        apply: bool,
        /// Execution-level network policy forwarded to each autopilot run.
        #[arg(long, value_parser = parse_network_policy_arg)]
        network_policy: Option<a2_core::protocol::NetworkPolicy>,
        /// Explicit task forwarded to each autopilot run. May be repeated.
        #[arg(long)]
        task: Vec<String>,
        /// File containing an explicit task forwarded to each autopilot run. May be repeated.
        #[arg(long)]
        task_file: Vec<String>,
        /// Forward --dry-run to autopilot; discovers and logs without model calls.
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
        /// Also require the archived demo-evidence contract after the 6/6 sentinel suite passes.
        #[arg(long)]
        require_demo_evidence: bool,
        /// Also require the child-agent network boundary precondition after the 6/6 sentinel suite passes.
        #[arg(long)]
        require_agent_network_boundary: bool,
        /// Archived JSONL artifact used by --require-demo-evidence.
        #[arg(long, default_value = DEFAULT_ARCHIVE_RESULTS_JSONL)]
        demo_archive: String,
        /// Machine-readable demo evidence JSON used by --require-demo-evidence.
        #[arg(long, default_value = DEFAULT_ARCHIVE_EVIDENCE_JSON)]
        demo_evidence_json: String,
    },
    /// Verify an archived self-correction demo evidence contract.
    DemoEvidence {
        /// Workspace root path (defaults to current directory).
        #[arg(long, default_value = ".")]
        workspace: String,
        /// Archived JSONL artifact to re-score before structural validation.
        #[arg(long, default_value = DEFAULT_ARCHIVE_RESULTS_JSONL)]
        archive: String,
        /// Machine-readable demo evidence JSON to verify.
        #[arg(long, default_value = DEFAULT_ARCHIVE_EVIDENCE_JSON)]
        evidence_json: String,
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
    network_policy: Option<a2_core::protocol::NetworkPolicy>,
    started_at: String,
    completed_at: String,
    total_iterations: usize,
    total_tokens: u64,
    total_duration_secs: f64,
    patches_produced: usize,
    applied_count: usize,
    verified_count: usize,
    stop_reason: String,
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
    checklist_update: Option<ChecklistUpdateSummary>,
}

/// Result of marking a checklist-sourced autopilot task complete.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct ChecklistUpdateSummary {
    path: String,
    line: usize,
    status: String,
}

/// Configuration for the resident autopilot wrapper. Each resident tick invokes
/// the normal `a2ctl autopilot` command so the CLI loop remains the single
/// implementation of work discovery, execution, apply, verification, and logs.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ResidentAutopilotConfig {
    workspace: String,
    provider: String,
    max_iterations: usize,
    max_tokens: u64,
    timeout: u64,
    interval_secs: u64,
    max_runs: usize,
    apply: bool,
    network_policy: Option<a2_core::protocol::NetworkPolicy>,
    task: Vec<String>,
    task_file: Vec<String>,
    dry_run: bool,
    log_dir: String,
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

fn autopilot_stop_reason(
    summaries: &[AutopilotIterationSummary],
    max_tokens: u64,
    max_iterations: usize,
) -> Option<String> {
    let total_tokens: u64 = summaries.iter().map(|summary| summary.tokens).sum();
    if max_tokens > 0 && total_tokens >= max_tokens {
        return Some(format!(
            "budget_exhausted: used {total_tokens} tokens, limit {max_tokens}"
        ));
    }

    if let Some(latest) = summaries.last()
        && is_provider_quota_failure(&latest.decision)
    {
        return Some(format!("provider_quota_failure: {}", latest.decision));
    }

    if summaries.len() >= 2 {
        let latest = &summaries[summaries.len() - 1];
        let previous = &summaries[summaries.len() - 2];
        if !latest.verify_ok
            && !previous.verify_ok
            && latest.decision == previous.decision
            && latest.patch_produced == previous.patch_produced
        {
            return Some(format!(
                "repeated_failure_class: decision='{}', patch_produced={}",
                latest.decision, latest.patch_produced
            ));
        }
    }

    if summaries.len() >= max_iterations {
        return Some(format!("max_iterations_reached: {max_iterations}"));
    }

    None
}

fn is_provider_quota_failure(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("quota")
        || lower.contains("rate limit")
        || lower.contains("429")
        || lower.contains("capacity")
        || lower.contains("insufficient balance")
}

fn checklist_source_location(source: &str) -> Option<(&str, usize)> {
    let (path, line) = source.rsplit_once(':')?;
    if !(path.starts_with("todos/") || path.starts_with("docs/plans/")) {
        return None;
    }
    let line = line.parse::<usize>().ok()?;
    if line == 0 {
        return None;
    }
    Some((path, line))
}

fn mark_checklist_item_completed(
    root: &Path,
    candidate: &AutopilotCandidate,
) -> io::Result<Option<ChecklistUpdateSummary>> {
    let Some((relative_path, line_number)) = checklist_source_location(&candidate.source) else {
        return Ok(None);
    };

    let path = root.join(relative_path);
    let content = fs::read_to_string(&path)?;
    let mut lines = content.lines().map(String::from).collect::<Vec<_>>();
    let Some(line) = lines.get_mut(line_number.saturating_sub(1)) else {
        return Ok(Some(ChecklistUpdateSummary {
            path: relative_path.into(),
            line: line_number,
            status: "missing_line".into(),
        }));
    };

    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    let indent = &line[..indent_len];
    let status = if let Some(rest) = trimmed.strip_prefix("- [ ]") {
        *line = format!("{indent}- [x]{rest}");
        "marked_complete"
    } else if let Some(rest) = trimmed.strip_prefix("* [ ]") {
        *line = format!("{indent}* [x]{rest}");
        "marked_complete"
    } else if trimmed.starts_with("- [x]") || trimmed.starts_with("* [x]") {
        "already_complete"
    } else {
        "not_unchecked_item"
    };

    if status == "marked_complete" {
        let mut updated = lines.join("\n");
        if content.ends_with('\n') {
            updated.push('\n');
        }
        fs::write(&path, updated)?;
    }

    Ok(Some(ChecklistUpdateSummary {
        path: relative_path.into(),
        line: line_number,
        status: status.into(),
    }))
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
    #[serde(default)]
    no_external_solution_search: bool,
    #[serde(default)]
    network_policy: Option<a2_core::protocol::NetworkPolicy>,
}

#[derive(Debug, Deserialize)]
struct RunVerificationSpec {
    command: String,
    #[serde(default)]
    expect_exit: i32,
}

fn network_policy_arg_value(policy: &a2_core::protocol::NetworkPolicy) -> String {
    match policy {
        a2_core::protocol::NetworkPolicy::Open => "open".into(),
        a2_core::protocol::NetworkPolicy::Isolated => "isolated".into(),
        a2_core::protocol::NetworkPolicy::AllowList(endpoints) => {
            format!("allowlist:{}", endpoints.join(","))
        }
    }
}

fn restricted_policy_without_candidate(
    policy: Option<&a2_core::protocol::NetworkPolicy>,
    patch_produced: bool,
) -> bool {
    !patch_produced
        && matches!(
            policy,
            Some(
                a2_core::protocol::NetworkPolicy::Isolated
                    | a2_core::protocol::NetworkPolicy::AllowList(_)
            )
        )
}

fn parse_network_policy_arg(value: &str) -> Result<a2_core::protocol::NetworkPolicy, String> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("open") {
        return Ok(a2_core::protocol::NetworkPolicy::Open);
    }
    if trimmed.eq_ignore_ascii_case("isolated") {
        return Ok(a2_core::protocol::NetworkPolicy::Isolated);
    }
    let lower = trimmed.to_ascii_lowercase();
    if let Some(rest) = lower
        .strip_prefix("allowlist:")
        .or_else(|| lower.strip_prefix("allow-list:"))
    {
        let prefix_len = trimmed.len() - rest.len();
        let endpoints: Vec<String> = trimmed[prefix_len..]
            .split(',')
            .map(str::trim)
            .filter(|endpoint| !endpoint.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        if endpoints.is_empty() {
            return Err("allowlist network policy requires at least one endpoint".into());
        }
        return Ok(a2_core::protocol::NetworkPolicy::AllowList(endpoints));
    }
    Err(
        "network policy must be one of: open, isolated, allowlist:<endpoint>[,<endpoint>...]"
            .into(),
    )
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
            network_policy,
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
            let mut restricted_policy_launch_blocked = false;
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

                let task = task_from_run_input_with_network_policy(
                    &ingester,
                    parse_run_input(raw_line),
                    network_policy.clone(),
                );

                let title = task.title.clone();
                let task_network_policy = task.network_policy.clone();

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
                        if restricted_policy_without_candidate(
                            task_network_policy.as_ref(),
                            outcome.result.patch.is_some(),
                        ) {
                            restricted_policy_launch_blocked = true;
                            eprintln!(
                                "[restricted network policy blocked provider launch for {title}; no candidate patch produced]"
                            );
                        }
                        rows.push(run_summary_row(&title, p, &outcome));
                    }
                    Err(e) => {
                        if restricted_policy_without_candidate(task_network_policy.as_ref(), false)
                        {
                            restricted_policy_launch_blocked = true;
                        }
                        rows.push(RunSummaryRow {
                            title,
                            model: requested_model(p),
                            tokens: 0,
                            duration_secs: 0.0,
                            decision: format!("error: {e}"),
                        });
                    }
                }
            }

            if rows.is_empty() {
                eprintln!("No task descriptions provided on stdin.");
                std::process::exit(1);
            }

            print!("{}", render_summary_table(&rows));
            if restricted_policy_launch_blocked {
                std::process::exit(1);
            }
        }
        Commands::Autopilot {
            workspace,
            provider,
            max_iterations,
            max_tokens,
            timeout,
            apply,
            network_policy,
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
                    "network_policy": network_policy,
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
            let mut stop_reason = None;
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
                if let Some(policy) = network_policy.clone() {
                    task.network_policy = Some(policy);
                }

                let provider = providers[iteration % providers.len()].as_ref();
                log_autopilot_event(
                    &run_dir,
                    "iteration_started",
                    serde_json::json!({
                        "iteration": iteration + 1,
                        "task_id": task.id.to_string(),
                        "candidate": autopilot_candidate_json(candidate),
                        "model": requested_model(provider),
                        "network_policy": task.network_policy,
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
                        let checklist_update = if apply_ok && verify_ok {
                            match mark_checklist_item_completed(&workspace_root, candidate) {
                                Ok(update) => update,
                                Err(e) => Some(ChecklistUpdateSummary {
                                    path: candidate.source.clone(),
                                    line: 0,
                                    status: format!("failed: {e}"),
                                }),
                            }
                        } else {
                            None
                        };
                        if let Some(update) = &checklist_update {
                            log_autopilot_event(
                                &run_dir,
                                "checklist_update",
                                serde_json::json!({
                                    "iteration": iteration + 1,
                                    "path": update.path,
                                    "line": update.line,
                                    "status": update.status,
                                }),
                            );
                        }
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
                            checklist_update: checklist_update.clone(),
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
                                "checklist_update": checklist_update,
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
                            checklist_update: None,
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

                if let Some(reason) =
                    autopilot_stop_reason(&iteration_summaries, max_tokens, max_iterations)
                {
                    log_autopilot_event(
                        &run_dir,
                        "autopilot_stopped",
                        serde_json::json!({
                            "iteration": iteration + 1,
                            "reason": reason,
                        }),
                    );
                    stop_reason = Some(reason);
                    break;
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
                network_policy: network_policy.clone(),
                started_at,
                completed_at,
                total_iterations: iteration_summaries.len(),
                total_tokens,
                total_duration_secs,
                patches_produced,
                applied_count,
                verified_count,
                stop_reason: stop_reason.unwrap_or_else(|| "completed".into()),
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
            if let Err(e) = append_autopilot_run_index(&run_dir, &run_summary) {
                eprintln!("[failed to append autopilot run index: {e}]");
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
                    "network_policy": run_summary.network_policy,
                    "stop_reason": run_summary.stop_reason,
                }),
            );
            print!("{}", render_summary_table(&rows));
            println!("Autopilot log: {}", run_dir.display());
        }
        Commands::AutopilotResident {
            workspace,
            provider,
            max_iterations,
            max_tokens,
            timeout,
            interval_secs,
            max_runs,
            apply,
            network_policy,
            task,
            task_file,
            dry_run,
            log_dir,
        } => {
            let config = ResidentAutopilotConfig {
                workspace,
                provider,
                max_iterations,
                max_tokens,
                timeout,
                interval_secs,
                max_runs,
                apply,
                network_policy,
                task,
                task_file,
                dry_run,
                log_dir,
            };
            if let Err(e) = run_autopilot_resident(&config) {
                eprintln!("Autopilot resident failed: {e}");
                std::process::exit(1);
            }
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
        Commands::Sentinel {
            workspace,
            require_demo_evidence,
            require_agent_network_boundary,
            demo_archive,
            demo_evidence_json,
        } => {
            let suite =
                a2_eval::sentinel::SentinelSuite::seed_suite(std::path::PathBuf::from(&workspace));
            let result = suite.run_all();

            print!("{}", render_sentinel_output(&workspace, &result));

            let mut failed = !result.all_passed;
            if require_demo_evidence && result.all_passed {
                println!();
                println!("Required demo evidence gate:");
                match run_demo_evidence_contract(&workspace, &demo_archive, &demo_evidence_json) {
                    Ok(_) => {
                        println!("  PASS archived demo evidence contract validated");
                        println!("  archive: {demo_archive}");
                        println!("  evidence: {demo_evidence_json}");
                    }
                    Err(error) => {
                        eprintln!("  FAIL archived demo evidence contract rejected");
                        eprintln!("{error}");
                        failed = true;
                    }
                }
            }

            if require_agent_network_boundary && result.all_passed {
                println!();
                println!("Required agent network boundary gate:");
                match run_agent_network_boundary_gate(&workspace) {
                    Ok(_) => {
                        println!("  PASS child-agent network boundary precondition validated");
                        println!(
                            "  command: python3 bench/agent_network_boundary_check.py --require-sandbox-runtime"
                        );
                    }
                    Err(error) => {
                        eprintln!("  FAIL child-agent network boundary precondition rejected");
                        eprintln!("{error}");
                        failed = true;
                    }
                }
            }

            if failed {
                std::process::exit(1);
            }
        }
        Commands::DemoEvidence {
            workspace,
            archive,
            evidence_json,
        } => match run_demo_evidence_contract(&workspace, &archive, &evidence_json) {
            Ok(output) => print!("{output}"),
            Err(error) => {
                eprintln!("Demo evidence gate: FAIL");
                eprintln!("{error}");
                std::process::exit(1);
            }
        },
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
        task.no_external_solution_search = true;
        task.network_policy = Some(a2_core::protocol::NetworkPolicy::Isolated);
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
            task.no_external_solution_search = input.no_external_solution_search;
            task.network_policy = input.network_policy;

            task
        }
    }
}

fn task_from_run_input_with_network_policy(
    ingester: &a2_sensorium::ingest::Ingester,
    input: ParsedRunInput,
    network_policy: Option<a2_core::protocol::NetworkPolicy>,
) -> a2_core::protocol::TaskContract {
    let mut task = task_from_run_input(ingester, input);
    if let Some(policy) = network_policy {
        task.network_policy = Some(policy);
    }
    task
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

fn autopilot_resident_dir(workspace_root: &Path, log_dir: &Path) -> PathBuf {
    let base = if log_dir.is_absolute() {
        log_dir.to_path_buf()
    } else {
        workspace_root.join(log_dir)
    };
    base.join("resident").join(format!(
        "resident-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ))
}

fn resident_autopilot_args(config: &ResidentAutopilotConfig) -> Vec<String> {
    let mut args = vec![
        "autopilot".to_string(),
        "--workspace".to_string(),
        config.workspace.clone(),
        "--provider".to_string(),
        config.provider.clone(),
        "--max-iterations".to_string(),
        config.max_iterations.to_string(),
        "--max-tokens".to_string(),
        config.max_tokens.to_string(),
        "--timeout".to_string(),
        config.timeout.to_string(),
        "--log-dir".to_string(),
        config.log_dir.clone(),
    ];
    if config.apply {
        args.push("--apply".to_string());
    }
    if let Some(policy) = &config.network_policy {
        args.push("--network-policy".to_string());
        args.push(network_policy_arg_value(policy));
    }
    if config.dry_run {
        args.push("--dry-run".to_string());
    }
    for task in &config.task {
        args.push("--task".to_string());
        args.push(task.clone());
    }
    for task_file in &config.task_file {
        args.push("--task-file".to_string());
        args.push(task_file.clone());
    }
    args
}

fn run_autopilot_resident(config: &ResidentAutopilotConfig) -> io::Result<()> {
    let workspace_root = PathBuf::from(&config.workspace);
    let resident_dir = autopilot_resident_dir(&workspace_root, Path::new(&config.log_dir));
    fs::create_dir_all(&resident_dir)?;
    log_autopilot_event(
        &resident_dir,
        "resident_started",
        serde_json::json!({
            "workspace": &config.workspace,
            "provider": &config.provider,
            "max_iterations": config.max_iterations,
            "max_tokens": config.max_tokens,
            "timeout": config.timeout,
            "interval_secs": config.interval_secs,
            "max_runs": config.max_runs,
            "apply": config.apply,
            "network_policy": config.network_policy,
            "dry_run": config.dry_run,
        }),
    );
    println!("A² Autopilot resident: {}", resident_dir.display());

    let mut run_count = 0usize;
    loop {
        if config.max_runs > 0 && run_count >= config.max_runs {
            log_autopilot_event(
                &resident_dir,
                "resident_stopped",
                serde_json::json!({
                    "reason": format!("max_runs_reached: {}", config.max_runs),
                    "runs": run_count,
                }),
            );
            break;
        }

        run_count += 1;
        let args = resident_autopilot_args(config);
        log_autopilot_event(
            &resident_dir,
            "resident_run_started",
            serde_json::json!({
                "run": run_count,
                "args": &args,
            }),
        );

        let output = std::process::Command::new(std::env::current_exe()?)
            .args(&args)
            .current_dir(&workspace_root)
            .output()?;
        fs::write(
            resident_dir.join(format!("run-{run_count:04}.stdout")),
            &output.stdout,
        )?;
        fs::write(
            resident_dir.join(format!("run-{run_count:04}.stderr")),
            &output.stderr,
        )?;
        io::stdout().write_all(&output.stdout)?;
        io::stderr().write_all(&output.stderr)?;

        log_autopilot_event(
            &resident_dir,
            "resident_run_completed",
            serde_json::json!({
                "run": run_count,
                "success": output.status.success(),
                "exit_code": output.status.code(),
                "stdout_path": resident_dir.join(format!("run-{run_count:04}.stdout")).display().to_string(),
                "stderr_path": resident_dir.join(format!("run-{run_count:04}.stderr")).display().to_string(),
            }),
        );

        if config.max_runs > 0 && run_count >= config.max_runs {
            continue;
        }
        std::thread::sleep(std::time::Duration::from_secs(config.interval_secs));
    }
    Ok(())
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

fn append_autopilot_run_index(run_dir: &Path, summary: &AutopilotRunSummary) -> io::Result<()> {
    let base_dir = run_dir
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| io::Error::other("autopilot run dir has no log base"))?;
    fs::create_dir_all(base_dir)?;

    let compact_iterations = summary
        .iterations
        .iter()
        .map(|iteration| {
            serde_json::json!({
                "iteration": iteration.iteration,
                "candidate_id": &iteration.candidate_id,
                "candidate_source": &iteration.candidate_source,
                "candidate_title": &iteration.candidate_title,
                "model": &iteration.model,
                "tokens": iteration.tokens,
                "duration_secs": iteration.duration_secs,
                "decision": &iteration.decision,
                "patch_produced": iteration.patch_produced,
                "apply_ok": iteration.apply_ok,
                "verify_ok": iteration.verify_ok,
            })
        })
        .collect::<Vec<_>>();
    let record = serde_json::json!({
        "at": chrono::Utc::now().to_rfc3339(),
        "run_id": &summary.run_id,
        "workspace": &summary.workspace,
        "provider": &summary.provider,
        "max_iterations": summary.max_iterations,
        "total_iterations": summary.total_iterations,
        "total_tokens": summary.total_tokens,
        "total_duration_secs": summary.total_duration_secs,
        "patches_produced": summary.patches_produced,
        "applied_count": summary.applied_count,
        "verified_count": summary.verified_count,
        "stop_reason": &summary.stop_reason,
        "run_dir": run_dir.display().to_string(),
        "events_path": run_dir.join("events.jsonl").display().to_string(),
        "summary_path": run_dir.join("run_summary.json").display().to_string(),
        "iterations": compact_iterations,
    });

    let mut index = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(base_dir.join("run_index.jsonl"))?;
    serde_json::to_writer(&mut index, &record).map_err(io::Error::other)?;
    writeln!(index)?;
    let latest = serde_json::to_string_pretty(&record).map_err(io::Error::other)?;
    fs::write(base_dir.join("latest_run.json"), latest)?;
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
    use std::path::{Path, PathBuf};
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
    fn sentinel_cli_keeps_demo_evidence_gate_opt_in() {
        let cli = Cli::try_parse_from(["a2ctl", "sentinel", "--workspace", "."]).unwrap();
        match cli.command {
            Commands::Sentinel {
                workspace,
                require_demo_evidence,
                require_agent_network_boundary,
                demo_archive,
                demo_evidence_json,
            } => {
                assert_eq!(workspace, ".");
                assert!(!require_demo_evidence);
                assert!(!require_agent_network_boundary);
                assert_eq!(demo_archive, DEFAULT_ARCHIVE_RESULTS_JSONL);
                assert_eq!(demo_evidence_json, DEFAULT_ARCHIVE_EVIDENCE_JSON);
            }
            _ => panic!("expected sentinel command"),
        }
    }

    #[test]
    fn sentinel_cli_accepts_required_demo_evidence_paths() {
        let cli = Cli::try_parse_from([
            "a2ctl",
            "sentinel",
            "--require-demo-evidence",
            "--demo-archive",
            "custom/results.jsonl",
            "--demo-evidence-json",
            "custom/evidence.json",
        ])
        .unwrap();
        match cli.command {
            Commands::Sentinel {
                workspace,
                require_demo_evidence,
                require_agent_network_boundary,
                demo_archive,
                demo_evidence_json,
            } => {
                assert_eq!(workspace, ".");
                assert!(require_demo_evidence);
                assert!(!require_agent_network_boundary);
                assert_eq!(demo_archive, "custom/results.jsonl");
                assert_eq!(demo_evidence_json, "custom/evidence.json");
            }
            _ => panic!("expected sentinel command"),
        }
    }

    #[test]
    fn sentinel_cli_accepts_required_agent_network_boundary_without_changing_default() {
        let cli =
            Cli::try_parse_from(["a2ctl", "sentinel", "--require-agent-network-boundary"]).unwrap();
        match cli.command {
            Commands::Sentinel {
                workspace,
                require_demo_evidence,
                require_agent_network_boundary,
                demo_archive,
                demo_evidence_json,
            } => {
                assert_eq!(workspace, ".");
                assert!(!require_demo_evidence);
                assert!(require_agent_network_boundary);
                assert_eq!(demo_archive, DEFAULT_ARCHIVE_RESULTS_JSONL);
                assert_eq!(demo_evidence_json, DEFAULT_ARCHIVE_EVIDENCE_JSON);
            }
            _ => panic!("expected sentinel command"),
        }
        assert_eq!(
            agent_network_boundary_command_args(),
            vec![
                "bench/agent_network_boundary_check.py".to_string(),
                "--require-sandbox-runtime".to_string(),
            ]
        );
    }

    #[test]
    fn sentinel_advisory_is_non_gating_and_not_pass_shaped() {
        let advisories = sentinel_non_gating_advisories();
        let advisory = advisories
            .iter()
            .find(|line| line.contains("[INFO] agent_network_boundary"))
            .expect("agent network boundary advisory should be present");
        assert!(advisory.contains("[INFO] agent_network_boundary"));
        assert!(advisory.contains("not part of the 6/6 sentinel gate"));
        assert!(advisory.contains("agent_network_boundary_check.py --self-test"));
        assert!(advisory.contains("--require-sandbox-runtime"));
        assert!(advisory.contains("--require-agent-network-boundary"));
        assert!(!advisory.contains("[PASS]"));
        assert!(!advisory.contains("Sentinel gate: PASS"));
    }

    #[test]
    fn sentinel_demo_evidence_advisory_is_non_gating_and_copy_pasteable() {
        let advisories = sentinel_non_gating_advisories();
        let advisory = advisories
            .iter()
            .find(|line| line.contains("[INFO] demo_evidence"))
            .expect("demo evidence advisory should be present");

        assert!(advisory.contains("not part of the 6/6 sentinel gate"));
        assert!(advisory.contains("python3 bench/self_correction_demo.py verify-demo-docs"));
        assert!(advisory.contains("python3 bench/self_correction_demo.py audit-demo-evidence"));
        assert!(advisory.contains("python3 bench/self_correction_demo.py audit-demo-evidence --json"));
        assert!(advisory.contains(
            "python3 bench/self_correction_demo.py verify-archive --evidence-json docs/benchmark-results/self-correction/a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.demo-evidence.json"
        ));
        assert!(advisory.contains("--require-demo-evidence"));
        assert!(advisory.contains("sentinel default does not refresh or replace those checks"));
        assert!(!advisory.contains("[PASS]"));
        assert!(!advisory.contains("Sentinel gate: PASS"));
    }

    #[test]
    fn sentinel_advisory_block_keeps_demo_evidence_visible_without_extra_gate() {
        let block = render_sentinel_non_gating_advisory_block();
        let lines: Vec<&str> = block.lines().collect();

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Non-gating advisory checks:");
        assert!(lines[1].contains("[INFO] agent_network_boundary"));
        assert!(lines[2].contains("[INFO] demo_evidence"));
        assert!(block.contains("python3 bench/self_correction_demo.py verify-demo-docs"));
        assert!(block.contains("python3 bench/self_correction_demo.py audit-demo-evidence"));
        assert!(block.contains("python3 bench/self_correction_demo.py audit-demo-evidence --json"));
        assert!(block.contains(
            "python3 bench/self_correction_demo.py verify-archive --evidence-json docs/benchmark-results/self-correction/a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.demo-evidence.json"
        ));
        assert!(block.contains("--require-demo-evidence"));
        assert!(block.contains("sentinel default does not refresh or replace those checks"));
        assert!(!block.contains("[PASS]"));
        assert!(!block.contains("[FAIL]"));
        assert!(!block.contains("Sentinel gate:"));
    }

    fn normalize_sentinel_output_snapshot(output: &str) -> String {
        output
            .lines()
            .map(|line| {
                if line.starts_with("Workspace: ") {
                    "Workspace: <workspace>".to_string()
                } else {
                    normalize_long_hex_runs(&normalize_iso_timestamps(&normalize_absolute_paths(
                        line,
                    )))
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    fn normalize_absolute_paths(line: &str) -> String {
        let chars: Vec<char> = line.chars().collect();
        let mut normalized = String::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '/' && (i == 0 || chars[i - 1].is_whitespace()) {
                normalized.push_str("<path>");
                i += 1;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !matches!(chars[i], '`' | ',' | ';' | ')')
                {
                    i += 1;
                }
            } else {
                normalized.push(chars[i]);
                i += 1;
            }
        }
        normalized
    }

    fn normalize_iso_timestamps(line: &str) -> String {
        let chars: Vec<char> = line.chars().collect();
        let mut normalized = String::new();
        let mut i = 0;
        while i < chars.len() {
            let looks_like_timestamp = i + 19 < chars.len()
                && chars[i..i + 4].iter().all(|c| c.is_ascii_digit())
                && chars[i + 4] == '-'
                && chars[i + 5..i + 7].iter().all(|c| c.is_ascii_digit())
                && chars[i + 7] == '-'
                && chars[i + 8..i + 10].iter().all(|c| c.is_ascii_digit())
                && chars[i + 10] == 'T';
            if looks_like_timestamp {
                normalized.push_str("<timestamp>");
                i += 11;
                while i < chars.len()
                    && (chars[i].is_ascii_digit()
                        || matches!(chars[i], ':' | '.' | '-' | '+' | 'Z'))
                {
                    i += 1;
                }
            } else {
                normalized.push(chars[i]);
                i += 1;
            }
        }
        normalized
    }

    fn normalize_long_hex_runs(line: &str) -> String {
        let chars: Vec<char> = line.chars().collect();
        let mut normalized = String::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i].is_ascii_hexdigit() {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
                if i - start >= 12 {
                    normalized.push_str("<hex>");
                } else {
                    normalized.extend(chars[start..i].iter());
                }
            } else {
                normalized.push(chars[i]);
                i += 1;
            }
        }
        normalized
    }

    #[test]
    fn sentinel_full_pass_output_snapshot_normalizes_volatile_fields() {
        let result = a2_eval::sentinel::SuiteResult {
            results: vec![
                a2_eval::sentinel::SentinelResult {
                    name: "compile_check".into(),
                    passed: true,
                    detail: "compiled /tmp/a2-worktree at 2026-07-04T18:15:00Z commit 0123456789abcdef0123456789abcdef01234567".into(),
                },
                a2_eval::sentinel::SentinelResult {
                    name: "unit_tests".into(),
                    passed: true,
                    detail: "tests passed".into(),
                },
                a2_eval::sentinel::SentinelResult {
                    name: "unsafe_check".into(),
                    passed: true,
                    detail: "no unsafe blocks".into(),
                },
                a2_eval::sentinel::SentinelResult {
                    name: "clippy".into(),
                    passed: true,
                    detail: "clippy clean".into(),
                },
                a2_eval::sentinel::SentinelResult {
                    name: "docs".into(),
                    passed: true,
                    detail: "docs build".into(),
                },
                a2_eval::sentinel::SentinelResult {
                    name: "lockfile".into(),
                    passed: true,
                    detail: "lockfile current".into(),
                },
            ],
            all_passed: true,
            score: 1.0,
        };

        let snapshot = normalize_sentinel_output_snapshot(&render_sentinel_output(
            "/private/tmp/a2-workspace-2026-07-04T18:15:00Z",
            &result,
        ));

        let expected = [
            "A² Seed Sentinel Suite".to_string(),
            "Workspace: <workspace>".to_string(),
            String::new(),
            "  [PASS] compile_check: compiled <path> at <timestamp> commit <hex>".to_string(),
            "  [PASS] unit_tests: tests passed".to_string(),
            "  [PASS] unsafe_check: no unsafe blocks".to_string(),
            "  [PASS] clippy: clippy clean".to_string(),
            "  [PASS] docs: docs build".to_string(),
            "  [PASS] lockfile: lockfile current".to_string(),
            String::new(),
            "Score: 100% (6/6)".to_string(),
            String::new(),
            render_sentinel_non_gating_advisory_block()
                .trim_end()
                .to_string(),
            "Sentinel gate: PASS".to_string(),
        ]
        .join("\n")
            + "\n";

        assert_eq!(snapshot, expected);
        assert_eq!(snapshot.matches("[PASS]").count(), 6);
        assert_eq!(snapshot.matches("[INFO]").count(), 2);
        assert!(snapshot.contains("Score: 100% (6/6)"));
        assert!(snapshot.contains("[INFO] demo_evidence"));
        assert!(snapshot.contains("python3 bench/self_correction_demo.py verify-demo-docs"));
        assert!(snapshot.contains("python3 bench/self_correction_demo.py audit-demo-evidence"));
        assert!(snapshot.contains("python3 bench/self_correction_demo.py audit-demo-evidence --json"));
        assert!(snapshot.contains(
            "python3 bench/self_correction_demo.py verify-archive --evidence-json docs/benchmark-results/self-correction/a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.demo-evidence.json"
        ));
        assert!(snapshot.contains("--require-demo-evidence"));
        assert!(snapshot.contains("--require-agent-network-boundary"));
        assert!(snapshot.contains("sentinel default does not refresh or replace those checks"));
    }

    fn complete_demo_evidence_value() -> serde_json::Value {
        serde_json::json!({
            "artifact": DEFAULT_ARCHIVE_RESULTS_JSONL,
            "artifact_sha256": "33a83345adac350b9a79bdd7842ac0c0cad1b698f7fc636a8a12f0c32fe7cee3",
            "complete": true,
            "demos": [
                {
                    "causal_chain": [
                        {
                            "requirement": "failed_first_attempt",
                            "status": "proved",
                            "selector": {"run_id": "run-a", "task_id": "task-a", "attempt": 1},
                            "evidence_row": {
                                "run_id": "run-a",
                                "task_id": "task-a",
                                "attempt": 1,
                                "resolved": false,
                                "verify_returncode": 1
                            }
                        },
                        {
                            "requirement": "archived_verifier_failure_evidence",
                            "status": "proved",
                            "selector": {"run_id": "run-a", "task_id": "task-a", "attempt": 1},
                            "evidence_row": {
                                "run_id": "run-a",
                                "task_id": "task-a",
                                "attempt": 1,
                                "resolved": false,
                                "verify_returncode": 1
                            },
                            "fields": {
                                "lineage_advanced": true,
                                "lineage_records_before": 0,
                                "lineage_records_after": 1
                            }
                        },
                        {
                            "requirement": "retry_context_from_failure_evidence",
                            "status": "proved",
                            "archived_failure_selector": {"run_id": "run-a", "task_id": "task-a", "attempt": 1},
                            "archived_failure_artifact_sha256": "33a83345adac350b9a79bdd7842ac0c0cad1b698f7fc636a8a12f0c32fe7cee3",
                            "selectors": [{"run_id": "run-a", "task_id": "task-a", "attempt": 2}],
                            "evidence_rows": [
                                {
                                    "run_id": "run-a",
                                    "task_id": "task-a",
                                    "attempt": 2,
                                    "resolved": true,
                                    "prior_lineage_present": true,
                                    "lineage_records_before": 1,
                                    "verify_returncode": 0
                                }
                            ],
                            "fields": [
                                {
                                    "derived_from_failed_lineage": true,
                                    "archived_verifier_failure_evidence": true,
                                    "retry_context_links_archived_failure": true,
                                    "prior_lineage_present": true,
                                    "failed_verify_returncode": 1,
                                    "failed_lineage_records_after": 1,
                                    "lineage_records_before": 1,
                                    "attempt": 2,
                                    "failed_attempt_selector": {"run_id": "run-a", "task_id": "task-a", "attempt": 1}
                                }
                            ]
                        },
                        {
                            "requirement": "later_passing_attempt",
                            "status": "proved",
                            "selector": {"run_id": "run-a", "task_id": "task-a", "attempt": 2},
                            "evidence_row": {
                                "run_id": "run-a",
                                "task_id": "task-a",
                                "attempt": 2,
                                "resolved": true,
                                "verify_returncode": 0
                            }
                        },
                        {
                            "requirement": "lineage_trajectory_recorded",
                            "status": "proved",
                            "evidence_rows": [
                                {
                                    "run_id": "run-a",
                                    "task_id": "task-a",
                                    "attempt": 1,
                                    "resolved": false,
                                    "verify_returncode": 1
                                },
                                {
                                    "run_id": "run-a",
                                    "task_id": "task-a",
                                    "attempt": 2,
                                    "resolved": true,
                                    "verify_returncode": 0
                                }
                            ],
                            "fields": {
                                "attempts": [1, 2],
                                "lineage_records_before": 0,
                                "lineage_records_after": 2
                            }
                        },
                        {
                            "requirement": "verifier_gated_germline_promotion",
                            "status": "proved",
                            "selector": {"run_id": "run-a", "task_id": "task-a", "attempt": 2},
                            "evidence_row": {
                                "run_id": "run-a",
                                "task_id": "task-a",
                                "attempt": 2,
                                "verify_returncode": 0,
                                "lineage_reconciled_by_core": true,
                                "promotion_evidence_present": true
                            },
                            "fields": {
                                "verify_returncode": 0,
                                "lineage_reconciled_by_core": true,
                                "promotion_evidence_present": true
                            }
                        }
                    ]
                }
            ]
        })
    }

    fn failed_artifact_payload() -> serde_json::Value {
        serde_json::json!({
            "run_id": "run-a",
            "task_id": "task-a",
            "attempt": 1,
            "resolved": false,
            "prior_lineage_present": false,
            "a2_returncode": 0,
            "verify_returncode": 1,
            "verify_command": "cargo test -p a2_archive",
            "touched_files": [],
            "diff_added_lines": 0,
            "diff_removed_lines": 0,
            "lineage_records_before": 0,
            "lineage_records_after": 1,
            "lineage_reconciled_by_core": false,
            "verifier_failure_evidence_present": true
        })
    }

    fn promotion_artifact_payload(promotion_evidence_present: bool) -> serde_json::Value {
        serde_json::json!({
            "run_id": "run-a",
            "task_id": "task-a",
            "attempt": 2,
            "resolved": true,
            "prior_lineage_present": true,
            "a2_returncode": 0,
            "verify_returncode": 0,
            "verify_command": "cargo test -p a2_archive",
            "touched_files": ["a2-autopoietic-autocatalysis/crates/a2_archive/src/journal.rs"],
            "diff_added_lines": 2,
            "diff_removed_lines": 2,
            "lineage_records_before": 1,
            "lineage_records_after": 2,
            "lineage_reconciled_by_core": true,
            "promotion_evidence_present": promotion_evidence_present
        })
    }

    fn complete_demo_evidence_with_artifact_rows(
        artifact: &str,
        rows: &[serde_json::Value],
        embedded_promotion_row: serde_json::Value,
    ) -> (serde_json::Value, Vec<u8>) {
        let failed_row = failed_artifact_payload();
        let mut artifact_rows = vec![failed_row.clone()];
        artifact_rows.extend_from_slice(rows);
        let artifact_text = artifact_rows
            .iter()
            .map(|row| serde_json::to_string(row).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let mut evidence = complete_demo_evidence_value();
        let artifact_sha256 = format!("{:x}", Sha256::digest(artifact_text.as_bytes()));
        let promotion_row = rows
            .first()
            .cloned()
            .unwrap_or_else(|| promotion_artifact_payload(true));
        let normalized_failed_row = normalized_demo_evidence_row_from_payload(&failed_row);
        let normalized_promotion_row = normalized_demo_evidence_row_from_payload(&promotion_row);
        evidence["artifact"] = serde_json::Value::String(artifact.to_string());
        evidence["artifact_sha256"] = serde_json::Value::String(artifact_sha256.clone());
        evidence["demos"][0]["causal_chain"][0]["evidence_row"] = normalized_failed_row.clone();
        evidence["demos"][0]["causal_chain"][1]["evidence_row"] = normalized_failed_row.clone();
        evidence["demos"][0]["causal_chain"][2]["archived_failure_artifact_sha256"] =
            serde_json::Value::String(artifact_sha256);
        evidence["demos"][0]["causal_chain"][2]["evidence_rows"] =
            serde_json::Value::Array(vec![normalized_promotion_row.clone()]);
        evidence["demos"][0]["causal_chain"][3]["evidence_row"] = normalized_promotion_row.clone();
        evidence["demos"][0]["causal_chain"][4]["evidence_rows"] =
            serde_json::Value::Array(vec![normalized_failed_row, normalized_promotion_row]);
        evidence["demos"][0]["causal_chain"][5]["evidence_row"] = embedded_promotion_row;
        (evidence, artifact_text.into_bytes())
    }

    #[test]
    fn demo_evidence_command_forwards_archive_and_evidence_paths() {
        assert_eq!(
            demo_evidence_command_args("custom/results.jsonl", "custom/evidence.json"),
            vec![
                "bench/self_correction_demo.py".to_string(),
                "verify-archive".to_string(),
                "--archive".to_string(),
                "custom/results.jsonl".to_string(),
                "--evidence-json".to_string(),
                "custom/evidence.json".to_string(),
            ]
        );
    }

    fn project_workspace_root() -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn demo_evidence_cli_paths_allow_default_archive_and_evidence() {
        validate_demo_evidence_cli_paths(
            &project_workspace_root(),
            DEFAULT_ARCHIVE_RESULTS_JSONL,
            DEFAULT_ARCHIVE_EVIDENCE_JSON,
        )
        .unwrap();
    }

    #[test]
    fn demo_evidence_cli_paths_reject_custom_archive_with_default_evidence() {
        let err = validate_demo_evidence_cli_paths(
            &project_workspace_root(),
            "custom/results.jsonl",
            DEFAULT_ARCHIVE_EVIDENCE_JSON,
        )
        .unwrap_err();

        assert!(err.contains(
            "custom demo archive `custom/results.jsonl` requires an explicit non-default"
        ));
        assert!(err.contains(DEFAULT_ARCHIVE_EVIDENCE_JSON));
    }

    #[test]
    fn demo_evidence_cli_paths_reject_custom_archive_with_default_evidence_aliases() {
        let workspace = project_workspace_root();
        let relative_alias = format!("./{DEFAULT_ARCHIVE_EVIDENCE_JSON}");
        let err =
            validate_demo_evidence_cli_paths(&workspace, "custom/results.jsonl", &relative_alias)
                .unwrap_err();
        assert!(err.contains("custom demo archive `custom/results.jsonl` requires"));

        let absolute_default = fs::canonicalize(resolve_workspace_path(
            &workspace,
            DEFAULT_ARCHIVE_EVIDENCE_JSON,
        ))
        .unwrap();
        let err = validate_demo_evidence_cli_paths(
            &workspace,
            "custom/results.jsonl",
            absolute_default.to_str().unwrap(),
        )
        .unwrap_err();
        assert!(err.contains("refusing to rewrite the canonical archived evidence JSON"));
    }

    #[test]
    fn demo_evidence_cli_paths_allow_custom_archive_with_custom_evidence() {
        validate_demo_evidence_cli_paths(
            &project_workspace_root(),
            "custom/results.jsonl",
            "custom/evidence.json",
        )
        .unwrap();
    }

    #[test]
    fn demo_evidence_cli_paths_allow_custom_only_workspace_without_default_artifacts() {
        let workspace = std::env::temp_dir().join(format!(
            "a2-custom-demo-evidence-paths-{}",
            std::process::id()
        ));
        let custom_dir = workspace.join("custom");
        fs::create_dir_all(&custom_dir).unwrap();
        fs::write(custom_dir.join("results.jsonl"), b"{}\n").unwrap();
        fs::write(custom_dir.join("evidence.json"), b"{}\n").unwrap();

        validate_demo_evidence_cli_paths(
            workspace.to_str().unwrap(),
            "custom/results.jsonl",
            "custom/evidence.json",
        )
        .unwrap();

        fs::remove_dir_all(workspace).unwrap();
    }

    fn complete_demo_evidence_verifier_output(
        summary: &DemoEvidenceContractSummary,
        evidence_json: &str,
    ) -> String {
        format!(
            "Self-Correction Benchmark\n  artifact: {artifact}\n  PASS complete self-correction demo trajectory found\nDemo evidence contract check\n  evidence: {evidence_json}\n  mode: archived historical provider evidence; no fresh run-id provenance check requested\n  PASS evidence JSON matches archived demo contract (requirements=6, demos={demos})\n  proved: {proof_chain}\nPASS clean-room evidence regeneration: temp output was absent before scoring; normalized SHA-256 matches checked-in evidence\n",
            artifact = summary.artifact,
            demos = summary.demos,
            proof_chain = DEMO_EVIDENCE_PROOF_STEPS.join(" -> "),
        )
    }

    #[test]
    fn demo_evidence_contract_output_requires_clean_room_and_proof_chain() {
        let summary = DemoEvidenceContractSummary {
            artifact: DEFAULT_ARCHIVE_RESULTS_JSONL.to_string(),
            artifact_sha256: "33a83345adac350b9a79bdd7842ac0c0cad1b698f7fc636a8a12f0c32fe7cee3"
                .to_string(),
            demos: 2,
        };
        let evidence_json = DEFAULT_ARCHIVE_EVIDENCE_JSON;
        let output = complete_demo_evidence_verifier_output(&summary, evidence_json);

        validate_demo_evidence_contract_output(&output, evidence_json, &summary).unwrap();

        let missing_clean_room = output.replace(
            "PASS clean-room evidence regeneration",
            "SKIP clean-room evidence regeneration",
        );
        let err =
            validate_demo_evidence_contract_output(&missing_clean_room, evidence_json, &summary)
                .unwrap_err();
        assert!(err.contains("PASS clean-room evidence regeneration"));

        let missing_proof_chain = output.replace(
            &DEMO_EVIDENCE_PROOF_STEPS.join(" -> "),
            "failed_first_attempt -> later_passing_attempt",
        );
        let err =
            validate_demo_evidence_contract_output(&missing_proof_chain, evidence_json, &summary)
                .unwrap_err();
        assert!(err.contains("retry_context_from_failure_evidence"));
    }

    #[test]
    fn demo_evidence_contract_output_requires_demo_count_from_artifact_summary() {
        let summary = DemoEvidenceContractSummary {
            artifact: DEFAULT_ARCHIVE_RESULTS_JSONL.to_string(),
            artifact_sha256: "33a83345adac350b9a79bdd7842ac0c0cad1b698f7fc636a8a12f0c32fe7cee3"
                .to_string(),
            demos: 2,
        };
        let evidence_json = DEFAULT_ARCHIVE_EVIDENCE_JSON;
        let output = complete_demo_evidence_verifier_output(&summary, evidence_json)
            .replace("requirements=6, demos=2", "requirements=6, demos=1");

        let err =
            validate_demo_evidence_contract_output(&output, evidence_json, &summary).unwrap_err();
        assert!(err.contains("requirements=6, demos=2"));
    }

    #[test]
    fn demo_evidence_contract_accepts_complete_structural_chain() {
        let summary = validate_demo_evidence_value(&complete_demo_evidence_value()).unwrap();
        assert_eq!(summary.artifact, DEFAULT_ARCHIVE_RESULTS_JSONL);
        assert_eq!(summary.demos, 1);
    }

    #[test]
    fn demo_evidence_contract_rejects_reordered_causal_chain() {
        let mut evidence = complete_demo_evidence_value();
        evidence["demos"][0]["causal_chain"]
            .as_array_mut()
            .unwrap()
            .swap(1, 2);

        let err = validate_demo_evidence_value(&evidence).unwrap_err();
        assert!(err.contains("causal_chain proof steps must be ordered"));
        assert!(err.contains("archived_verifier_failure_evidence"));
    }

    #[test]
    fn demo_evidence_artifact_validation_requires_promotion_row_match() {
        let workspace =
            std::env::temp_dir().join(format!("a2-promotion-row-match-{}", std::process::id()));
        let artifact = "evidence/results.jsonl";
        let artifact_path = workspace.join(artifact);
        std::fs::create_dir_all(artifact_path.parent().unwrap()).unwrap();
        let promotion_row = promotion_artifact_payload(true);
        let (evidence, artifact_bytes) = complete_demo_evidence_with_artifact_rows(
            artifact,
            std::slice::from_ref(&promotion_row),
            normalized_demo_evidence_row_from_payload(&promotion_row),
        );
        std::fs::write(&artifact_path, artifact_bytes).unwrap();
        let evidence_path = workspace.join("evidence.json");
        std::fs::write(&evidence_path, serde_json::to_string(&evidence).unwrap()).unwrap();

        let summary = validate_demo_evidence_contract_artifact(
            workspace.to_str().unwrap(),
            evidence_path.to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(summary.demos, 1);

        std::fs::remove_file(evidence_path).unwrap();
        std::fs::remove_file(artifact_path).unwrap();
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn demo_evidence_artifact_validation_rejects_embedded_row_missing_sandbox_audit_fields() {
        let workspace = std::env::temp_dir().join(format!(
            "a2-promotion-row-sandbox-audit-missing-{}",
            std::process::id()
        ));
        let artifact = "evidence/results.jsonl";
        let artifact_path = workspace.join(artifact);
        std::fs::create_dir_all(artifact_path.parent().unwrap()).unwrap();
        let mut promotion_row = promotion_artifact_payload(true);
        promotion_row["audited_sandbox_provider_allowlist_enforced"] =
            serde_json::Value::Bool(true);
        promotion_row["audited_sandbox_provider_allowlist_status"] =
            serde_json::Value::String("enforced".to_string());
        promotion_row["audited_sandbox_provider_allowlist_evidence"] = serde_json::json!({
            "status": "enforced",
            "enforcement_layer": "test sandbox wrapper",
            "launch_boundary": "candidate-worktree agent subprocess",
            "benchmark_network_policy": "Isolated",
            "provider_endpoint_allowlist_enforced": true,
            "allowed_provider_endpoints": ["https://api.openai.com"],
            "public_solution_egress_blocked": true,
            "blocked_solution_hosts": ["github.com", "githubusercontent.com", "github.io"],
            "sandbox_profile_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        });
        let mut embedded_row = normalized_demo_evidence_row_from_payload(&promotion_row);
        embedded_row
            .as_object_mut()
            .unwrap()
            .remove("audited_sandbox_provider_allowlist_evidence");
        let (evidence, artifact_bytes) = complete_demo_evidence_with_artifact_rows(
            artifact,
            std::slice::from_ref(&promotion_row),
            embedded_row,
        );
        std::fs::write(&artifact_path, artifact_bytes).unwrap();
        let evidence_path = workspace.join("evidence.json");
        std::fs::write(&evidence_path, serde_json::to_string(&evidence).unwrap()).unwrap();

        let err = validate_demo_evidence_contract_artifact(
            workspace.to_str().unwrap(),
            evidence_path.to_str().unwrap(),
        )
        .unwrap_err();
        assert!(
            err.contains("verifier_gated_germline_promotion evidence_row does not match"),
            "unexpected error: {err}"
        );

        std::fs::remove_file(evidence_path).unwrap();
        std::fs::remove_file(artifact_path).unwrap();
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn demo_evidence_row_normalization_omits_malformed_optional_provenance_and_audit_fields() {
        let mut row = promotion_artifact_payload(true);
        row["resolved"] = serde_json::Value::String("true".to_string());
        row["prior_lineage_present"] = serde_json::Value::String("true".to_string());
        row["lineage_reconciled_by_core"] = serde_json::Value::String("false".to_string());
        row["no_external_solution_search"] = serde_json::Value::String("true".to_string());
        row["network_policy"] = serde_json::Value::Array(vec![]);
        row["benchmark_source"] = serde_json::Value::String(String::new());
        row["senior_swe_bench_export_sha256"] = serde_json::json!({});
        row["senior_swe_bench_export_row_index"] = serde_json::Value::String("9".to_string());
        row["audited_sandbox_provider_allowlist_enforced"] =
            serde_json::Value::String("true".to_string());
        row["audited_sandbox_provider_allowlist_status"] = serde_json::Value::Array(vec![]);
        row["audited_sandbox_provider_allowlist_evidence"] =
            serde_json::Value::String("not-a-map".to_string());

        let normalized = normalized_demo_evidence_row_from_payload(&row);
        let object = normalized.as_object().unwrap();
        assert_eq!(
            object.get("resolved").and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            object
                .get("prior_lineage_present")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            object.get("lineage_reconciled_by_core"),
            Some(&serde_json::Value::Null)
        );
        for field in [
            "no_external_solution_search",
            "network_policy",
            "benchmark_source",
            "senior_swe_bench_export_sha256",
            "senior_swe_bench_export_row_index",
            "audited_sandbox_provider_allowlist_enforced",
            "audited_sandbox_provider_allowlist_status",
            "audited_sandbox_provider_allowlist_evidence",
        ] {
            assert!(
                !object.contains_key(field),
                "{field} should be omitted when malformed"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn demo_evidence_contract_accepts_requested_archive_symlink_to_evidence_artifact() {
        let workspace = std::env::temp_dir().join(format!(
            "a2-requested-archive-symlink-{}",
            std::process::id()
        ));
        let artifact = "evidence/results.jsonl";
        let artifact_path = workspace.join(artifact);
        std::fs::create_dir_all(artifact_path.parent().unwrap()).unwrap();
        let promotion_row = promotion_artifact_payload(true);
        let (evidence, artifact_bytes) = complete_demo_evidence_with_artifact_rows(
            artifact,
            std::slice::from_ref(&promotion_row),
            normalized_demo_evidence_row_from_payload(&promotion_row),
        );
        std::fs::write(&artifact_path, artifact_bytes).unwrap();
        let evidence_path = workspace.join("evidence.json");
        std::fs::write(&evidence_path, serde_json::to_string(&evidence).unwrap()).unwrap();
        let requested_archive = workspace.join("requested-link.jsonl");
        std::os::unix::fs::symlink(&artifact_path, &requested_archive).unwrap();

        let summary = validate_demo_evidence_contract_artifact_for_archive(
            workspace.to_str().unwrap(),
            evidence_path.to_str().unwrap(),
            requested_archive.to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(summary.artifact, artifact);

        std::fs::remove_file(requested_archive).unwrap();
        std::fs::remove_file(evidence_path).unwrap();
        std::fs::remove_file(artifact_path).unwrap();
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn demo_evidence_contract_rejects_requested_archive_that_differs_from_evidence_artifact() {
        let workspace = std::env::temp_dir().join(format!(
            "a2-requested-archive-mismatch-{}",
            std::process::id()
        ));
        let artifact = "evidence/results.jsonl";
        let artifact_path = workspace.join(artifact);
        let requested_archive = workspace.join("requested/results.jsonl");
        std::fs::create_dir_all(artifact_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(requested_archive.parent().unwrap()).unwrap();
        let promotion_row = promotion_artifact_payload(true);
        let (evidence, artifact_bytes) = complete_demo_evidence_with_artifact_rows(
            artifact,
            std::slice::from_ref(&promotion_row),
            normalized_demo_evidence_row_from_payload(&promotion_row),
        );
        std::fs::write(&artifact_path, &artifact_bytes).unwrap();
        std::fs::write(&requested_archive, artifact_bytes).unwrap();
        let evidence_path = workspace.join("evidence.json");
        std::fs::write(&evidence_path, serde_json::to_string(&evidence).unwrap()).unwrap();

        let err = validate_demo_evidence_contract_artifact_for_archive(
            workspace.to_str().unwrap(),
            evidence_path.to_str().unwrap(),
            requested_archive.to_str().unwrap(),
        )
        .unwrap_err();
        assert!(
            err.contains("does not match requested archive"),
            "unexpected error: {err}"
        );

        std::fs::remove_file(evidence_path).unwrap();
        std::fs::remove_file(requested_archive).unwrap();
        std::fs::remove_file(artifact_path).unwrap();
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn demo_evidence_artifact_validation_rejects_embedded_failed_row_missing_sandbox_audit_fields()
    {
        let workspace = std::env::temp_dir().join(format!(
            "a2-failed-row-sandbox-audit-missing-{}",
            std::process::id()
        ));
        let artifact = "evidence/results.jsonl";
        let artifact_path = workspace.join(artifact);
        std::fs::create_dir_all(artifact_path.parent().unwrap()).unwrap();
        let promotion_row = promotion_artifact_payload(true);
        let (mut evidence, artifact_bytes) = complete_demo_evidence_with_artifact_rows(
            artifact,
            std::slice::from_ref(&promotion_row),
            normalized_demo_evidence_row_from_payload(&promotion_row),
        );
        let mut failed_row = failed_artifact_payload();
        failed_row["audited_sandbox_provider_allowlist_enforced"] = serde_json::Value::Bool(true);
        failed_row["audited_sandbox_provider_allowlist_status"] =
            serde_json::Value::String("enforced".to_string());
        failed_row["audited_sandbox_provider_allowlist_evidence"] = serde_json::json!({
            "status": "enforced",
            "enforcement_layer": "test sandbox wrapper",
            "launch_boundary": "candidate-worktree agent subprocess",
            "benchmark_network_policy": "Isolated",
            "provider_endpoint_allowlist_enforced": true,
            "allowed_provider_endpoints": ["https://api.openai.com"],
            "public_solution_egress_blocked": true,
            "blocked_solution_hosts": ["github.com", "githubusercontent.com", "github.io"],
            "sandbox_profile_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        });
        let artifact_text = [failed_row.clone(), promotion_row]
            .iter()
            .map(|row| serde_json::to_string(row).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let artifact_sha256 = format!("{:x}", Sha256::digest(artifact_text.as_bytes()));
        evidence["artifact_sha256"] = serde_json::Value::String(artifact_sha256.clone());
        evidence["demos"][0]["causal_chain"][2]["archived_failure_artifact_sha256"] =
            serde_json::Value::String(artifact_sha256);
        std::fs::write(&artifact_path, artifact_text).unwrap();
        let evidence_path = workspace.join("evidence.json");
        std::fs::write(&evidence_path, serde_json::to_string(&evidence).unwrap()).unwrap();

        let err = validate_demo_evidence_contract_artifact(
            workspace.to_str().unwrap(),
            evidence_path.to_str().unwrap(),
        )
        .unwrap_err();
        assert!(
            err.contains("failed_first_attempt evidence_row does not match"),
            "unexpected error: {err}"
        );

        drop(artifact_bytes);
        std::fs::remove_file(evidence_path).unwrap();
        std::fs::remove_file(artifact_path).unwrap();
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn demo_evidence_artifact_validation_accepts_numeric_string_attempt_row() {
        let workspace = std::env::temp_dir().join(format!(
            "a2-promotion-row-string-attempt-{}",
            std::process::id()
        ));
        let artifact = "evidence/results.jsonl";
        let artifact_path = workspace.join(artifact);
        std::fs::create_dir_all(artifact_path.parent().unwrap()).unwrap();
        let mut promotion_row = promotion_artifact_payload(true);
        promotion_row["attempt"] = serde_json::Value::String("2".to_string());
        let (evidence, artifact_bytes) = complete_demo_evidence_with_artifact_rows(
            artifact,
            std::slice::from_ref(&promotion_row),
            normalized_demo_evidence_row_from_payload(&promotion_row),
        );
        std::fs::write(&artifact_path, artifact_bytes).unwrap();
        let evidence_path = workspace.join("evidence.json");
        std::fs::write(&evidence_path, serde_json::to_string(&evidence).unwrap()).unwrap();

        let summary = validate_demo_evidence_contract_artifact(
            workspace.to_str().unwrap(),
            evidence_path.to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(summary.demos, 1);

        std::fs::remove_file(evidence_path).unwrap();
        std::fs::remove_file(artifact_path).unwrap();
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn demo_evidence_contract_rejects_malformed_promotion_selector_attempt() {
        let mut evidence = complete_demo_evidence_value();
        evidence["demos"][0]["causal_chain"][5]["selector"]["attempt"] =
            serde_json::Value::String("2".to_string());

        let err = validate_demo_evidence_value(&evidence).unwrap_err();
        assert!(
            err.contains("verifier_gated_germline_promotion.selector.attempt must be an integer")
        );
    }

    #[test]
    fn demo_evidence_artifact_validation_rejects_spoofed_embedded_promotion_row() {
        let workspace =
            std::env::temp_dir().join(format!("a2-promotion-row-spoof-{}", std::process::id()));
        let artifact = "evidence/results.jsonl";
        let artifact_path = workspace.join(artifact);
        std::fs::create_dir_all(artifact_path.parent().unwrap()).unwrap();
        let promotion_row = promotion_artifact_payload(false);
        let mut embedded_row = normalized_demo_evidence_row_from_payload(&promotion_row);
        embedded_row["promotion_evidence_present"] = serde_json::Value::Bool(true);
        let (evidence, artifact_bytes) =
            complete_demo_evidence_with_artifact_rows(artifact, &[promotion_row], embedded_row);
        std::fs::write(&artifact_path, artifact_bytes).unwrap();
        let evidence_path = workspace.join("evidence.json");
        std::fs::write(&evidence_path, serde_json::to_string(&evidence).unwrap()).unwrap();

        let err = validate_demo_evidence_contract_artifact(
            workspace.to_str().unwrap(),
            evidence_path.to_str().unwrap(),
        )
        .unwrap_err();
        assert!(
            err.contains("promotion evidence_row does not match the selected JSONL artifact row")
        );

        std::fs::remove_file(evidence_path).unwrap();
        std::fs::remove_file(artifact_path).unwrap();
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn demo_evidence_artifact_validation_rejects_duplicate_promotion_selector_rows() {
        let workspace =
            std::env::temp_dir().join(format!("a2-promotion-row-duplicate-{}", std::process::id()));
        let artifact = "evidence/results.jsonl";
        let artifact_path = workspace.join(artifact);
        std::fs::create_dir_all(artifact_path.parent().unwrap()).unwrap();
        let promotion_row = promotion_artifact_payload(true);
        let (evidence, artifact_bytes) = complete_demo_evidence_with_artifact_rows(
            artifact,
            &[promotion_row.clone(), promotion_row.clone()],
            normalized_demo_evidence_row_from_payload(&promotion_row),
        );
        std::fs::write(&artifact_path, artifact_bytes).unwrap();
        let evidence_path = workspace.join("evidence.json");
        std::fs::write(&evidence_path, serde_json::to_string(&evidence).unwrap()).unwrap();

        let err = validate_demo_evidence_contract_artifact(
            workspace.to_str().unwrap(),
            evidence_path.to_str().unwrap(),
        )
        .unwrap_err();
        assert!(err.contains("selector matched 2 JSONL artifact rows; expected exactly one"));

        std::fs::remove_file(evidence_path).unwrap();
        std::fs::remove_file(artifact_path).unwrap();
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn demo_evidence_contract_rejects_retry_context_without_archived_failure_link() {
        let mut evidence = complete_demo_evidence_value();
        evidence["demos"][0]["causal_chain"][2]["fields"][0]["retry_context_links_archived_failure"] =
            serde_json::Value::Bool(false);

        let err = validate_demo_evidence_value(&evidence).unwrap_err();
        assert!(err.contains("retry_context_links_archived_failure expected true"));
    }

    #[test]
    fn demo_evidence_contract_rejects_missing_retry_evidence_rows() {
        let mut evidence = complete_demo_evidence_value();
        evidence["demos"][0]["causal_chain"][2]
            .as_object_mut()
            .unwrap()
            .remove("evidence_rows");

        let err = validate_demo_evidence_value(&evidence).unwrap_err();
        assert!(err.contains("retry_context_from_failure_evidence.evidence_rows must be an array"));
    }

    #[test]
    fn demo_evidence_contract_rejects_retry_evidence_row_from_other_run() {
        let mut evidence = complete_demo_evidence_value();
        evidence["demos"][0]["causal_chain"][2]["evidence_rows"][0]["run_id"] =
            serde_json::Value::String("other-run".to_string());

        let err = validate_demo_evidence_value(&evidence).unwrap_err();
        assert!(err.contains("retry evidence row selector does not match retry selector"));
    }

    #[test]
    fn demo_evidence_contract_rejects_lineage_evidence_row_from_other_run() {
        let mut evidence = complete_demo_evidence_value();
        evidence["demos"][0]["causal_chain"][4]["evidence_rows"][1]["run_id"] =
            serde_json::Value::String("other-run".to_string());

        let err = validate_demo_evidence_value(&evidence).unwrap_err();
        assert!(err.contains("lineage evidence row is not in the failed run/task trajectory"));
    }

    #[test]
    fn demo_evidence_contract_rejects_non_later_passing_attempt() {
        let mut evidence = complete_demo_evidence_value();
        evidence["demos"][0]["causal_chain"][3]["selector"]["attempt"] = serde_json::json!(1);

        let err = validate_demo_evidence_value(&evidence).unwrap_err();
        assert!(err.contains("later passing attempt must occur after failed first attempt"));
    }

    #[test]
    fn demo_evidence_contract_rejects_lineage_attempts_without_later_pass() {
        let mut evidence = complete_demo_evidence_value();
        evidence["demos"][0]["causal_chain"][4]["fields"]["attempts"] = serde_json::json!([1, 3]);

        let err = validate_demo_evidence_value(&evidence).unwrap_err();
        assert!(err.contains("lineage trajectory must span failed and later attempts"));
    }

    #[test]
    fn demo_evidence_contract_rejects_promotion_not_tied_to_later_pass() {
        let mut evidence = complete_demo_evidence_value();
        evidence["demos"][0]["causal_chain"][5]["selector"]["attempt"] = serde_json::json!(3);

        let err = validate_demo_evidence_value(&evidence).unwrap_err();
        assert!(
            err.contains("verifier-gated promotion selector does not match later passing attempt")
        );
    }

    #[test]
    fn demo_evidence_contract_rejects_promotion_field_spoof_without_row_evidence() {
        let mut evidence = complete_demo_evidence_value();
        evidence["demos"][0]["causal_chain"][5]["evidence_row"]["promotion_evidence_present"] =
            serde_json::Value::Bool(false);

        let err = validate_demo_evidence_value(&evidence).unwrap_err();
        assert!(err.contains("verifier-gated promotion lacks gated apply evidence"));
    }

    #[test]
    fn demo_evidence_contract_rejects_missing_referenced_jsonl_artifact() {
        let workspace =
            std::env::temp_dir().join(format!("a2-missing-demo-artifact-{}", std::process::id()));
        std::fs::create_dir_all(&workspace).unwrap();
        let evidence_path = workspace.join("evidence.json");
        std::fs::write(
            &evidence_path,
            serde_json::to_string(&complete_demo_evidence_value()).unwrap(),
        )
        .unwrap();

        let err = validate_demo_evidence_contract_artifact(
            workspace.to_str().unwrap(),
            evidence_path.to_str().unwrap(),
        )
        .unwrap_err();
        assert!(err.contains("failed to read referenced JSONL artifact"));

        std::fs::remove_file(evidence_path).unwrap();
        std::fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn demo_evidence_contract_rejects_referenced_jsonl_hash_mismatch() {
        let workspace = std::env::temp_dir().join(format!(
            "a2-mismatched-demo-artifact-{}",
            std::process::id()
        ));
        let artifact_path = workspace.join(DEFAULT_ARCHIVE_RESULTS_JSONL);
        std::fs::create_dir_all(artifact_path.parent().unwrap()).unwrap();
        std::fs::write(&artifact_path, b"stale or substituted jsonl\n").unwrap();
        let evidence_path = workspace.join("evidence.json");
        std::fs::write(
            &evidence_path,
            serde_json::to_string(&complete_demo_evidence_value()).unwrap(),
        )
        .unwrap();

        let err = validate_demo_evidence_contract_artifact(
            workspace.to_str().unwrap(),
            evidence_path.to_str().unwrap(),
        )
        .unwrap_err();
        assert!(err.contains("demo evidence artifact hash mismatch"));

        std::fs::remove_file(evidence_path).unwrap();
        std::fs::remove_file(artifact_path).unwrap();
        std::fs::remove_dir_all(workspace).unwrap();
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
    fn parses_run_network_policy_cli_values() {
        assert_eq!(
            parse_network_policy_arg("isolated").unwrap(),
            a2_core::protocol::NetworkPolicy::Isolated
        );
        assert_eq!(
            parse_network_policy_arg("Open").unwrap(),
            a2_core::protocol::NetworkPolicy::Open
        );
        assert_eq!(
            parse_network_policy_arg("allowlist:https://api.openai.com, https://api.anthropic.com")
                .unwrap(),
            a2_core::protocol::NetworkPolicy::AllowList(vec![
                "https://api.openai.com".into(),
                "https://api.anthropic.com".into(),
            ])
        );
        assert!(parse_network_policy_arg("allowlist:").is_err());
        assert!(parse_network_policy_arg("prompt-only").is_err());
    }

    #[test]
    fn restricted_network_policy_without_candidate_is_cli_failure() {
        assert!(restricted_policy_without_candidate(
            Some(&a2_core::protocol::NetworkPolicy::Isolated),
            false,
        ));
        assert!(restricted_policy_without_candidate(
            Some(&a2_core::protocol::NetworkPolicy::AllowList(vec![
                "https://api.openai.com".into(),
            ])),
            false,
        ));
        assert!(
            !restricted_policy_without_candidate(
                Some(&a2_core::protocol::NetworkPolicy::Isolated),
                true,
            ),
            "a future sandboxed provider run that produces a candidate must not be treated as a launch block"
        );
        assert!(!restricted_policy_without_candidate(
            Some(&a2_core::protocol::NetworkPolicy::Open),
            false,
        ));
        assert!(!restricted_policy_without_candidate(None, false));
    }

    #[test]
    fn json_run_input_sets_no_external_solution_search_guard() {
        let ingester = a2_sensorium::ingest::Ingester::new(build_budget(50_000, 300));
        let task = task_from_run_input(
            &ingester,
            parse_run_input(
                r#"{"task_id":"senior-swe-bench-1","problem_statement":"Fix benchmark task","no_external_solution_search":true,"network_policy":"Isolated"}"#,
            ),
        );

        assert!(task.no_external_solution_search);
        assert_eq!(
            task.network_policy,
            Some(a2_core::protocol::NetworkPolicy::Isolated)
        );
    }

    #[test]
    fn run_network_policy_cli_applies_to_plain_text_tasks() {
        let ingester = a2_sensorium::ingest::Ingester::new(build_budget(50_000, 300));
        let task = task_from_run_input_with_network_policy(
            &ingester,
            parse_run_input("Fix Senior SWE Bench task without public solution lookup"),
            Some(a2_core::protocol::NetworkPolicy::Isolated),
        );

        assert_eq!(
            task.network_policy,
            Some(a2_core::protocol::NetworkPolicy::Isolated)
        );
    }

    #[test]
    fn run_network_policy_cli_overrides_json_task_policy() {
        let ingester = a2_sensorium::ingest::Ingester::new(build_budget(50_000, 300));
        let task = task_from_run_input_with_network_policy(
            &ingester,
            parse_run_input(
                r#"{"task_id":"bench-1","problem_statement":"Fix task","network_policy":"Open"}"#,
            ),
            Some(a2_core::protocol::NetworkPolicy::Isolated),
        );

        assert_eq!(
            task.network_policy,
            Some(a2_core::protocol::NetworkPolicy::Isolated)
        );
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
            network_policy: Some(a2_core::protocol::NetworkPolicy::Isolated),
            started_at: "2026-05-25T12:00:00Z".into(),
            completed_at: "2026-05-25T12:01:00Z".into(),
            total_iterations: 2,
            total_tokens: 4500,
            total_duration_secs: 30.5,
            patches_produced: 1,
            applied_count: 1,
            verified_count: 0,
            stop_reason: "repeated_failure_class: decision='error: timeout', patch_produced=false"
                .into(),
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
                    checklist_update: Some(ChecklistUpdateSummary {
                        path: "docs/plans/work.md".into(),
                        line: 4,
                        status: "marked_complete".into(),
                    }),
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
                    checklist_update: None,
                },
            ],
        };

        let json = serde_json::to_string_pretty(&summary).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["run_id"], "run-20260525T120000Z");
        assert_eq!(parsed["total_iterations"], 2);
        assert_eq!(parsed["total_tokens"], 4500);
        assert_eq!(parsed["network_policy"], "Isolated");
        assert_eq!(parsed["patches_produced"], 1);
        assert_eq!(
            parsed["stop_reason"],
            "repeated_failure_class: decision='error: timeout', patch_produced=false"
        );
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
        assert_eq!(
            parsed["iterations"][0]["checklist_update"]["status"],
            "marked_complete"
        );
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
            network_policy: None,
            started_at: "2026-05-25T12:00:00Z".into(),
            completed_at: "2026-05-25T12:00:30Z".into(),
            total_iterations: 1,
            total_tokens: 500,
            total_duration_secs: 5.0,
            patches_produced: 1,
            applied_count: 1,
            verified_count: 1,
            stop_reason: "completed".into(),
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
                checklist_update: None,
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
    fn resident_autopilot_args_forward_bounded_loop_options() {
        let config = ResidentAutopilotConfig {
            workspace: "/tmp/ws".into(),
            provider: "pi/zai/glm-5.1,opencode/minimax".into(),
            max_iterations: 2,
            max_tokens: 90000,
            timeout: 1200,
            interval_secs: 30,
            max_runs: 4,
            apply: false,
            network_policy: Some(a2_core::protocol::NetworkPolicy::AllowList(vec![
                "https://api.openai.com".into(),
            ])),
            task: vec!["improve summaries".into()],
            task_file: vec!["task.md".into()],
            dry_run: true,
            log_dir: ".a2/autopilot".into(),
        };

        let args = resident_autopilot_args(&config);

        assert_eq!(args[0], "autopilot");
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--workspace", "/tmp/ws"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--max-iterations", "2"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--max-tokens", "90000"])
        );
        assert!(args.windows(2).any(|pair| pair == ["--timeout", "1200"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--task", "improve summaries"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--task-file", "task.md"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--network-policy", "allowlist:https://api.openai.com"])
        );
        assert!(args.iter().any(|arg| arg == "--dry-run"));
        assert!(!args.iter().any(|arg| arg == "--apply"));
    }

    #[test]
    fn autopilot_resident_dir_lives_under_log_base() {
        let root = PathBuf::from("/tmp/a2-workspace");
        let relative = autopilot_resident_dir(&root, Path::new(".a2/autopilot"));
        assert!(relative.starts_with("/tmp/a2-workspace/.a2/autopilot/resident"));
        assert!(
            relative
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("resident-")
        );

        let absolute = autopilot_resident_dir(&root, Path::new("/tmp/a2-autopilot-logs"));
        assert!(absolute.starts_with("/tmp/a2-autopilot-logs/resident"));
    }

    #[test]
    fn autopilot_run_index_appends_dashboard_record_and_latest_pointer() {
        let root = unique_test_dir("autopilot-index");
        let run_dir = root.join(".a2/autopilot/runs/run-test-index");
        std::fs::create_dir_all(&run_dir).unwrap();
        let summary = AutopilotRunSummary {
            run_id: "run-test-index".into(),
            workspace: "/tmp/ws".into(),
            provider: "pi/zai/glm-5.1".into(),
            max_iterations: 2,
            network_policy: None,
            started_at: "2026-05-26T12:00:00Z".into(),
            completed_at: "2026-05-26T12:01:00Z".into(),
            total_iterations: 1,
            total_tokens: 1234,
            total_duration_secs: 42.0,
            patches_produced: 1,
            applied_count: 1,
            verified_count: 1,
            stop_reason: "completed".into(),
            iterations: vec![AutopilotIterationSummary {
                iteration: 1,
                task_id: "task-abc".into(),
                candidate_id: "autopilot:explicit:abc".into(),
                candidate_source: "--task[0]".into(),
                candidate_title: "Improve logs".into(),
                model: "pi/zai/glm-5.1".into(),
                tokens: 1234,
                duration_secs: 42.0,
                decision: "promote_germline::Prompt".into(),
                patch_produced: true,
                patch_stats: None,
                verifier_focus: vec![],
                apply_ok: true,
                verify_ok: true,
                apply_note: None,
                checklist_update: None,
            }],
        };

        append_autopilot_run_index(&run_dir, &summary).unwrap();

        let base_dir = root.join(".a2/autopilot");
        let index = std::fs::read_to_string(base_dir.join("run_index.jsonl")).unwrap();
        let records = index.lines().collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        let record: serde_json::Value = serde_json::from_str(records[0]).unwrap();
        assert_eq!(record["run_id"], "run-test-index");
        assert_eq!(record["total_tokens"], 1234);
        assert_eq!(record["verified_count"], 1);
        assert_eq!(record["iterations"][0]["candidate_title"], "Improve logs");
        assert_eq!(record["iterations"][0]["verify_ok"], true);
        assert!(
            record["events_path"]
                .as_str()
                .unwrap()
                .ends_with("events.jsonl")
        );
        assert!(
            record["summary_path"]
                .as_str()
                .unwrap()
                .ends_with("run_summary.json")
        );

        let latest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(base_dir.join("latest_run.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(latest["run_id"], "run-test-index");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checklist_source_location_accepts_only_project_checklists() {
        assert_eq!(
            checklist_source_location("docs/plans/continuous-self-iteration.md:40"),
            Some(("docs/plans/continuous-self-iteration.md", 40))
        );
        assert_eq!(
            checklist_source_location("todos/autopilot.md:2"),
            Some(("todos/autopilot.md", 2))
        );
        assert_eq!(checklist_source_location("--task[0]"), None);
        assert_eq!(checklist_source_location("scan"), None);
        assert_eq!(checklist_source_location("docs/plans/file.md:0"), None);
    }

    #[test]
    fn mark_checklist_item_completed_checks_only_target_line() {
        let root = unique_test_dir("checklist-update");
        std::fs::create_dir_all(root.join("docs/plans")).unwrap();
        std::fs::write(
            root.join("docs/plans/work.md"),
            "# Work\n\n- [ ] first\n- [ ] second\n",
        )
        .unwrap();
        let candidate = AutopilotCandidate {
            id: "autopilot:checklist:docs/plans/work.md:4".into(),
            title: "second".into(),
            description: "second".into(),
            source: "docs/plans/work.md:4".into(),
        };

        let update = mark_checklist_item_completed(&root, &candidate)
            .unwrap()
            .unwrap();

        assert_eq!(update.path, "docs/plans/work.md");
        assert_eq!(update.line, 4);
        assert_eq!(update.status, "marked_complete");
        let content = std::fs::read_to_string(root.join("docs/plans/work.md")).unwrap();
        assert!(content.contains("- [ ] first"));
        assert!(content.contains("- [x] second"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mark_checklist_item_completed_ignores_non_checklist_sources() {
        let root = unique_test_dir("checklist-update-ignore");
        let candidate = AutopilotCandidate {
            id: "autopilot:explicit:abc".into(),
            title: "explicit".into(),
            description: "explicit".into(),
            source: "--task[0]".into(),
        };

        let update = mark_checklist_item_completed(&root, &candidate).unwrap();

        assert_eq!(update, None);
        std::fs::remove_dir_all(root).ok();
    }

    fn autopilot_iteration_for_stop_test(
        tokens: u64,
        decision: &str,
        patch_produced: bool,
        verify_ok: bool,
    ) -> AutopilotIterationSummary {
        AutopilotIterationSummary {
            iteration: 1,
            task_id: "task".into(),
            candidate_id: "candidate".into(),
            candidate_source: "test".into(),
            candidate_title: "test".into(),
            model: "test/noop".into(),
            tokens,
            duration_secs: 1.0,
            decision: decision.into(),
            patch_produced,
            patch_stats: None,
            verifier_focus: vec![],
            apply_ok: false,
            verify_ok,
            apply_note: None,
            checklist_update: None,
        }
    }

    #[test]
    fn autopilot_stop_reason_detects_budget_exhaustion() {
        let summaries = vec![autopilot_iteration_for_stop_test(
            101,
            "discard (task not completed)",
            true,
            false,
        )];

        let reason = autopilot_stop_reason(&summaries, 100, 3).unwrap();

        assert!(reason.contains("budget_exhausted"));
        assert!(reason.contains("used 101 tokens"));
    }

    #[test]
    fn autopilot_stop_reason_detects_provider_quota_failure() {
        let summaries = vec![autopilot_iteration_for_stop_test(
            0,
            "error: provider returned 429 insufficient balance",
            false,
            false,
        )];

        let reason = autopilot_stop_reason(&summaries, 1000, 3).unwrap();

        assert!(reason.contains("provider_quota_failure"));
    }

    #[test]
    fn autopilot_stop_reason_detects_repeated_failure_class() {
        let summaries = vec![
            autopilot_iteration_for_stop_test(10, "discard (task not completed)", true, false),
            autopilot_iteration_for_stop_test(12, "discard (task not completed)", true, false),
        ];

        let reason = autopilot_stop_reason(&summaries, 1000, 3).unwrap();

        assert!(reason.contains("repeated_failure_class"));
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
