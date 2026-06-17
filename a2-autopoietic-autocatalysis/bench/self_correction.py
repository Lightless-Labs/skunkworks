#!/usr/bin/env python3
"""Run A²'s first loop-shaped self-correction benchmark.

The harness creates an isolated git worktree, injects a deterministic bug, commits
that bug only in the worktree branch, and then runs repeated `a2ctl run --apply`
attempts with the same JSONL `task_id`. Each attempt is evaluated immediately and
emitted as one JSON object.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
import unittest
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from uuid import UUID

CATEGORY = "self_correction"
FIBONACCI_TASK_ID = "self-correction-fibonacci-regression"
FIBONACCI_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_core test_fibonacci` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
function name is the location of the bug; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2_core test_fibonacci` before
finishing.
"""
FIBONACCI_VERIFY_COMMAND = "cargo test -p a2_core test_fibonacci"
FIBONACCI_BUG_OLD = "if n == 0 {\n        return 0;\n    }"
FIBONACCI_BUG_NEW = "if n == 0 {\n        return 1;\n    }"
CORE_TASK_ID = "self-correction-compound-core-same-crate-hidden-regressions"
CORE_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_core test_fibonacci` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
function name is the only broken behavior; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2_core test_fibonacci` before
finishing.
"""
CORE_SUMMARY_BUG_OLD = "self.task_completed, self.tests_pass, self.tokens_used, self.duration_secs"
CORE_SUMMARY_BUG_NEW = "self.task_completed, self.tests_pass, self.tokens_used + 1, self.duration_secs"
SCAN_BUG_OLD = "if byte == b'\"' {\n            in_double = true;\n            index += 1;\n            continue;\n        }"
SCAN_BUG_NEW = "if byte == b'\"' {\n            index += 1;\n            continue;\n        }"
MEMBRANE_BUG_OLD = 'if cap.denied_tools.iter().any(|d| d == tool_name || d == "*") {\n            return false;\n        }'
MEMBRANE_BUG_NEW = 'if cap.denied_tools.iter().any(|d| d == tool_name || d == "*") {\n            return true;\n        }'
ARCHIVE_BUG_OLD = "FROM lineage_records\n                WHERE task_id = ?1\n                ORDER BY created_at ASC"
ARCHIVE_BUG_NEW = "FROM lineage_records\n                WHERE task_id = ?1\n                ORDER BY created_at DESC"
ARCHIVE_SAME_CRATE_TASK_ID = "self-correction-compound-archive-same-crate-hidden-regressions"
ARCHIVE_SAME_CRATE_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_archive returns_history_in_reverse_chronological_order` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
test name is the only broken behavior; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2_archive returns_history_in_reverse_chronological_order` before
finishing.
"""
ARCHIVE_INDEX_TASK_ID = "self-correction-compound-archive-index-hidden-regressions"
ARCHIVE_INDEX_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_archive returns_history_in_reverse_chronological_order` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
test name is the only broken behavior; inspect the archive persistence code and
make the minimal change that restores the test. Run `cargo test -p a2_archive returns_history_in_reverse_chronological_order` before
finishing.
"""
ARCHIVE_JOURNAL_HISTORY_BUG_OLD = """\
                ORDER BY promoted_at DESC
                LIMIT ?1
"""
ARCHIVE_JOURNAL_HISTORY_BUG_NEW = """\
                ORDER BY promoted_at ASC
                LIMIT ?1
"""
ARCHIVE_SCHEMA_EXTERNAL_VERIFICATIONS_BUG_OLD = """\
    ensure_optional_column(
        connection,
        "lineage_records",
        "external_verifications_json",
        "TEXT",
    )?;
"""
ARCHIVE_SCHEMA_EXTERNAL_VERIFICATIONS_BUG_NEW = """\
    ensure_optional_column(
        connection,
        "lineage_records",
        "external_verification_json",
        "TEXT",
    )?;
"""
ARCHIVE_SCHEMA_RECENT_INDEX_BUG_OLD = """\
            CREATE INDEX IF NOT EXISTS idx_lineage_records_created_at
            ON lineage_records(created_at DESC);
"""
ARCHIVE_SCHEMA_RECENT_INDEX_BUG_NEW = """\
            CREATE INDEX IF NOT EXISTS idx_lineage_records_created_at
            ON lineage_records(created_at ASC);
"""
ARCHIVE_RECENT_INDEX_VERIFY_COMMAND = (
    "python3 -c \"from pathlib import Path; "
    "text = Path('crates/a2_archive/src/schema.rs').read_text(); "
    "needle = 'CREATE INDEX IF NOT EXISTS idx_lineage_records_created_at\\\\n            ON lineage_records(created_at DESC);'; "
    "raise SystemExit(0 if needle in text else 'crates/a2_archive/src/schema.rs: idx_lineage_records_created_at must keep created_at DESC for recent lineage ordering')\""
)
SENSORIUM_TASK_ID = "self-correction-compound-sensorium-same-crate-hidden-regressions"
SENSORIUM_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_sensorium high_risk_gets_low_priority` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
function name is the location of the bug; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2_sensorium high_risk_gets_low_priority` before
finishing.
"""
SENSORIUM_PRIORITY_BUG_OLD = "RiskTier::High => Priority::Low, // Untrusted signals get lower priority."
SENSORIUM_PRIORITY_BUG_NEW = "RiskTier::High => Priority::Normal, // Untrusted signals get lower priority."
SENSORIUM_TRUNCATE_BUG_OLD = "let mut t = s[..max - 3].to_string();"
SENSORIUM_TRUNCATE_BUG_NEW = "let mut t = s[..max].to_string();"
RAF_TASK_ID = "self-correction-compound-raf-same-crate-hidden-regressions"
RAF_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_raf single_node_is_not_repairable` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
function name is the location of the bug; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2_raf single_node_is_not_repairable` before
finishing.
"""
RAF_SINGLE_NODE_BUG_OLD = """\
        if node_count < 2 {
            return false;
        }
"""
RAF_SINGLE_NODE_BUG_NEW = """\
        if node_count < 2 {
            return true;
        }
"""
RAF_EMPTY_COVERAGE_BUG_OLD = """\
        if node_count == 0 {
            return 0.0;
        }

        let covered = self
"""
RAF_EMPTY_COVERAGE_BUG_NEW = """\
        if node_count == 0 {
            return 1.0;
        }

        let covered = self
"""
EVAL_TASK_ID = "self-correction-compound-eval-same-crate-hidden-regressions"
EVAL_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_eval failing_tests_score_incomplete` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
test name is the only broken behavior; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2_eval failing_tests_score_incomplete` before
finishing.
"""
EVAL_TESTS_BUG_OLD = "results.failed == 0"
EVAL_TESTS_BUG_NEW = "results.passed > 0"
EVAL_BUDGET_BUG_OLD = "total <= self.token_ceiling"
EVAL_BUDGET_BUG_NEW = "total >= self.token_ceiling"
BROKER_TASK_ID = "self-correction-compound-broker-same-crate-hidden-regressions"
BROKER_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_broker test_parse_gemini_usage_from_flat_stats` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
test name is the only broken behavior; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2_broker test_parse_gemini_usage_from_flat_stats` before
finishing.
"""
BROKER_GEMINI_OUTPUT_BUG_OLD = """\
    let tokens_out = stats
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
"""
BROKER_GEMINI_OUTPUT_BUG_NEW = """\
    let tokens_out = stats
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
"""
BROKER_PI_CACHE_BUG_OLD = """\
                + usage
                    .get("cacheWrite")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
"""
BROKER_PI_CACHE_BUG_NEW = """\
                + usage
                    .get("cache_write")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
"""
CONSTITUTION_TASK_ID = "self-correction-compound-constitution-same-crate-hidden-regressions"
CONSTITUTION_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_constitution b1_does_not_require_human_review` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
test name is the only broken behavior; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2_constitution b1_does_not_require_human_review` before
finishing.
"""
CONSTITUTION_HUMAN_REVIEW_BUG_OLD = "matches!(self, Self::B0)"
CONSTITUTION_HUMAN_REVIEW_BUG_NEW = "matches!(self, Self::B0 | Self::B1)"
CONSTITUTION_NETWORK_BUG_OLD = '"lineage://archive".to_string(),'
CONSTITUTION_NETWORK_BUG_NEW = '"production://write".to_string(),'
WORKCELL_TASK_ID = "self-correction-compound-workcell-same-crate-hidden-regressions"
WORKCELL_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2_workcell uses_last_diff_block_when_model_self_corrects` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
test name is the only broken behavior; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2_workcell uses_last_diff_block_when_model_self_corrects` before
finishing.
"""
WORKCELL_LAST_DIFF_BUG_OLD = 'if let Some(start) = text.rfind("```diff") {'
WORKCELL_LAST_DIFF_BUG_NEW = 'if let Some(start) = text.find("```diff") {'
WORKCELL_ZERO_BUDGET_BUG_OLD = """\
    if max_lines == 0 {
        return (String::new(), 0, !text.is_empty());
    }
"""
WORKCELL_ZERO_BUDGET_BUG_NEW = """\
    if max_lines == 0 {
        return (text.to_string(), 0, false);
    }
"""
A2D_TASK_ID = "self-correction-compound-a2d-same-crate-hidden-regressions"
A2D_DESCRIPTION = """\
The workspace contains a regression. `cargo test -p a2d flat_rounds_trigger_stagnation_after_window_size` fails.
Diagnose the root cause and fix the implementation. Do not assume the failing
test name is the only broken behavior; inspect the code and make the minimal
change that restores the test. Run `cargo test -p a2d flat_rounds_trigger_stagnation_after_window_size` before
finishing.
"""
A2D_STAGNATION_WINDOW_BUG_OLD = """\
        if window == 0 || self.rounds.len() < window {
            return false;
        }
"""
A2D_STAGNATION_WINDOW_BUG_NEW = """\
        if window == 0 || self.rounds.len() <= window {
            return false;
        }
"""
A2D_VERIFIER_BACKSTOP_BUG_OLD = """\
        if patch.worktree_verifications.is_empty() {
            return false;
        }
"""
A2D_VERIFIER_BACKSTOP_BUG_NEW = """\
        if !patch.worktree_verifications.is_empty() {
            return false;
        }
"""
FNV_OFFSET_128 = 0x6C62_272E_07BB_0142_62B8_2175_6295_C58D
FNV_PRIME_128 = 0x0000_0000_0100_0000_0000_0000_0000_013B


