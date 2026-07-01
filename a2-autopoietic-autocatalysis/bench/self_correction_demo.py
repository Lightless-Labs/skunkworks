#!/usr/bin/env python3
"""Run A²'s auditable self-correction demo gates.

This wrapper has two intentionally separate modes:

* ``verify-archive`` re-scores a durable archived JSONL artifact and proves that it
  contains a failed-attempt -> retry-context -> verified-promotion trajectory.
* ``fresh`` runs the self-correction harness to regenerate a new JSONL artifact,
  then immediately applies the same ``--require-demo`` scorer gate to that output.

The default mode is archive verification because a fresh provider run can be slow
and may consume paid quota. Use ``fresh --print-only`` to inspect the exact command
sequence before running it.
"""

from __future__ import annotations

import argparse
import contextlib
import io
import json
import shlex
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


DEFAULT_ARCHIVE = Path(
    "docs/benchmark-results/self-correction/"
    "a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.jsonl"
)
DEFAULT_ARCHIVE_EVIDENCE = Path(
    "docs/benchmark-results/self-correction/"
    "a2-archive-same-crate-opencode-minimax-m3-20260615T165316Z.demo-evidence.json"
)
DEFAULT_FIXTURE = "compound-archive-same-crate-hidden"
DEFAULT_PROVIDER = "opencode/minimax-coding-plan/MiniMax-M3"


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def display_command(command: list[str]) -> str:
    root = repo_root()
    display: list[str] = []
    for part in command:
        try:
            path = Path(part)
            if path.is_absolute() and path.is_relative_to(root):
                display.append(str(path.relative_to(root)))
                continue
        except ValueError:
            pass
        display.append(part)
    return shlex.join(display)


def score_command(logfile: Path, evidence_json: Path | None = None) -> list[str]:
    root = repo_root()
    command = [
        str(root / "bench/self_correction_score.py"),
        "--require-demo",
        "--trajectories",
    ]
    if evidence_json is not None:
        command.extend(["--demo-evidence-json", str(evidence_json)])
    command.append(str(logfile))
    return command


def repo_path(path: Path) -> Path:
    return path if path.is_absolute() else repo_root() / path


def ensure_fresh_results_path(results: Path) -> None:
    resolved = repo_path(results)
    if resolved.exists() and resolved.stat().st_size > 0:
        raise RuntimeError(
            f"fresh demo results path already contains data: {results}. "
            "Use a unique --results path or remove/truncate the file first."
        )


def load_jsonl(path: Path) -> list[dict[str, object]]:
    resolved = repo_path(path)
    if not resolved.exists():
        raise RuntimeError(f"fresh demo results file was not created: {path}")
    rows: list[dict[str, object]] = []
    with resolved.open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as exc:
                raise RuntimeError(
                    f"invalid JSONL in fresh demo results at line {line_number}: {exc}"
                ) from exc
            if not isinstance(row, dict):
                raise RuntimeError(
                    f"fresh demo results line {line_number} is not a JSON object"
                )
            rows.append(row)
    return rows


def run_id_matches(row_run_id: object, expected: str) -> bool:
    if not isinstance(row_run_id, str):
        return False
    if row_run_id == expected:
        return True
    prefix = f"{expected}-"
    suffix = row_run_id.removeprefix(prefix)
    return row_run_id.startswith(prefix) and suffix.isdecimal()


def validate_fresh_results(args: argparse.Namespace) -> None:
    rows = load_jsonl(args.results)
    if not rows:
        raise RuntimeError(f"fresh demo results file has no rows: {args.results}")

    if args.run_id is not None:
        mismatched = [
            row.get("run_id") for row in rows if not run_id_matches(row.get("run_id"), args.run_id)
        ]
        if mismatched:
            raise RuntimeError(
                "fresh demo results contain rows outside the requested run_id "
                f"{args.run_id!r}: {mismatched[:3]}"
            )

    for index, row in enumerate(rows, start=1):
        missing = [
            key
            for key in (
                "source_head",
                "source_head_short",
                "source_branch",
                "source_dirty",
                "max_tokens",
                "timeout_secs",
            )
            if key not in row
        ]
        if missing:
            raise RuntimeError(
                f"fresh demo row {index} is missing audit field(s): {', '.join(missing)}"
            )
        if not args.allow_dirty_source and row.get("source_dirty") is not False:
            raise RuntimeError(
                f"fresh demo row {index} was produced from dirty source: "
                f"source_dirty={row.get('source_dirty')!r}"
            )
        if row.get("max_tokens") != args.max_tokens:
            raise RuntimeError(
                f"fresh demo row {index} records max_tokens={row.get('max_tokens')!r}; "
                f"expected {args.max_tokens}"
            )
        if row.get("timeout_secs") != args.timeout:
            raise RuntimeError(
                f"fresh demo row {index} records timeout_secs={row.get('timeout_secs')!r}; "
                f"expected {args.timeout}"
            )


