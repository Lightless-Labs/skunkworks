#!/usr/bin/env python3
"""Run A²'s auditable self-correction demo gates.

This wrapper has two intentionally separate modes:

* ``verify-archive`` re-scores a durable archived JSONL artifact and proves that it
  contains a failed-attempt -> retry-context -> verified-promotion trajectory.
* ``fresh`` runs the self-correction harness to regenerate a new JSONL artifact,
  then immediately applies the same ``--require-demo`` scorer gate to that output.

The default mode is archive verification because a fresh provider run can be slow
and may consume paid quota. Use ``fresh --preflight-only`` for local no-network
checks before running it, or ``fresh --print-only`` to inspect command wiring only.
"""

from __future__ import annotations

import argparse
import contextlib
import io
import json
import shutil
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


def default_fresh_evidence_path(results: Path) -> Path:
    if results.suffix == ".jsonl":
        return results.with_suffix(".demo-evidence.json")
    return Path(f"{results}.demo-evidence.json")


def repo_path(path: Path) -> Path:
    return path if path.is_absolute() else repo_root() / path


def ensure_output_path_empty(path: Path, *, label: str) -> None:
    resolved = repo_path(path)
    if resolved.exists() and resolved.stat().st_size > 0:
        raise RuntimeError(
            f"fresh demo {label} path already contains data: {path}. "
            "Use a unique path or remove/truncate the file first."
        )


def ensure_fresh_results_path(results: Path) -> None:
    ensure_output_path_empty(results, label="results")


def ensure_fresh_evidence_path(evidence_json: Path) -> None:
    ensure_output_path_empty(evidence_json, label="evidence")


def provider_binary_name(provider: str) -> str:
    family = provider.split("/", 1)[0]
    return {
        "opencode": "opencode",
        "pi": "pi",
        "claude": "claude",
        "codex": "codex",
        "gemini": "gemini",
    }.get(family, family)


def ensure_provider_binary(provider: str) -> None:
    binary = provider_binary_name(provider)
    if shutil.which(binary) is None:
        raise RuntimeError(
            f"fresh demo provider binary {binary!r} for provider {provider!r} was not found in PATH"
        )


def opencode_auth_path() -> Path:
    return Path.home() / ".local/share/opencode/auth.json"