@dataclass(frozen=True)
class Replacement:
    path: str
    old: str
    new: str


@dataclass(frozen=True)
class Fixture:
    name: str
    task_id: str
    description: str
    verify_command: str
    replacements: tuple[Replacement, ...]


FIXTURES: dict[str, Fixture] = {
    "fibonacci": Fixture(
        name="fibonacci",
        task_id=FIBONACCI_TASK_ID,
        description=FIBONACCI_DESCRIPTION,
        verify_command=FIBONACCI_VERIFY_COMMAND,
        replacements=(
            Replacement(
                "crates/a2_core/src/lib.rs",
                FIBONACCI_BUG_OLD,
                FIBONACCI_BUG_NEW,
            ),
        ),
    ),
    "compound-hidden": Fixture(
        name="compound-hidden",
        task_id="self-correction-compound-hidden-regressions",
        description=FIBONACCI_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_core test_fibonacci; core=$?; "
            "cargo test -p a2ctl ignores_non_task_mentions_inside_comments_and_strings; ctl=$?; "
            "test $core -eq 0 -a $ctl -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_core/src/lib.rs",
                FIBONACCI_BUG_OLD,
                FIBONACCI_BUG_NEW,
            ),
            Replacement(
                "crates/a2ctl/src/main.rs",
                SCAN_BUG_OLD,
                SCAN_BUG_NEW,
            ),
        ),
    ),
    "compound-core-same-crate-hidden": Fixture(
        name="compound-core-same-crate-hidden",
        task_id=CORE_TASK_ID,
        description=CORE_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_core test_fibonacci; fib=$?; "
            "cargo test -p a2_core test_somatic_summary; summary=$?; "
            "test $fib -eq 0 -a $summary -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_core/src/lib.rs",
                FIBONACCI_BUG_OLD,
                FIBONACCI_BUG_NEW,
            ),
            Replacement(
                "crates/a2_core/src/protocol.rs",
                CORE_SUMMARY_BUG_OLD,
                CORE_SUMMARY_BUG_NEW,
            ),
        ),
    ),
    "compound-membrane-hidden": Fixture(
        name="compound-membrane-hidden",
        task_id="self-correction-compound-membrane-hidden-regressions",
        description=FIBONACCI_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_core test_fibonacci; core=$?; "
            "cargo test -p a2_membrane deny_overrides_allow; membrane=$?; "
            "test $core -eq 0 -a $membrane -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_core/src/lib.rs",
                FIBONACCI_BUG_OLD,
                FIBONACCI_BUG_NEW,
            ),
            Replacement(
                "crates/a2_membrane/src/policy.rs",
                MEMBRANE_BUG_OLD,
                MEMBRANE_BUG_NEW,
            ),
        ),
    ),
    "compound-archive-hidden": Fixture(
        name="compound-archive-hidden",
        task_id="self-correction-compound-archive-hidden-regressions",
        description=FIBONACCI_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_core test_fibonacci; core=$?; "
            "cargo test -p a2_archive filters_by_task_and_orders_recent_records; archive=$?; "
            "test $core -eq 0 -a $archive -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_core/src/lib.rs",
                FIBONACCI_BUG_OLD,
                FIBONACCI_BUG_NEW,
            ),
            Replacement(
                "crates/a2_archive/src/store.rs",
                ARCHIVE_BUG_OLD,
                ARCHIVE_BUG_NEW,
            ),
        ),
    ),
    "compound-archive-same-crate-hidden": Fixture(
        name="compound-archive-same-crate-hidden",
        task_id=ARCHIVE_SAME_CRATE_TASK_ID,
        description=ARCHIVE_SAME_CRATE_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_archive returns_history_in_reverse_chronological_order; journal=$?; "
            "cargo test -p a2_archive reads_existing_legacy_lineage_rows_with_empty_external_verifications; schema=$?; "
            "test $journal -eq 0 -a $schema -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_archive/src/journal.rs",
                ARCHIVE_JOURNAL_HISTORY_BUG_OLD,
                ARCHIVE_JOURNAL_HISTORY_BUG_NEW,
            ),
            Replacement(
                "crates/a2_archive/src/schema.rs",
                ARCHIVE_SCHEMA_EXTERNAL_VERIFICATIONS_BUG_OLD,
                ARCHIVE_SCHEMA_EXTERNAL_VERIFICATIONS_BUG_NEW,
            ),
        ),
    ),
    "compound-archive-index-hidden": Fixture(
        name="compound-archive-index-hidden",
        task_id=ARCHIVE_INDEX_TASK_ID,
        description=ARCHIVE_INDEX_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_archive returns_history_in_reverse_chronological_order; journal=$?; "
            f"{ARCHIVE_RECENT_INDEX_VERIFY_COMMAND}; schema=$?; "
            "test $journal -eq 0 -a $schema -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_archive/src/journal.rs",
                ARCHIVE_JOURNAL_HISTORY_BUG_OLD,
                ARCHIVE_JOURNAL_HISTORY_BUG_NEW,
            ),
            Replacement(
                "crates/a2_archive/src/schema.rs",
                ARCHIVE_SCHEMA_RECENT_INDEX_BUG_OLD,
                ARCHIVE_SCHEMA_RECENT_INDEX_BUG_NEW,
            ),
        ),
    ),
    "compound-sensorium-same-crate-hidden": Fixture(
        name="compound-sensorium-same-crate-hidden",
        task_id=SENSORIUM_TASK_ID,
        description=SENSORIUM_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_sensorium high_risk_gets_low_priority; priority=$?; "
            "cargo test -p a2_sensorium long_content_truncated_in_title; title=$?; "
            "test $priority -eq 0 -a $title -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_sensorium/src/ingest.rs",
                SENSORIUM_PRIORITY_BUG_OLD,
                SENSORIUM_PRIORITY_BUG_NEW,
            ),
            Replacement(
                "crates/a2_sensorium/src/ingest.rs",
                SENSORIUM_TRUNCATE_BUG_OLD,
                SENSORIUM_TRUNCATE_BUG_NEW,
            ),
        ),
    ),
    "compound-raf-same-crate-hidden": Fixture(
        name="compound-raf-same-crate-hidden",
        task_id=RAF_TASK_ID,
        description=RAF_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_raf single_node_is_not_repairable; single=$?; "
            "cargo test -p a2_raf empty_graph_has_no_coverage_or_connectivity; empty=$?; "
            "test $single -eq 0 -a $empty -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_raf/src/graph.rs",
                RAF_SINGLE_NODE_BUG_OLD,
                RAF_SINGLE_NODE_BUG_NEW,
            ),
            Replacement(
                "crates/a2_raf/src/graph.rs",
                RAF_EMPTY_COVERAGE_BUG_OLD,
                RAF_EMPTY_COVERAGE_BUG_NEW,
            ),
        ),
    ),
    "compound-eval-same-crate-hidden": Fixture(
        name="compound-eval-same-crate-hidden",
        task_id=EVAL_TASK_ID,
        description=EVAL_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_eval failing_tests_score_incomplete; tests=$?; "
            "cargo test -p a2_eval over_budget_scores_incomplete; budget=$?; "
            "test $tests -eq 0 -a $budget -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_eval/src/seed.rs",
                EVAL_TESTS_BUG_OLD,
                EVAL_TESTS_BUG_NEW,
            ),
            Replacement(
                "crates/a2_eval/src/seed.rs",
                EVAL_BUDGET_BUG_OLD,
                EVAL_BUDGET_BUG_NEW,
            ),
        ),
    ),
    "compound-broker-same-crate-hidden": Fixture(
        name="compound-broker-same-crate-hidden",
        task_id=BROKER_TASK_ID,
        description=BROKER_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_broker test_parse_gemini_usage_from_flat_stats; gemini=$?; "
            "cargo test -p a2_broker test_parse_pi_jsonl_extracts_final_text_and_usage; pi=$?; "
            "test $gemini -eq 0 -a $pi -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_broker/src/broker.rs",
                BROKER_GEMINI_OUTPUT_BUG_OLD,
                BROKER_GEMINI_OUTPUT_BUG_NEW,
            ),
            Replacement(
                "crates/a2_broker/src/broker.rs",
                BROKER_PI_CACHE_BUG_OLD,
                BROKER_PI_CACHE_BUG_NEW,
            ),
        ),
    ),
    "compound-constitution-same-crate-hidden": Fixture(
        name="compound-constitution-same-crate-hidden",
        task_id=CONSTITUTION_TASK_ID,
        description=CONSTITUTION_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_constitution b1_does_not_require_human_review; human=$?; "
            "cargo test -p a2_constitution b2_network_allowlist_is_quarantine_and_lineage_only; network=$?; "
            "test $human -eq 0 -a $network -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_constitution/src/profile.rs",
                CONSTITUTION_HUMAN_REVIEW_BUG_OLD,
                CONSTITUTION_HUMAN_REVIEW_BUG_NEW,
            ),
            Replacement(
                "crates/a2_constitution/src/profile.rs",
                CONSTITUTION_NETWORK_BUG_OLD,
                CONSTITUTION_NETWORK_BUG_NEW,
            ),
        ),
    ),
    "compound-workcell-same-crate-hidden": Fixture(
        name="compound-workcell-same-crate-hidden",
        task_id=WORKCELL_TASK_ID,
        description=WORKCELL_DESCRIPTION,
        verify_command=(
            "cargo test -p a2_workcell uses_last_diff_block_when_model_self_corrects; diff=$?; "
            "cargo test -p a2_workcell zero_line_budget_truncates_all_content; budget=$?; "
            "test $diff -eq 0 -a $budget -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2_workcell/src/catalyst.rs",
                WORKCELL_LAST_DIFF_BUG_OLD,
                WORKCELL_LAST_DIFF_BUG_NEW,
            ),
            Replacement(
                "crates/a2_workcell/src/catalyst.rs",
                WORKCELL_ZERO_BUDGET_BUG_OLD,
                WORKCELL_ZERO_BUDGET_BUG_NEW,
            ),
        ),
    ),
    "compound-a2d-same-crate-hidden": Fixture(
        name="compound-a2d-same-crate-hidden",
        task_id=A2D_TASK_ID,
        description=A2D_DESCRIPTION,
        verify_command=(
            "cargo test -p a2d flat_rounds_trigger_stagnation_after_window_size; stagnation=$?; "
            "cargo test -p a2d candidate_verifier_backstop_promotes_when_mutable_evaluator_is_corrupt; backstop=$?; "
            "test $stagnation -eq 0 -a $backstop -eq 0"
        ),
        replacements=(
            Replacement(
                "crates/a2d/src/lib.rs",
                A2D_STAGNATION_WINDOW_BUG_OLD,
                A2D_STAGNATION_WINDOW_BUG_NEW,
            ),
            Replacement(
                "crates/a2d/src/lib.rs",
                A2D_VERIFIER_BACKSTOP_BUG_OLD,
                A2D_VERIFIER_BACKSTOP_BUG_NEW,
            ),
        ),
    ),
}