def fresh_validation_summary(args: argparse.Namespace) -> str:
    dirty_requirement = "source_dirty=false" if not args.allow_dirty_source else "source_dirty may be true"
    return (
        "# would validate fresh results before scoring: "
        "JSONL exists and is non-empty; "
        f"all rows match run_id {args.run_id!r} or numeric suffixed variants; "
        f"required provenance fields are present; {dirty_requirement}; "
        f"max_tokens={args.max_tokens}; timeout_secs={args.timeout}"
    )


def fresh_command(args: argparse.Namespace) -> list[str]:
    root = repo_root()
    command = [
        str(root / "bench/self_correction.py"),
        "--fixture",
        args.fixture,
        "--provider",
        args.provider,
        "--runs",
        str(args.runs),
        "--attempts",
        str(args.attempts),
        "--max-tokens",
        str(args.max_tokens),
        "--timeout",
        str(args.timeout),
        "--results",
        str(args.results),
    ]
    if args.run_id:
        command.extend(["--run-id", args.run_id])
    if not args.allow_dirty_source:
        command.append("--require-clean-source")
    if args.keep_workspace:
        command.append("--keep-workspace")
    return command


def run_command(command: list[str], *, print_only: bool) -> int:
    print(f"$ {display_command(command)}")
    if print_only:
        return 0
    return subprocess.run(command, cwd=repo_root(), check=False).returncode


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run wrapper unit tests instead of invoking demo commands.",
    )
    subparsers = parser.add_subparsers(dest="mode")

    verify = subparsers.add_parser(
        "verify-archive",
        help="Score a durable archived JSONL demo artifact with --require-demo.",
    )
    verify.add_argument("--archive", type=Path, default=DEFAULT_ARCHIVE)
    verify.add_argument(
        "--evidence-json",
        type=Path,
        help=(
            "Path for a machine-readable demo causal-chain evidence map. "
            "The default archive writes the checked-in evidence map when omitted."
        ),
    )
    verify.add_argument("--print-only", action="store_true")

    fresh = subparsers.add_parser(
        "fresh",
        help="Regenerate a fresh demo JSONL artifact, then score it with --require-demo.",
    )
    fresh.add_argument("--results", type=Path, required=True)
    fresh.add_argument("--fixture", default=DEFAULT_FIXTURE)
    fresh.add_argument("--provider", default=DEFAULT_PROVIDER)
    fresh.add_argument("--runs", type=int, default=3)
    fresh.add_argument("--attempts", type=int, default=3)
    fresh.add_argument("--max-tokens", type=int, default=100_000)
    fresh.add_argument("--timeout", type=int, default=1800)
    fresh.add_argument(
        "--run-id",
        required=True,
        help="Required stable prefix for rows produced by this fresh demo invocation.",
    )
    fresh.add_argument(
        "--allow-dirty-source",
        action="store_true",
        help="Omit --require-clean-source when regenerating the benchmark artifact.",
    )
    fresh.add_argument("--keep-workspace", action="store_true")
    fresh.add_argument(
        "--evidence-json",
        type=Path,
        help="Optional path for a machine-readable demo causal-chain evidence map.",
    )
    fresh.add_argument("--print-only", action="store_true")

    defaultable_argv = list(argv)
    if not defaultable_argv or (
        defaultable_argv[0].startswith("-") and defaultable_argv[0] != "--self-test"
    ):
        defaultable_argv.insert(0, "verify-archive")

    args = parser.parse_args(defaultable_argv)
    return args


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.self_test:
        sys.argv = [sys.argv[0]]
        return unittest.main(exit=False).result.wasSuccessful() is False

    if args.mode == "verify-archive":
        evidence_json = args.evidence_json
        if evidence_json is None and args.archive == DEFAULT_ARCHIVE:
            evidence_json = DEFAULT_ARCHIVE_EVIDENCE
        return run_command(
            score_command(args.archive, evidence_json), print_only=args.print_only
        )

    if args.mode == "fresh":
        if not args.print_only:
            try:
                ensure_fresh_results_path(args.results)
            except RuntimeError as exc:
                print(f"error: {exc}", file=sys.stderr)
                return 2
        first = run_command(fresh_command(args), print_only=args.print_only)
        if first != 0:
            return first
        if args.print_only:
            print(fresh_validation_summary(args))
        else:
            try:
                validate_fresh_results(args)
            except RuntimeError as exc:
                print(f"error: {exc}", file=sys.stderr)
                return 2
        return run_command(
            score_command(args.results, args.evidence_json), print_only=args.print_only
        )

    raise AssertionError(f"unhandled mode: {args.mode}")


