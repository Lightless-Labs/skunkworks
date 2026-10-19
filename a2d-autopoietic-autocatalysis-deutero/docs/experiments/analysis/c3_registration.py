"""Assemble confirmation-3's registration.json mechanically (plan 2026-10-18-confirmation-3 §0,
§0.13). Only the candidate-specific wording is given by hand; everything else is derived:
the frozen file and its sha and lanes, the certified suite's controller hash and task ids,
the codex fingerprint, the instrument commit, and the fixed rules copied from the plan.

usage: python3 -I docs/experiments/analysis/c3_registration.py --candidate <harness.py>
           --frozen-from "<text>" --gate "<text>" --claim-wording "<text>" [--disclosure "<text>"]...
           [--suite <dir>] [--dry-run]
--dry-run prints the registration with the task ids replaced by their count and writes nothing.
"""
import argparse
import json
import pathlib
import re
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "nano"))
import confirm3 as c3  # noqa: E402

SEEDS = {"1": 9191, "2": 9292}          # new: not confirmation-2's 8181/8282 nor the looks' 6161/6262
TASK = re.compile(r"^c3-f\d{2}-l\d$")
FIXED_DISCLOSURES = [
    "Authorship: the tasks were authored blind by Sol (gpt-6.1-sol, high) and audited by an independent Sol solve "
    "(medium); a Sol judge lane in the candidate is an author-judge alignment. The claim stays narrow, to this "
    "authoring distribution.",
    "Mid-authoring pipeline fixes (plan §0.13): the command scan's prose false positive (ecfb1c65) and the retry "
    "codex-home reset (17da7217). Jobs before 08:00 UTC 10-19 ran under the earlier pipeline commit.",
    "Injected context: codex 0.162 injects its 4 bundled skills (~7.3k chars) and multi-agent role text (~2.8k) "
    "that no switch removes; identical in every arm, no task information.",
    "Instrument versus dev: the dev and dev-holdout estimates were measured with web search open (unused), plugins "
    "and user skills loaded; confirmation-3 runs with them off (C2-WEB, C2-MCP, plan §0.13). Symmetric across arms.",
    "Integrity policy: tuple-level voids with worst-case imputation, weaker than confirmation-2's any-event "
    "amendment but conservative for the claim; IntegrityReview above 0.5% tool events per replicate.",
    "The CI is the controller's whole-family bootstrap on complete tuples; imputation binds the point threshold "
    "(SUPPORTED: worst case; RULED OUT: best case).",
    "Codex version: --json transcripts carry no banner, so the binary is pinned by a sha256 fingerprint bracketing "
    "each replicate; a change voids that replicate's tuples.",
    "Spent sets: confirmation-2 (IntegrityReview, web search in arm B); its scored data stays sealed until this "
    "decision and is then reported descriptively.",
    "Possible model drift between the dev runs (10-15 to 10-23) and the spend days.",
]


def lanes_of(path):
    lanes = {}
    for m in re.finditer(r"^# lanes:\s*(.+)$", pathlib.Path(path).read_text(), re.M):
        for spec in m.group(1).split():
            name, value = spec.split("=", 1)
            lanes[name] = value
    return lanes