@dataclass
class CommandResult:
    command: list[str] | str
    returncode: int
    stdout: str
    stderr: str
    duration_secs: float


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=".", help="Source A² project path containing Cargo.toml.")
    parser.add_argument("--provider", default="gemini", help="Provider/model for a2ctl run.")
    parser.add_argument(
        "--fixture",
        choices=sorted(FIXTURES),
        default="fibonacci",
        help="Bug fixture to inject.",
    )
    parser.add_argument("--attempts", type=int, default=2, help="Number of repeated A² attempts.")
    parser.add_argument("--max-tokens", type=int, default=100_000, help="Per-attempt token budget.")
    parser.add_argument("--timeout", type=int, default=1800, help="Per-attempt timeout in seconds.")
    parser.add_argument("--run-id", default=None, help="Stable run ID for result records.")
    parser.add_argument(
        "--results",
        default="bench/self-correction-results.jsonl",
        help="Path for JSONL attempt results.",
    )
    parser.add_argument("--workdir", default=None, help="Use this isolated git worktree root path.")
    parser.add_argument("--keep-workspace", action="store_true", help="Do not remove the worktree.")
    parser.add_argument(
        "--smoke-only",
        action="store_true",
        help="Create/inject/evaluate the bugged workspace without calling a model.",
    )
    parser.add_argument(
        "--disable-anti-repeat",
        action="store_true",
        help=(
            "Ablation mode: pass --disable-anti-repeat-retry to a2ctl run, "
            "leaving candidate verifiers and other retry context enabled."
        ),
    )
    return parser.parse_args(argv)