def ensure_opencode_provider_config(provider: str, *, auth_path: Path | None = None) -> None:
    parts = provider.split("/")
    if len(parts) < 2:
        return
    configured_provider = parts[1]
    auth_path = auth_path or opencode_auth_path()
    if not auth_path.exists():
        raise RuntimeError(
            f"fresh demo opencode credentials file was not found: {auth_path}"
        )
    try:
        auth = json.loads(auth_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise RuntimeError(
            f"fresh demo opencode credentials file is invalid JSON: {auth_path}: {exc}"
        ) from exc
    if not isinstance(auth, dict) or configured_provider not in auth:
        raise RuntimeError(
            "fresh demo opencode credentials do not include provider "
            f"{configured_provider!r} in {auth_path}"
        )


def ensure_provider_config(provider: str) -> None:
    family = provider.split("/", 1)[0]
    if family == "opencode":
        ensure_opencode_provider_config(provider)


def ensure_clean_source() -> None:
    status = subprocess.run(
        ["git", "status", "--porcelain", "--", "."],
        cwd=repo_root(),
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if status.returncode != 0:
        raise RuntimeError(f"could not inspect source cleanliness: {status.stderr.strip()}")
    if status.stdout.strip():
        raise RuntimeError(
            "fresh demo source tree is dirty; commit/stash changes or pass --allow-dirty-source"
        )


def fresh_preflight(args: argparse.Namespace, evidence_json: Path) -> None:
    ensure_fresh_results_path(args.results)
    ensure_fresh_evidence_path(evidence_json)
    ensure_provider_binary(args.provider)
    ensure_provider_config(args.provider)
    if not args.allow_dirty_source:
        ensure_clean_source()


def provider_config_checked(provider: str) -> bool:
    parts = provider.split("/")
    return len(parts) >= 2 and parts[0] == "opencode"


def paths_alias(left: Path, right: Path) -> bool:
    return repo_path(left).resolve(strict=False) == repo_path(right).resolve(strict=False)


def ensure_preflight_report_path(report: Path, *, results: Path, evidence_json: Path) -> None:
    if paths_alias(report, results):
        raise RuntimeError(
            "fresh demo preflight report path must be distinct from results path: "
            f"{report}"
        )
    if paths_alias(report, evidence_json):
        raise RuntimeError(
            "fresh demo preflight report path must be distinct from evidence path: "
            f"{report}"
        )
    ensure_output_path_empty(report, label="preflight report")


def fresh_preflight_report(args: argparse.Namespace, evidence_json: Path) -> dict[str, object]:
    config_checked = provider_config_checked(args.provider)
    return {
        "mode": "fresh_preflight",
        "creates_loop_evidence": False,
        "live_provider_auth_quota_model_checked": False,
        "results": str(args.results),
        "evidence_json": str(evidence_json),
        "preflight_report_json": str(args.preflight_report_json),
        "fixture": args.fixture,
        "provider": args.provider,
        "run_id": args.run_id,
        "runs": args.runs,
        "attempts": args.attempts,
        "max_tokens": args.max_tokens,
        "timeout_secs": args.timeout,
        "checks": {
            "results_path_empty": True,
            "evidence_path_empty": True,
            "preflight_report_path_empty": True,
            "preflight_report_path_distinct_from_results": True,
            "preflight_report_path_distinct_from_evidence": True,
            "provider_binary": provider_binary_name(args.provider),
            "provider_binary_present": True,
            "local_provider_config_checked": config_checked,
            "local_provider_config_present_when_supported": True if config_checked else None,
            "source_clean_required": not args.allow_dirty_source,
            "source_clean": None if args.allow_dirty_source else True,
            "dirty_source_allowed": args.allow_dirty_source,
        },
        "commands": {
            "harness": display_command(fresh_command(args)),
            "validation": fresh_validation_summary(args),
            "scorer": display_command(score_command(args.results, evidence_json)),
        },
        "notes": [
            "No provider-backed benchmark was executed by this preflight.",
            "Live provider auth, quota, and model availability are not verified until the fresh run executes.",
            "This report is readiness evidence only; it is not loop evidence and contains no failed-attempt/retry/promotion proof.",
        ],
    }


def write_fresh_preflight_report(
    path: Path,
    report: dict[str, object],
    *,
    results: Path,
    evidence_json: Path,
) -> None:
    ensure_preflight_report_path(path, results=results, evidence_json=evidence_json)
    resolved = repo_path(path)
    resolved.parent.mkdir(parents=True, exist_ok=True)
    resolved.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")


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


def fresh_preflight_summary(args: argparse.Namespace) -> str:
    source_check = "source is clean" if not args.allow_dirty_source else "dirty source allowed"
    return (
        "# preflight checked local prerequisites: empty results/evidence paths; "
        f"provider binary {provider_binary_name(args.provider)!r} present; "
        f"local provider config present when supported; {source_check}. "
        "Live provider auth, quota, and model availability are not verified until the fresh run executes."
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
    fresh.add_argument(
        "--preflight-only",
        action="store_true",
        help=(
            "Check local fresh-run prerequisites (empty output paths, provider binary, "
            "local provider config where supported, and clean source unless "
            "--allow-dirty-source) and print the commands without running the "
            "provider-backed benchmark. This does not validate live auth or quota."
        ),
    )
    fresh.add_argument(
        "--preflight-report-json",
        type=Path,
        help=(
            "With --preflight-only, write a machine-readable no-network readiness "
            "report. The report is not loop evidence and does not validate live "
            "provider auth, quota, or model availability."
        ),
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
        evidence_json = args.evidence_json or default_fresh_evidence_path(args.results)
        if args.preflight_report_json and not args.preflight_only:
            print("error: --preflight-report-json requires --preflight-only", file=sys.stderr)
            return 2
        if args.preflight_only:
            try:
                fresh_preflight(args, evidence_json)
                if args.preflight_report_json:
                    write_fresh_preflight_report(
                        args.preflight_report_json,
                        fresh_preflight_report(args, evidence_json),
                        results=args.results,
                        evidence_json=evidence_json,
                    )
            except RuntimeError as exc:
                print(f"error: {exc}", file=sys.stderr)
                return 2
            print(fresh_preflight_summary(args))
            if args.preflight_report_json:
                print(f"# wrote preflight report: {args.preflight_report_json}")
            run_command(fresh_command(args), print_only=True)
            print(fresh_validation_summary(args))
            run_command(score_command(args.results, evidence_json), print_only=True)
            return 0
        if not args.print_only:
            try:
                fresh_preflight(args, evidence_json)
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
            score_command(args.results, evidence_json), print_only=args.print_only
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

    def test_default_fresh_evidence_path_replaces_jsonl_suffix(self) -> None:
        self.assertEqual(
            default_fresh_evidence_path(Path("docs/results/fresh.jsonl")),
            Path("docs/results/fresh.demo-evidence.json"),
        )
        self.assertEqual(
            default_fresh_evidence_path(Path("docs/results/fresh")),
            Path("docs/results/fresh.demo-evidence.json"),
        )

    def test_provider_binary_name_maps_provider_families(self) -> None:
        self.assertEqual(provider_binary_name("opencode/minimax/MiniMax-M3"), "opencode")
        self.assertEqual(provider_binary_name("pi/zai/glm-5.2"), "pi")
        self.assertEqual(provider_binary_name("gemini"), "gemini")

    def test_opencode_provider_config_requires_configured_provider(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            auth = Path(tmpdir) / "auth.json"
            auth.write_text(
                json.dumps({"minimax-coding-plan": {"type": "api", "key": "redacted"}}),
                encoding="utf-8",
            )

            ensure_opencode_provider_config(
                "opencode/minimax-coding-plan/MiniMax-M3",
                auth_path=auth,
            )
            with self.assertRaises(RuntimeError):
                ensure_opencode_provider_config(
                    "opencode/missing-plan/model",
                    auth_path=auth,
                )

    def test_fresh_evidence_path_refuses_non_empty_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            evidence = Path(tmpdir) / "fresh.demo-evidence.json"
            evidence.write_text('{"old": true}\n', encoding="utf-8")

            with self.assertRaises(RuntimeError):
                ensure_fresh_evidence_path(evidence)

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
        self.assertIn(str(results.with_suffix(".demo-evidence.json")), output)
        self.assertLess(
            output.index("# would validate fresh results before scoring"),
            output.index("bench/self_correction_score.py"),
        )

    def test_fresh_print_only_honors_explicit_evidence_json(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            stdout = io.StringIO()
            results = Path(tmpdir) / "fresh-print-only.jsonl"
            evidence = Path(tmpdir) / "custom-evidence.json"
            with contextlib.redirect_stdout(stdout):
                result = main(
                    [
                        "fresh",
                        "--results",
                        str(results),
                        "--run-id",
                        "fresh-demo",
                        "--evidence-json",
                        str(evidence),
                        "--print-only",
                    ]
                )

        output = stdout.getvalue()
        self.assertEqual(result, 0)
        self.assertIn(str(evidence), output)
        self.assertNotIn(str(results.with_suffix(".demo-evidence.json")), output)

    def test_fresh_preflight_checks_local_prerequisites_and_prints_commands(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                with contextlib.redirect_stdout(stdout):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                        ]
                    )
        finally:
            shutil.which = original_which

        output = stdout.getvalue()
        self.assertEqual(result, 0)
        self.assertIn("# preflight checked local prerequisites", output)
        self.assertIn("Live provider auth, quota, and model availability are not verified", output)
        self.assertIn("bench/self_correction.py", output)
        self.assertIn("# would validate fresh results before scoring", output)
        self.assertIn(str(results.with_suffix(".demo-evidence.json")), output)

    def test_fresh_preflight_writes_machine_readable_readiness_report(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                report = Path(tmpdir) / "fresh-preflight.report.json"
                with contextlib.redirect_stdout(stdout):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                            "--preflight-report-json",
                            str(report),
                        ]
                    )
                data = json.loads(report.read_text(encoding="utf-8"))
        finally:
            shutil.which = original_which

        self.assertEqual(result, 0)
        self.assertIn("# wrote preflight report", stdout.getvalue())
        self.assertEqual(data["mode"], "fresh_preflight")
        self.assertFalse(data["creates_loop_evidence"])
        self.assertFalse(data["live_provider_auth_quota_model_checked"])
        self.assertEqual(data["results"], str(results))
        self.assertEqual(data["evidence_json"], str(results.with_suffix(".demo-evidence.json")))
        self.assertEqual(data["preflight_report_json"], str(report))
        self.assertTrue(data["checks"]["preflight_report_path_empty"])
        self.assertTrue(data["checks"]["preflight_report_path_distinct_from_results"])
        self.assertTrue(data["checks"]["preflight_report_path_distinct_from_evidence"])
        self.assertEqual(data["checks"]["provider_binary"], "local-test-provider")
        self.assertTrue(data["checks"]["provider_binary_present"])
        self.assertFalse(data["checks"]["local_provider_config_checked"])
        self.assertIsNone(data["checks"]["local_provider_config_present_when_supported"])
        self.assertTrue(data["checks"]["dirty_source_allowed"])
        self.assertIn("bench/self_correction.py", data["commands"]["harness"])
        self.assertIn("--demo-evidence-json", data["commands"]["scorer"])
        self.assertIn("not loop evidence", " ".join(data["notes"]))

    def test_fresh_preflight_report_refuses_non_empty_file(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                stderr = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                report = Path(tmpdir) / "fresh-preflight.report.json"
                report.write_text('{"old": true}\n', encoding="utf-8")
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                            "--preflight-report-json",
                            str(report),
                        ]
                    )
        finally:
            shutil.which = original_which

        self.assertEqual(result, 2)
        self.assertIn("fresh demo preflight report path already contains data", stderr.getvalue())
        self.assertEqual(stdout.getvalue(), "")

    def test_fresh_preflight_report_refuses_results_alias(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                stderr = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                            "--preflight-report-json",
                            str(results),
                        ]
                    )
        finally:
            shutil.which = original_which

        self.assertEqual(result, 2)
        self.assertIn("preflight report path must be distinct from results path", stderr.getvalue())
        self.assertEqual(stdout.getvalue(), "")

    def test_fresh_preflight_report_refuses_default_evidence_alias(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                stderr = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                evidence = results.with_suffix(".demo-evidence.json")
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--preflight-only",
                            "--preflight-report-json",
                            str(evidence),
                        ]
                    )
        finally:
            shutil.which = original_which

        self.assertEqual(result, 2)
        self.assertIn("preflight report path must be distinct from evidence path", stderr.getvalue())
        self.assertEqual(stdout.getvalue(), "")

    def test_fresh_preflight_report_refuses_explicit_evidence_alias(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                stderr = io.StringIO()
                results = Path(tmpdir) / "fresh-preflight.jsonl"
                evidence = Path(tmpdir) / "custom-evidence.json"
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--evidence-json",
                            str(evidence),
                            "--preflight-only",
                            "--preflight-report-json",
                            str(evidence),
                        ]
                    )
        finally:
            shutil.which = original_which

        self.assertEqual(result, 2)
        self.assertIn("preflight report path must be distinct from evidence path", stderr.getvalue())
        self.assertEqual(stdout.getvalue(), "")

    def test_bare_opencode_provider_does_not_claim_config_check(self) -> None:
        self.assertFalse(provider_config_checked("opencode"))
        self.assertTrue(provider_config_checked("opencode/minimax-coding-plan/MiniMax-M3"))

    def test_preflight_report_requires_preflight_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            stdout = io.StringIO()
            stderr = io.StringIO()
            results = Path(tmpdir) / "fresh.jsonl"
            report = Path(tmpdir) / "fresh.report.json"
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                result = main(
                    [
                        "fresh",
                        "--results",
                        str(results),
                        "--run-id",
                        "fresh-demo",
                        "--preflight-report-json",
                        str(report),
                        "--print-only",
                    ]
                )

        self.assertEqual(result, 2)
        self.assertIn("--preflight-report-json requires --preflight-only", stderr.getvalue())
        self.assertEqual(stdout.getvalue(), "")

    def test_fresh_mode_refuses_non_empty_evidence_before_harness(self) -> None:
        original_which = shutil.which
        shutil.which = lambda binary: f"/usr/bin/{binary}"
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                stdout = io.StringIO()
                stderr = io.StringIO()
                results = Path(tmpdir) / "fresh.jsonl"
                evidence = Path(tmpdir) / "fresh.demo-evidence.json"
                evidence.write_text('{"old": true}\n', encoding="utf-8")
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    result = main(
                        [
                            "fresh",
                            "--results",
                            str(results),
                            "--run-id",
                            "fresh-demo",
                            "--allow-dirty-source",
                            "--provider",
                            "local-test-provider/model",
                            "--evidence-json",
                            str(evidence),
                        ]
                    )
        finally:
            shutil.which = original_which

        self.assertEqual(result, 2)
        self.assertIn("fresh demo evidence path already contains data", stderr.getvalue())
        self.assertNotIn("bench/self_correction.py", stdout.getvalue())

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
