"""Dress rehearsal of nano/confirm3.py on 2 dev tasks (fresh-Claude registration review, B1(ii)).

Runs the runner's own cmd_generate (both replicates), cmd_manifest and cmd_score with the real
`a2d` binary, the real Popen and the exact registered command line (flags, external frozen arm,
judge lane, claim, seeds), so that a rejected flag, an arm-label or invocation-count mismatch, or
a malformed record shows up here instead of on the spend-once set.

Dev data only. Scratch holdout: ~/.a2d-rehearsal (never ~/.a2d-holdout). Repo dir:
runs/c2-confirm3-rehearsal. Patched, and nothing else:
- the registration lookup: a fake registration of the 2 tasks (written by `setup`), registered at
  the HEAD recorded by `setup` (the controller still checks crates/ against it);
- the date gates: registered yesterday, earliest day today; replicate 2 runs on tomorrow's date;
- the drain check: the scratch holdout has no authoring pipeline.

usage: python3 -I docs/experiments/analysis/c3_rehearsal.py setup|generate1|generate2|manifest|score|check
       (commit and push runs/c2-confirm3-rehearsal/manifest-r*.json between manifest and score)
"""
import argparse
import datetime
import json
import os
import pathlib
import shutil
import sys

TAG = os.environ.get("C3_REHEARSAL_TAG", "")             # a later rehearsal gets fresh dirs
SCRATCH = pathlib.Path.home() / f".a2d-rehearsal{TAG}"
os.environ["A2D_HOLDOUT_DIR"] = str(SCRATCH)          # c3lib reads it at import
ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path[:0] = [str(ROOT / "nano"), str(ROOT / "tools" / "holdout")]
import confirm3 as c3  # noqa: E402

TASKS = ["csv-quote-l1", "varint-codec-l1"]
DIR = ROOT / "runs" / f"c2-confirm3-rehearsal{TAG}"
c3.HOLDOUT, c3.SUITE, c3.DIR = SCRATCH, SCRATCH / "confirmation-3", DIR
c3.CODEX_HOMES, c3.CANARY = SCRATCH / "codex-homes", SCRATCH / "canary"
REAL_NOW, REAL_TODAY = c3.now, c3.today


def registered_at():
    return (DIR / "registered_at.txt").read_text().strip()


def patch(shift_days=0):
    day = REAL_TODAY() + datetime.timedelta(days=shift_days)
    c3.registration_commit = registered_at
    c3.registration = lambda sha: json.loads((DIR / "registration.json").read_text())
    c3.commit_day = lambda sha: REAL_TODAY() - datetime.timedelta(days=1)
    c3.EARLIEST = REAL_TODAY()
    c3.drained = lambda: True
    c3.now = lambda: REAL_NOW() + datetime.timedelta(days=shift_days)
    c3.today = lambda: day


def setup():
    if DIR.exists() or SCRATCH.exists():
        sys.exit("rehearsal dirs exist; remove them for a fresh rehearsal")
    for t in TASKS:
        shutil.copytree(ROOT / "tasks" / "train-all" / t, c3.SUITE / t)
    DIR.mkdir(parents=True)
    shutil.copy(ROOT / "runs" / "c2-confirm2" / "frozen.py", DIR / "frozen.py")
    fp = c3.codex_fingerprint()
    reg = {"tasks": TASKS, "arms": ["D", "B", "X:frozen"], "k": 5, "model": "codex/gpt-6-luna", "effort": "low",
           "lanes": {"judge": "codex/gpt-6.1-sol:medium"}, "seeds": {"1": 9191, "2": 9292},
           "claim": "X:frozen>D,X:frozen>B", "suite_hash": c3.suite_hash_now(), "codex_version": fp["version"],
           "codex_files": fp["files"], "frozen_sha": c3.sha256(DIR / "frozen.py")}
    if not reg["suite_hash"]:
        sys.exit("a2d lift hash failed on the rehearsal suite")
    (DIR / "registration.json").write_text(json.dumps(reg, indent=1) + "\n")
    (DIR / "registered_at.txt").write_text(c3.git("rev-parse", "HEAD")[1].strip() + "\n")
    (DIR / "runner_sha256.txt").write_text(c3.sha256(ROOT / "nano" / "confirm3.py") + "\n")
    print(json.dumps({"setup": True, "tasks": len(TASKS), "registered_at": registered_at()[:10],
                      "runner_sha256": c3.sha256(ROOT / "nano" / "confirm3.py")[:12]}))


def check():
    """Counts only: the runner's tuples against the controller's own per-replicate summaries."""
    reg = json.loads((DIR / "registration.json").read_text())
    for rep in (1, 2):
        out, _ = c3.generated_out(rep)
        tuples, stats = c3.replicate_tuples(out, rep, reg["arms"], reg["tasks"])
        summary = json.loads((out / "summary.json").read_text()) if (out / "summary.json").exists() else {}
        print(json.dumps({"rep": rep, "tuples": len(tuples), "runner_voids": sum(v["void"] for v in tuples.values()),
                          "unscored": sum(v["pass"] is None for v in tuples.values()),
                          "controller_tuples_void": summary.get("tuples_void"), "stats": stats,
                          "lanes": c3.lane_usage(out)}))


def main():
    step = sys.argv[1] if len(sys.argv) > 1 else ""
    pinned = DIR / "runner_sha256.txt"
    if step != "setup" and pinned.exists() and pinned.read_text().strip() != c3.sha256(ROOT / "nano" / "confirm3.py"):
        sys.exit("nano/confirm3.py changed since this rehearsal's setup")
    ns = argparse.Namespace
    if step == "setup":
        setup()
    elif step in ("generate1", "generate2"):
        patch(shift_days=0 if step == "generate1" else 1)
        c3.cmd_generate(ns(rep=int(step[-1]), relaunch=False, registered_at=registered_at(), jobs=8))
    elif step == "manifest":
        patch(1)
        c3.cmd_manifest(ns(registered_at=registered_at()))
    elif step == "score":
        patch(1)
        c3.cmd_score(ns(registered_at=registered_at()))
    elif step == "check":
        patch(1)
        check()
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