def run_command(
    command: list[str] | str,
    cwd: Path,
    *,
    stdin: str | None = None,
    timeout: int | None = None,
    shell: bool = False,
) -> CommandResult:
    start = time.monotonic()
    process = subprocess.run(
        command,
        cwd=str(cwd),
        input=stdin,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        shell=shell,
        executable="/bin/bash" if shell else None,
        env={**os.environ, "PYTHONUNBUFFERED": "1"},
    )
    return CommandResult(
        command=command,
        returncode=process.returncode,
        stdout=process.stdout,
        stderr=process.stderr,
        duration_secs=time.monotonic() - start,
    )


def git(args: list[str], cwd: Path) -> CommandResult:
    result = run_command(["git", *args], cwd)
    if result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result


def repo_root(path: Path) -> Path:
    result = git(["rev-parse", "--show-toplevel"], path)
    return Path(result.stdout.strip()).resolve()


def deterministic_task_uuid(key: str, prefix: str = "task") -> str:
    hash_value = FNV_OFFSET_128
    for byte in prefix.encode("utf-8") + b"\0" + key.encode("utf-8"):
        hash_value ^= byte
        hash_value = (hash_value * FNV_PRIME_128) % (1 << 128)

    raw = bytearray(hash_value.to_bytes(16, "big"))
    raw[6] = (raw[6] & 0x0F) | 0x80
    raw[8] = (raw[8] & 0x3F) | 0x80
    return str(UUID(bytes=bytes(raw)))