class SelfCorrectionDemoTests(unittest.TestCase):
    def test_default_verify_archive_command_scores_known_artifact(self) -> None:
        command = score_command(DEFAULT_ARCHIVE)

        self.assertIn("--require-demo", command)
        self.assertIn("--trajectories", command)
        self.assertEqual(Path(command[-1]), DEFAULT_ARCHIVE)

    def test_score_command_can_write_demo_evidence_json(self) -> None:
        command = score_command(DEFAULT_ARCHIVE, Path("evidence.json"))

        self.assertIn("--demo-evidence-json", command)
        self.assertLess(command.index("--demo-evidence-json"), command.index(str(DEFAULT_ARCHIVE)))
        self.assertEqual(command[command.index("--demo-evidence-json") + 1], "evidence.json")

    def test_no_args_defaults_to_verify_archive_mode(self) -> None:
        args = parse_args([])

        self.assertEqual(args.mode, "verify-archive")
        self.assertEqual(args.archive, DEFAULT_ARCHIVE)
        self.assertIsNone(args.evidence_json)

    def test_archive_flags_work_without_explicit_subcommand(self) -> None:
        args = parse_args(["--archive", "custom.jsonl", "--print-only"])

        self.assertEqual(args.mode, "verify-archive")
        self.assertEqual(args.archive, Path("custom.jsonl"))
        self.assertIsNone(args.evidence_json)
        self.assertTrue(args.print_only)

    def test_default_verify_archive_print_only_includes_checked_in_evidence_json(self) -> None:
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            result = main(["verify-archive", "--print-only"])

        output = stdout.getvalue()
        self.assertEqual(result, 0)
        self.assertIn("--demo-evidence-json", output)
        self.assertIn(str(DEFAULT_ARCHIVE_EVIDENCE), output)
        self.assertIn(str(DEFAULT_ARCHIVE), output)

    def test_verify_archive_print_only_includes_demo_evidence_json(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            stdout = io.StringIO()
            evidence = Path(tmpdir) / "evidence.json"
            with contextlib.redirect_stdout(stdout):
                result = main(
                    [
                        "verify-archive",
                        "--evidence-json",
                        str(evidence),
                        "--print-only",
                    ]
                )

        output = stdout.getvalue()
        self.assertEqual(result, 0)
        self.assertIn("--demo-evidence-json", output)
        self.assertIn(str(evidence), output)
        self.assertIn("bench/self_correction_score.py", output)

    def test_fresh_command_requires_clean_source_by_default(self) -> None:
        args = argparse.Namespace(
            fixture=DEFAULT_FIXTURE,
            provider=DEFAULT_PROVIDER,
            runs=3,
            attempts=3,
            max_tokens=100_000,
            timeout=1800,
            results=Path("docs/benchmark-results/self-correction/fresh.jsonl"),
            run_id="fresh-demo",
            allow_dirty_source=False,
            keep_workspace=False,
            evidence_json=None,
        )

        command = fresh_command(args)

        self.assertIn("--require-clean-source", command)
        self.assertIn("--runs", command)
        self.assertIn("3", command)
        self.assertIn("--max-tokens", command)
        self.assertIn("100000", command)
        self.assertIn("--timeout", command)
        self.assertIn("1800", command)
        self.assertIn("--run-id", command)
        self.assertIn("fresh-demo", command)

    def test_fresh_command_can_print_dirty_local_smoke(self) -> None:
        args = argparse.Namespace(
            fixture=DEFAULT_FIXTURE,
            provider=DEFAULT_PROVIDER,
            runs=1,
            attempts=2,
            max_tokens=100_000,
            timeout=1800,
            results=Path("/tmp/local-smoke.jsonl"),
            run_id=None,
            allow_dirty_source=True,
            keep_workspace=True,
            evidence_json=None,
        )

        command = fresh_command(args)

        self.assertNotIn("--require-clean-source", command)
        self.assertIn("--keep-workspace", command)

    def test_fresh_print_only_shows_internal_validation_before_scoring(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            stdout = io.StringIO()
            results = Path(tmpdir) / "fresh-print-only.jsonl"
            with contextlib.redirect_stdout(stdout):
                result = main(
                    [
                        "fresh",
                        "--results",
                        str(results),
                        "--run-id",
                        "fresh-demo",
                        "--print-only",
                    ]
                )

        output = stdout.getvalue()
        self.assertEqual(result, 0)
        self.assertIn("# would validate fresh results before scoring", output)
        self.assertIn("all rows match run_id 'fresh-demo'", output)
        self.assertIn("source_dirty=false", output)
        self.assertLess(
            output.index("# would validate fresh results before scoring"),
            output.index("bench/self_correction_score.py"),
        )

    def test_fresh_results_refuses_non_empty_file_by_default(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "existing.jsonl"
            results.write_text('{"old": true}\n', encoding="utf-8")

            with self.assertRaises(RuntimeError):
                ensure_fresh_results_path(results)

    def test_fresh_results_allows_empty_precreated_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            empty = Path(tmpdir) / "empty.jsonl"
            empty.touch()

            ensure_fresh_results_path(empty)

    def test_fresh_mode_requires_run_id(self) -> None:
        with self.assertRaises(SystemExit):
            parse_args(["fresh", "--results", "fresh.jsonl"])

    def test_validate_fresh_results_requires_current_run_and_budget_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            rows = [
                {
                    "run_id": "fresh-demo-1",
                    "source_head": "abcdef123456",
                    "source_head_short": "abcdef1",
                    "source_branch": "main",
                    "source_dirty": False,
                    "max_tokens": 100_000,
                    "timeout_secs": 1800,
                },
                {
                    "run_id": "fresh-demo-2",
                    "source_head": "abcdef123456",
                    "source_head_short": "abcdef1",
                    "source_branch": "main",
                    "source_dirty": False,
                    "max_tokens": 100_000,
                    "timeout_secs": 1800,
                },
            ]
            results.write_text(
                "".join(json.dumps(row) + "\n" for row in rows),
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            validate_fresh_results(args)

    def test_validate_fresh_results_rejects_stale_or_mismatched_rows(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps(
                    {
                        "run_id": "old-demo-1",
                        "source_head": "abcdef123456",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": False,
                        "max_tokens": 100_000,
                        "timeout_secs": 1800,
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_same_prefix_non_numeric_suffix(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps(
                    {
                        "run_id": "fresh-demo-old",
                        "source_head": "abcdef123456",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": False,
                        "max_tokens": 100_000,
                        "timeout_secs": 1800,
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_empty_output(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.touch()
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_invalid_jsonl(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text("not json\n", encoding="utf-8")
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_missing_audit_fields(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps({"run_id": "fresh-demo-1", "source_head": "abcdef"}) + "\n",
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_dirty_source(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps(
                    {
                        "run_id": "fresh-demo-1",
                        "source_head": "abcdef123456",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": True,
                        "max_tokens": 100_000,
                        "timeout_secs": 1800,
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)

    def test_validate_fresh_results_rejects_budget_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            results = Path(tmpdir) / "fresh.jsonl"
            results.write_text(
                json.dumps(
                    {
                        "run_id": "fresh-demo-1",
                        "source_head": "abcdef123456",
                        "source_head_short": "abcdef1",
                        "source_branch": "main",
                        "source_dirty": False,
                        "max_tokens": 99_999,
                        "timeout_secs": 1800,
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            args = argparse.Namespace(
                results=results,
                run_id="fresh-demo",
                allow_dirty_source=False,
                max_tokens=100_000,
                timeout=1800,
            )

            with self.assertRaises(RuntimeError):
                validate_fresh_results(args)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
