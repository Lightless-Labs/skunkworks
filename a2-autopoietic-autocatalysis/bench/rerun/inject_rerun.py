"""Inject a pre-registered re-run of an infra-invalidated experiment into the evolve loop state.

Pre-registration: docs/benchmark-results/evolve/noise-floor.md, addendum 2026-10-21. The re-run applies
the archived diff to the logged parent genome and starts at stage A with fresh run ids. No researcher
call is made. The unchanged loop.py then runs it (with --max-experiments raised by one).

Run only while the loop is stopped: the loop keeps its state in memory and would overwrite this edit.

    python3 bench/rerun/inject_rerun.py --exp exp-0012 [--dry-run]
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "bench" / "e1"))
sys.path.insert(0, str(REPO_ROOT))

from bench.e1 import tasks  # noqa: E402
from bench.evolve import loop, researcher  # noqa: E402


def build_current(state: dict, exp: str, patch: str, run_root: Path, parent_genome: Path) -> dict:
    original = next((row for row in state.get("log_rows", []) if row.get("exp_id") == exp), None)
    if original is None or original.get("decision") != "invalid":
        raise SystemExit("{} is not logged as invalid".format(exp))
    exp_id = exp + "r"
    if any(row.get("exp_id") == exp_id for row in state.get("log_rows", [])):
        raise SystemExit("{} has already been run (one shot)".format(exp_id))
    candidate = run_root / exp_id / "candidate"
    applied = researcher.apply_candidate(parent_genome, patch, candidate)
    if not applied["ok"]:
        raise SystemExit("archived patch does not apply: " + applied["reason"])
    if applied["hypothesis"] != original.get("hypothesis"):
        raise SystemExit("hypothesis mismatch between the archive and the log row")
    private = tasks.private_dir()
    screen = loop._read_json_list(private / "evolve-screen.json")
    full_tasks = loop._read_json_list(private / "evolve-dev-tasks.json")
    return {"exp_id": exp_id, "stage": "stage_a", "candidate": str(candidate),
            "parent": original.get("parent", "G0"), "tokens": 0, "started": time.time(),
            "hypothesis": applied["hypothesis"], "candidate_diff": patch,
            "rerun_of": exp, "screen_tasks": screen, "full_tasks": full_tasks,
            "workspace": str(tasks.e1_home() / "evolve-workspaces" / exp_id / "workspace"),
            "researcher_run_dir": str(run_root / exp_id / "researcher")}


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--exp", required=True)
    parser.add_argument("--dry-run", action="store_true", help="build the candidate, print, do not write state")
    args = parser.parse_args(argv)
    private = tasks.private_dir()
    state_path = private / "evolve-loop" / "state.json"
    state = json.loads(state_path.read_text())
    if state.get("current") is not None and not args.dry_run:
        raise SystemExit("the loop has an experiment in progress: {}".format(state["current"].get("exp_id")))
    patch = (REPO_ROOT / "docs" / "benchmark-results" / "evolve" / "archive" / (args.exp + ".patch")).read_text()
    # The logged parent is G0. Its genome is the committed G0 tree while no variant has been kept.
    if state.get("champion_exp_id", "G0") != "G0":
        raise SystemExit("champion changed: rebuild G0 from git before re-running (see the addendum)")
    run_root = private / "evolve-loop" / ("rerun-dry" if args.dry_run else "runs")
    current = build_current(state, args.exp, patch, run_root, Path(state["champion_genome"]))
    print(json.dumps({k: v for k, v in current.items() if k not in ("candidate_diff", "screen_tasks", "full_tasks")},
                     indent=1))
    print("screen {} tasks, full {} tasks".format(len(current["screen_tasks"]), len(current["full_tasks"])))
    if not args.dry_run:
        state["current"] = current
        loop._write_state(state_path, state)
        print("injected; resume with loop.py and --max-experiments {}".format(int(state["completed"]) + 1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