def serialized_task_id(task_id: str) -> str:
    return json.dumps(deterministic_task_uuid(task_id))


def lineage_count(workspace: Path, task_id: str) -> int:
    db = workspace / "lineage.sqlite"
    if not db.exists():
        return 0

    with sqlite3.connect(db) as connection:
        try:
            row = connection.execute(
                "SELECT COUNT(*) FROM lineage_records WHERE task_id = ?",
                (serialized_task_id(task_id),),
            ).fetchone()
        except sqlite3.Error:
            return 0
    return int(row[0]) if row else 0


def latest_lineage_patch_diff(workspace: Path, task_id: str) -> str | None:
    db = workspace / "lineage.sqlite"
    if not db.exists():
        return None

    with sqlite3.connect(db) as connection:
        try:
            row = connection.execute(
                """
                SELECT patch_diff
                FROM lineage_records
                WHERE task_id = ?
                ORDER BY created_at DESC
                LIMIT 1
                """,
                (serialized_task_id(task_id),),
            ).fetchone()
        except sqlite3.Error:
            return None
    return row[0] if row and row[0] else None


def diff_stats(diff: str | None) -> dict[str, Any]:
    touched_files: list[str] = []
    touched_seen: set[str] = set()
    added_lines = 0
    removed_lines = 0

    if diff:
        for line in diff.splitlines():
            if line.startswith("diff --git "):
                parts = line.split()
                if len(parts) >= 4:
                    path = parts[3]
                    if path.startswith("b/"):
                        path = path[2:]
                    if path not in touched_seen:
                        touched_seen.add(path)
                        touched_files.append(path)
            elif line.startswith("+++") and not touched_files:
                path = line.removeprefix("+++ ").strip()
                if path.startswith("b/"):
                    path = path[2:]
                if path != "/dev/null" and path not in touched_seen:
                    touched_seen.add(path)
                    touched_files.append(path)
            elif line.startswith("+") and not line.startswith("+++"):
                added_lines += 1
            elif line.startswith("-") and not line.startswith("---"):
                removed_lines += 1

    return {
        "touched_files": touched_files,
        "touched_file_count": len(touched_files),
        "diff_added_lines": added_lines,
        "diff_removed_lines": removed_lines,
    }


def create_worktree(source_repo: Path, destination: Path, branch: str) -> Path:
    git(["worktree", "add", "-b", branch, str(destination), "HEAD"], source_repo)
    return destination