def build(args, suite):
    tasks = sorted(p.name for p in suite.iterdir() if TASK.match(p.name)) if suite.exists() else []
    suite_hash = subprocess.run([c3.A2D, "lift", "hash", str(suite)], capture_output=True, text=True).stdout.strip()
    fp = c3.codex_fingerprint()
    controller = c3.git("log", "-1", "--format=%h %cI", "--", "crates", "Cargo.toml", "Cargo.lock")[1].strip()
    return {
        "experiment": "C2-CONFIRM3",
        "protocol": "plan docs/plans/2026-10-18-confirmation-3.md §0 (copied, not reopened; §0.10 allows the review only "
                    "to tighten §0.4) and §0.13; runner nano/confirm3.py generate / manifest / score, bound with this "
                    "file and frozen.py by the registration commit",
        "frozen": "runs/c2-confirm3/frozen.py",
        "frozen_sha": c3.sha256(args.candidate),
        "frozen_from": args.frozen_from,
        "gate": args.gate,
        "suite": "~/.a2d-holdout/confirmation-3",
        "suite_hash": suite_hash,
        "tasks": tasks,
        "families": len({t.rsplit("-l", 1)[0] for t in tasks}),
        "arms": ["D", "B", "X:frozen"],
        "k": 5,
        "model": "codex/gpt-6-luna",
        "effort": "low",
        "lanes": lanes_of(args.candidate),
        "replicates": 2,
        "seeds": SEEDS,
        "claim": "X:frozen>D,X:frozen>B",
        "claim_wording": args.claim_wording,
        "decision_rule": {
            "authority": "nano/confirm3.py decide() over runs/c2-confirm3/pooled-summary.json (a2d lift pool of both "
                         "replicate ledgers) and the per-(task, arm, replicate) voids",
            "SUPPORTED": "X vs D and X vs B both Supported by the controller rule (ci_low > 0, lift >= +10 pp, >= 10 "
                         "families, discordance) AND worst-case imputed all-pass lift >= +10 pp vs both over every "
                         "planned (task, replicate)",
            "RULED OUT": "X vs D RuledOut (ci_high < +10 pp) AND best-case imputed lift vs D < +10 pp",
            "IntegrityReview": "any hidden-test name hit, or tool events > 0.5% of a replicate's invocations",
            "Invalid": "void tuples > 5% of all (task, arm, replicate) tuples, the controller's excluded-share guard, "
                       "an aborted generation, or a missing comparison",
            "void tuple": "void or unscored submission, any tool event, any invocation without a transcript or "
                          "(legacy text transcripts) from another codex version; all tuples of a replicate whose codex "
                          "fingerprint changed",
            "otherwise": "INCONCLUSIVE",
        },
        "codex": fp,
        "codex_version": fp["version"] if fp else None,
        "codex_files": fp["files"] if fp else None,
        "spend": {"earliest_utc_day": str(c3.EARLIEST), "after_registration_day": True,
                  "requires": "confirmation-2 sealed (runs/c2-confirm2/sealed-sha256.txt), the authoring pipeline "
                              "drained, codex = the registered fingerprint",
                  "latency_go": f"median of {c3.LATENCY_PROBES} PONG probes <= {c3.LATENCY_MAX_MEDIAN_SECS:g} s",
                  "daily_cap": c3.DAILY_CAP, "replicate_2": "a later UTC day than replicate 1's generation"},
        "failure_policy": "each replicate is attempted once (attempt record before any model call); a deferral is not "
                          "an attempt; a generation that fails before generation_complete may be relaunched once with "
                          "the same seed in its own dir, logged; a second failure ends confirmation-3 as not run",
        "instrument": {"controller_last_change": controller,
                       "provider": "c2-web-provider merged: no web search, isolated CODEX_HOME per call, tool features "
                                   "disabled, --json allowlist tool counting, failed-call transcripts (plan §0.13)",
                       "binding": "the controller refuses to run or score if crates/, Cargo.toml or Cargo.lock differ "
                                  "from the registration commit"},
        "power": "docs/experiments/analysis/c3_power.py (dev summaries only): P(RULED OUT | 0) = 0.91; P(SUPPORTED) = "
                 "0.14 / 0.33 / 0.47 / 0.96 at +7.1 / +8.9 / +10 / +15 pp",
        "expected_spend_per_replicate": "about 1.6k Luna and about 125 Sol (confirmation-2: 1,618 and 1,617 invocations)",
        "disclosures": FIXED_DISCLOSURES + list(args.disclosure),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--candidate", required=True, type=pathlib.Path)
    ap.add_argument("--frozen-from", required=True)
    ap.add_argument("--gate", required=True)
    ap.add_argument("--claim-wording", required=True)
    ap.add_argument("--disclosure", action="append", default=[])
    ap.add_argument("--suite", type=pathlib.Path, default=c3.SUITE)
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    reg = build(args, args.suite)
    if args.dry_run:
        print(json.dumps({**reg, "tasks": len(reg["tasks"])}, indent=1))
        return
    out = c3.DIR / "registration.json"
    if out.exists():
        sys.exit(f"{out} exists; the registration is written once")
    if not reg["tasks"] or not reg["suite_hash"] or not reg["codex"]:
        sys.exit("suite, suite hash or codex fingerprint missing")
    c3.DIR.mkdir(parents=True, exist_ok=True)
    shutil.copy(args.candidate, c3.DIR / "frozen.py")
    out.write_text(json.dumps(reg, indent=1) + "\n")
    print(json.dumps({"written": str(out), "tasks": len(reg["tasks"]), "families": reg["families"],
                      "frozen_sha": reg["frozen_sha"][:12], "suite_hash": reg["suite_hash"][:12]}))


if __name__ == "__main__":
    main()