def cleanup_worktree(source_repo: Path, workspace: Path, branch: str) -> None:
    subprocess.run(
        ["git", "worktree", "remove", "--force", str(workspace)],
        cwd=str(source_repo),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    subprocess.run(
        ["git", "branch", "-D", branch],
        cwd=str(source_repo),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )


def inject_fixture(workspace: Path, fixture: Fixture) -> None:
    for replacement in fixture.replacements:
        path = workspace / replacement.path
        content = path.read_text(encoding="utf-8")
        if replacement.new in content:
            continue
        if replacement.old not in content:
            raise RuntimeError(f"bug fixture target not found in {path}")
        path.write_text(content.replace(replacement.old, replacement.new, 1), encoding="utf-8")


def commit_bug(workspace: Path, fixture: Fixture) -> None:
    git(["add", "-A"], workspace)
    git(
        [
            "-c",
            "user.name=A2 Self-Correction Benchmark",
            "-c",
            "user.email=a2-self-correction@example.invalid",
            "commit",
            "-m",
            f"bench: inject {fixture.name} regression",
        ],
        workspace,
    )


def task_payload(fixture: Fixture, run_id: str, attempt: int) -> dict[str, Any]:
    return {
        "task_id": fixture.task_id,
        "problem_statement": fixture.description,
        "verification_commands": [
            {
                "command": fixture.verify_command,
                "expect_exit": 0,
            }
        ],
        "category": CATEGORY,
        "fixture": fixture.name,
        "run_id": run_id,
        "attempt": attempt,
    }


def run_a2_attempt(
    workspace: Path,
    provider: str,
    max_tokens: int,
    timeout: int,
    payload: dict[str, Any],
    *,
    disable_anti_repeat: bool,
) -> CommandResult:
    command = [
        "cargo",
        "run",
        "-p",
        "a2ctl",
        "--",
        "run",
        "--provider",
        provider,
        "--max-tokens",
        str(max_tokens),
        "--timeout",
        str(timeout),
        "--apply",
    ]
    if disable_anti_repeat:
        command.append("--disable-anti-repeat-retry")
    return run_command(command, workspace, stdin=json.dumps(payload) + "\n", timeout=timeout + 900)


def verify(workspace: Path, fixture: Fixture) -> CommandResult:
    return run_command(fixture.verify_command, workspace, shell=True, timeout=300)


def result_record(
    *,
    payload: dict[str, Any],
    provider: str,
    workspace: Path,
    a2_result: CommandResult | None,
    verify_result: CommandResult,
    lineage_before: int,
    lineage_after: int,
    lineage_reconciled_by_core: bool,
    patch_stats: dict[str, Any],
    anti_repeat_retry_enabled: bool,
) -> dict[str, Any]:
    return {
        "task_id": payload["task_id"],
        "category": payload["category"],
        "fixture": payload.get("fixture"),
        "run_id": payload["run_id"],
        "attempt": payload["attempt"],
        "provider": provider,
        "model": provider,
        "resolved": verify_result.returncode == 0,
        "prior_lineage_present": lineage_before > 0,
        "lineage_records_before": lineage_before,
        "lineage_records_after": lineage_after,
        "lineage_reconciled_by_core": lineage_reconciled_by_core,
        "anti_repeat_retry_enabled": anti_repeat_retry_enabled,
        "ablation": None if anti_repeat_retry_enabled else "anti_repeat_retry_disabled",
        **patch_stats,
        "workspace": str(workspace),
        "a2_returncode": a2_result.returncode if a2_result else None,
        "a2_duration_secs": round(a2_result.duration_secs, 3) if a2_result else 0.0,
        "verify_command": str(verify_result.command),
        "verify_returncode": verify_result.returncode,
        "verify_duration_secs": round(verify_result.duration_secs, 3),
        "stdout": "\n\n".join(
            part
            for part in (
                a2_result.stdout if a2_result else "",
                verify_result.stdout,
            )
            if part
        ),
        "stderr": "\n\n".join(
            part
            for part in (
                a2_result.stderr if a2_result else "",
                verify_result.stderr,
            )
            if part
        ),
        "evaluated_at": datetime.now(timezone.utc).isoformat(),
    }


def append_jsonl(path: Path, record: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, sort_keys=True) + "\n")


def run_benchmark(args: argparse.Namespace) -> int:
    fixture = FIXTURES[args.fixture]
    source_project = Path(args.repo).resolve()
    if not (source_project / "Cargo.toml").exists():
        raise RuntimeError(f"--repo must point at the A² project root: {source_project}")

    source_git_root = repo_root(source_project)
    project_relative = source_project.relative_to(source_git_root)
    run_id = args.run_id or datetime.now(timezone.utc).strftime("self-correction-%Y%m%dT%H%M%SZ")
    branch = f"a2-self-correction-{run_id}"
    worktree_root = Path(args.workdir).resolve() if args.workdir else Path(tempfile.mkdtemp(prefix="a2-self-correction-"))
    workspace = worktree_root / project_relative
    results = Path(args.results)
    if not results.is_absolute():
        results = source_project / results

    created = False
    try:
        if worktree_root.exists() and any(worktree_root.iterdir()):
            raise RuntimeError(f"workspace path is not empty: {worktree_root}")
        if worktree_root.exists():
            worktree_root.rmdir()
        create_worktree(source_git_root, worktree_root, branch)
        created = True
        inject_fixture(workspace, fixture)
        commit_bug(workspace, fixture)

        initial = verify(workspace, fixture)
        if initial.returncode == 0:
            raise RuntimeError("bug fixture did not fail before A² attempts")

        attempts = 1 if args.smoke_only else max(args.attempts, 1)
        for attempt in range(1, attempts + 1):
            payload = task_payload(fixture, run_id, attempt)
            lineage_before = lineage_count(workspace, fixture.task_id)
            a2_result = (
                None
                if args.smoke_only
                else run_a2_attempt(
                    workspace,
                    args.provider,
                    args.max_tokens,
                    args.timeout,
                    payload,
                    disable_anti_repeat=args.disable_anti_repeat,
                )
            )
            verified = verify(workspace, fixture)
            patch_stats = diff_stats(latest_lineage_patch_diff(workspace, fixture.task_id))
            lineage_after = lineage_count(workspace, fixture.task_id)
            lineage_reconciled_by_core = a2_result is not None and (
                "[applied and rebuilt:" in a2_result.stderr
                or "[apply/rebuild failed for" in a2_result.stderr
            )
            record = result_record(
                payload=payload,
                provider=args.provider,
                workspace=workspace,
                a2_result=a2_result,
                verify_result=verified,
                lineage_before=lineage_before,
                lineage_after=lineage_after,
                lineage_reconciled_by_core=lineage_reconciled_by_core,
                patch_stats=patch_stats,
                anti_repeat_retry_enabled=not args.disable_anti_repeat,
            )
            append_jsonl(results, record)
            print(json.dumps(record, sort_keys=True))
            if verified.returncode == 0:
                break

        return 0
    finally:
        if created and not args.keep_workspace:
            cleanup_worktree(source_git_root, worktree_root, branch)
        elif not created and worktree_root.exists() and not args.keep_workspace and args.workdir is None:
            shutil.rmtree(worktree_root, ignore_errors=True)


class SelfCorrectionTests(unittest.TestCase):
    def test_deterministic_task_uuid_is_stable(self) -> None:
        self.assertEqual(deterministic_task_uuid("same"), deterministic_task_uuid("same"))
        self.assertNotEqual(deterministic_task_uuid("same"), deterministic_task_uuid("other"))

    def test_task_payload_reuses_id_across_attempts(self) -> None:
        fixture = FIXTURES["fibonacci"]
        first = task_payload(fixture, "run", 1)
        second = task_payload(fixture, "run", 2)
        self.assertEqual(first["task_id"], second["task_id"])
        self.assertEqual(first["run_id"], second["run_id"])
        self.assertEqual(second["attempt"], 2)

    def test_task_payload_carries_fixture_verifier_command(self) -> None:
        fixture = FIXTURES["compound-hidden"]
        payload = task_payload(fixture, "run", 1)
        self.assertEqual(
            payload["verification_commands"],
            [{"command": fixture.verify_command, "expect_exit": 0}],
        )

    def test_compound_archive_fixture_checks_archive_regression(self) -> None:
        fixture = FIXTURES["compound-archive-hidden"]
        self.assertEqual(fixture.task_id, "self-correction-compound-archive-hidden-regressions")
        self.assertIn("a2_archive", fixture.verify_command)
        self.assertIn("filters_by_task_and_orders_recent_records", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2_core/src/lib.rs", "crates/a2_archive/src/store.rs"],
        )

    def test_compound_archive_same_crate_fixture_is_same_crate_multi_bug(self) -> None:
        fixture = FIXTURES["compound-archive-same-crate-hidden"]
        self.assertEqual(fixture.task_id, ARCHIVE_SAME_CRATE_TASK_ID)
        self.assertIn("returns_history_in_reverse_chronological_order", fixture.verify_command)
        self.assertIn("reads_existing_legacy_lineage_rows_with_empty_external_verifications", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2_archive/src/journal.rs", "crates/a2_archive/src/schema.rs"],
        )

    def test_compound_archive_index_fixture_hides_schema_index_check(self) -> None:
        fixture = FIXTURES["compound-archive-index-hidden"]
        self.assertEqual(fixture.task_id, ARCHIVE_INDEX_TASK_ID)
        self.assertIn("returns_history_in_reverse_chronological_order", fixture.verify_command)
        self.assertIn("idx_lineage_records_created_at must keep created_at DESC", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2_archive/src/journal.rs", "crates/a2_archive/src/schema.rs"],
        )

    def test_compound_core_fixture_is_same_crate_multi_bug(self) -> None:
        fixture = FIXTURES["compound-core-same-crate-hidden"]
        self.assertEqual(fixture.task_id, CORE_TASK_ID)
        self.assertIn("test_fibonacci", fixture.verify_command)
        self.assertIn("test_somatic_summary", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2_core/src/lib.rs", "crates/a2_core/src/protocol.rs"],
        )

    def test_compound_sensorium_fixture_is_same_crate_multi_bug(self) -> None:
        fixture = FIXTURES["compound-sensorium-same-crate-hidden"]
        self.assertEqual(fixture.task_id, SENSORIUM_TASK_ID)
        self.assertIn("high_risk_gets_low_priority", fixture.verify_command)
        self.assertIn("long_content_truncated_in_title", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2_sensorium/src/ingest.rs", "crates/a2_sensorium/src/ingest.rs"],
        )

    def test_compound_raf_fixture_is_same_crate_multi_bug(self) -> None:
        fixture = FIXTURES["compound-raf-same-crate-hidden"]
        self.assertEqual(fixture.task_id, RAF_TASK_ID)
        self.assertIn("single_node_is_not_repairable", fixture.verify_command)
        self.assertIn("empty_graph_has_no_coverage_or_connectivity", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2_raf/src/graph.rs", "crates/a2_raf/src/graph.rs"],
        )

    def test_compound_eval_fixture_is_same_crate_multi_bug(self) -> None:
        fixture = FIXTURES["compound-eval-same-crate-hidden"]
        self.assertEqual(fixture.task_id, EVAL_TASK_ID)
        self.assertIn("failing_tests_score_incomplete", fixture.verify_command)
        self.assertIn("over_budget_scores_incomplete", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2_eval/src/seed.rs", "crates/a2_eval/src/seed.rs"],
        )

    def test_compound_broker_fixture_is_same_crate_multi_bug(self) -> None:
        fixture = FIXTURES["compound-broker-same-crate-hidden"]
        self.assertEqual(fixture.task_id, BROKER_TASK_ID)
        self.assertIn("test_parse_gemini_usage_from_flat_stats", fixture.verify_command)
        self.assertIn("test_parse_pi_jsonl_extracts_final_text_and_usage", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2_broker/src/broker.rs", "crates/a2_broker/src/broker.rs"],
        )

    def test_compound_constitution_fixture_is_same_crate_multi_bug(self) -> None:
        fixture = FIXTURES["compound-constitution-same-crate-hidden"]
        self.assertEqual(fixture.task_id, CONSTITUTION_TASK_ID)
        self.assertIn("b1_does_not_require_human_review", fixture.verify_command)
        self.assertIn("b2_network_allowlist_is_quarantine_and_lineage_only", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2_constitution/src/profile.rs", "crates/a2_constitution/src/profile.rs"],
        )

    def test_compound_workcell_fixture_is_same_crate_multi_bug(self) -> None:
        fixture = FIXTURES["compound-workcell-same-crate-hidden"]
        self.assertEqual(fixture.task_id, WORKCELL_TASK_ID)
        self.assertIn("uses_last_diff_block_when_model_self_corrects", fixture.verify_command)
        self.assertIn("zero_line_budget_truncates_all_content", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2_workcell/src/catalyst.rs", "crates/a2_workcell/src/catalyst.rs"],
        )

    def test_compound_a2d_fixture_is_same_crate_multi_bug(self) -> None:
        fixture = FIXTURES["compound-a2d-same-crate-hidden"]
        self.assertEqual(fixture.task_id, A2D_TASK_ID)
        self.assertIn("flat_rounds_trigger_stagnation_after_window_size", fixture.verify_command)
        self.assertIn("candidate_verifier_backstop_promotes_when_mutable_evaluator_is_corrupt", fixture.verify_command)
        self.assertEqual(
            [replacement.path for replacement in fixture.replacements],
            ["crates/a2d/src/lib.rs", "crates/a2d/src/lib.rs"],
        )

    def test_result_record_reports_prior_lineage(self) -> None:
        payload = task_payload(FIXTURES["fibonacci"], "run", 2)
        verify_result = CommandResult(FIBONACCI_VERIFY_COMMAND, 0, "ok", "", 1.25)
        record = result_record(
            payload=payload,
            provider="gemini",
            workspace=Path("/tmp/workspace"),
            a2_result=None,
            verify_result=verify_result,
            lineage_before=1,
            lineage_after=2,
            lineage_reconciled_by_core=True,
            patch_stats={
                "touched_files": ["crates/a2_core/src/lib.rs"],
                "touched_file_count": 1,
                "diff_added_lines": 1,
                "diff_removed_lines": 1,
            },
            anti_repeat_retry_enabled=False,
        )
        self.assertTrue(record["resolved"])
        self.assertTrue(record["prior_lineage_present"])
        self.assertEqual(record["lineage_records_after"], 2)
        self.assertTrue(record["lineage_reconciled_by_core"])
        self.assertFalse(record["anti_repeat_retry_enabled"])
        self.assertEqual(record["ablation"], "anti_repeat_retry_disabled")
        self.assertEqual(record["touched_files"], ["crates/a2_core/src/lib.rs"])
        self.assertEqual(record["diff_added_lines"], 1)
        self.assertEqual(record["diff_removed_lines"], 1)

    def test_diff_stats_reports_touched_files_and_line_counts(self) -> None:
        stats = diff_stats(
            """
diff --git a/crates/a2_core/src/lib.rs b/crates/a2_core/src/lib.rs
--- a/crates/a2_core/src/lib.rs
+++ b/crates/a2_core/src/lib.rs
@@ -1,2 +1,2 @@
-old
+new
diff --git a/crates/a2ctl/src/main.rs b/crates/a2ctl/src/main.rs
--- a/crates/a2ctl/src/main.rs
+++ b/crates/a2ctl/src/main.rs
@@ -10,0 +11,2 @@
+first
+second
"""
        )
        self.assertEqual(
            stats["touched_files"],
            ["crates/a2_core/src/lib.rs", "crates/a2ctl/src/main.rs"],
        )
        self.assertEqual(stats["touched_file_count"], 2)
        self.assertEqual(stats["diff_added_lines"], 3)
        self.assertEqual(stats["diff_removed_lines"], 1)


if __name__ == "__main__":
    if sys.argv[1:2] == ["--self-test"]:
        sys.argv = [sys.argv[0]]
        raise SystemExit(unittest.main())
    raise SystemExit(run_benchmark(parse_args(sys.argv[1:])))
