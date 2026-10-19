"""Assemble confirmation-3's registration.json mechanically (plan 2026-10-18-confirmation-3 §0,
§0.13; early reviews 2026-10-19). Only wording is given by hand. Derived and checked:
- the candidate by §0.1 (no operator choice): the look-2 snapshot iff a committed look-2 gate
  is valid and PASS and the look completed by T, else E002 (runs/c2-confirm2/frozen.py);
- the provider merge (c3lib.PROVIDER_COMMIT on HEAD's history), recorded and re-checked by
  the runner;
- the suite's certification (validation and similarity clean on exactly this suite, every
  task audited, nothing left to adjudicate), complete 4-task families, >= 30 families;
- the frozen file's sha and lanes (refused when empty), the suite's controller hash and task
  ids, the codex fingerprint, the instrument commit, and the fixed rules copied from the plan.

usage: python3 -I docs/experiments/analysis/c3_registration.py --frozen-from "<text>"
           --claim-wording "<text>" --branch-caps "<text>" [--disclosure "<text>"]... [--dry-run]
--dry-run prints the registration with the task ids counted, skips the certification and
provider checks it cannot yet pass, and writes nothing.
"""
import argparse
import datetime
import json
import pathlib
import re
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "nano"))
sys.path.insert(0, str(ROOT / "tools" / "holdout"))
import confirm3 as c3  # noqa: E402

E002 = ROOT / "runs" / "c2-confirm2" / "frozen.py"
E002_SHA = "e772bd6e99f6483b8a01f1e6e3910a540b5a1d3898ba9b1b46c13e172acf2f09"
LOOK2_GATE = ROOT / "runs" / "c2-look2" / "gate.json"
LOOK2_SNAPSHOT = ROOT / "runs" / "c2-look2" / "harness.py"
LOOK2_RUN = c3.HOLDOUT / "runs" / "c2-look2"
T = datetime.datetime(2026, 10, 23, 23, 59, tzinfo=datetime.timezone.utc)
MIN_FAMILIES = 30

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
    "SPEC.md was kept byte-identical through authoring (Sol's suggestion #22 not adopted); the limits disclosed in "
    "§0.11 and the step-5 tooling choices of §0.12 apply (a void audit is no information and is re-audited once; a "
    "repair may change only the requirements and/or the hidden tests and must pass validation and similarity again).",
    "Authoring sandbox: a copied `security` binary can list the world-readable system keychain's metadata (no "
    "secrets); the login keychain file is unreadable (§0.12).",
    "The controller's own excluded-share guard (5% of (task, replicate) pairs, any arm void excludes the pair) is "
    "stricter than §0.13's 5% of (task, arm, replicate) tuples and also applies; c3_power assumed no voids, so the "
    "best-case RULED OUT guard lowers P(RULED OUT | 0) below 0.91.",
    "Latency go/no-go probes are model calls (3 per spend attempt) made before the attempt record; they count "
    "toward the daily cap.",
    "Launch isolation for a replicate covers what applies to a no-tool run: no codex context in any ancestor of "
    "the temp root, an auth-only CODEX_HOME per call (provider), the codex fingerprint. The canary read-deny and "
    "CODEX_HOME-under-holdout checks are authoring-job checks: replicate calls run no tools.",
    "Exposure policy: no confirmation-3 content was shown to any proposer, harness or operator. Task content "
    "exists only under the holdout dir; only counts, opaque ids and gateway booleans left it (diagnostics of "
    "2026-10-19 printed fixed vocabularies only). A task found exposed is excluded before the hash; none was.",
]


def lanes_of(path):
    lanes = {}
    for m in re.finditer(r"^# lanes:\s*(.+)$", pathlib.Path(path).read_text(), re.M):
        for spec in m.group(1).split():
            name, value = spec.split("=", 1)
            lanes[name] = value
    return lanes


def look2_completed_by_t():
    """(PASS and completed by T, why): the look's last summary record time, metadata only."""
    if not LOOK2_GATE.exists():
        return False, "no look-2 gate"
    gate = json.loads(LOOK2_GATE.read_text())
    if not (gate.get("valid") is True and gate.get("gate") == "PASS"):
        return False, "look 2 not a valid PASS"
    led = LOOK2_RUN / "ledger.jsonl"
    stamps = [json.loads(x).get("ts_ms", 0) for x in led.read_text().splitlines() if '"summary"' in x] if led.exists() else []
    if not stamps or datetime.datetime.fromtimestamp(max(stamps) / 1000, datetime.timezone.utc) > T:
        return False, "look 2 not completed by T"
    return True, "look 2 PASS by T"


def candidate():
    """§0.1, mechanically: (path, why)."""
    ok, why = look2_completed_by_t()
    if ok:
        return LOOK2_SNAPSHOT, why
    if c3.sha256(E002) != E002_SHA:
        sys.exit("E002's frozen file changed")
    return E002, why + "; candidate E002"


def certified(tasks):
    """Step 5 is complete on exactly this suite (False on any error: fail closed)."""
    if not tasks:
        return False
    try:
        return _certified(tasks)
    except (OSError, ValueError, KeyError):
        return False


def _certified(tasks):
    import c3_adjudicate
    import c3_audit
    from c3lib import RECORDS, load_json
    audit = load_json(RECORDS / "audit.json", {})
    adj = load_json(RECORDS / "adjudication.json", {})
    excluded = set(load_json(RECORDS / "excluded.json", {"tasks": []})["tasks"])
    return (c3_audit.clean_suite(tasks) and all("r1" in audit.get(t, {}) for t in tasks)
            and all(c3_audit.needs(audit, adj, t) is None for t in tasks)
            and not c3_adjudicate.due(audit, adj, tasks) and not (excluded & set(tasks)))


def provider_commit():
    from c3lib import PROVIDER_COMMIT
    code, out = c3.git("rev-parse", "--verify", f"{PROVIDER_COMMIT}^{{commit}}")
    if code or c3.git("merge-base", "--is-ancestor", PROVIDER_COMMIT, "HEAD")[0]:
        return None
    return out.strip()


def build(args, suite):
    tasks = sorted(p.name for p in suite.iterdir() if TASK.match(p.name)) if suite.exists() else []
    hashed = subprocess.run([c3.A2D, "lift", "hash", str(suite)], capture_output=True, text=True)
    suite_hash = hashed.stdout.strip() if hashed.returncode == 0 else ""
    fp = c3.codex_fingerprint()
    controller = c3.git("log", "-1", "--format=%h %cI", "--", "crates", "Cargo.toml", "Cargo.lock")[1].strip()
    path, why = args.candidate_path, args.candidate_why
    families = {t.rsplit("-l", 1)[0] for t in tasks}
    shortfall = [] if len(families) >= 36 else [
        f"{len(families)} families (< 36 registered in §0.3): power is below the §0.3 statement; recompute with "
        f"c3_power.py before the review."]
    return {
        "experiment": "C2-CONFIRM3",
        "protocol": "plan docs/plans/2026-10-18-confirmation-3.md §0 (copied, not reopened; §0.10 allows the review only "
                    "to tighten §0.4) and §0.13; runner nano/confirm3.py generate / manifest / score, bound with this "
                    "file and frozen.py by the registration commit",
        "frozen": "runs/c2-confirm3/frozen.py",
        "frozen_sha": c3.sha256(path),
        "frozen_from": args.frozen_from,
        "candidate_rule": why,
        "provider_commit": provider_commit(),
        "suite": "~/.a2d-holdout/confirmation-3",
        "suite_hash": suite_hash,
        "tasks": tasks,
        "families": len(families),
        "arms": ["D", "B", "X:frozen"],
        "k": 5,
        "model": "codex/gpt-6-luna",
        "effort": "low",
        "lanes": lanes_of(path),
        "branch_caps": args.branch_caps,
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
        "disclosures": FIXED_DISCLOSURES + shortfall + list(args.disclosure),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--frozen-from", required=True)
    ap.add_argument("--claim-wording", required=True)
    ap.add_argument("--branch-caps", required=True)
    ap.add_argument("--disclosure", action="append", default=[])
    ap.add_argument("--suite", type=pathlib.Path, default=c3.SUITE, help="dry runs only")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    if args.suite != c3.SUITE and not args.dry_run:
        sys.exit("--suite is for dry runs only")
    args.candidate_path, args.candidate_why = candidate()
    reg = build(args, args.suite)
    if args.dry_run:
        print(json.dumps({**reg, "tasks": len(reg["tasks"])}, indent=1))
        return
    out = c3.DIR / "registration.json"
    if out.exists():
        sys.exit(f"{out} exists; the registration is written once")
    problems = [name for name, bad in (
        ("suite hash", not reg["suite_hash"]), ("codex fingerprint (native binary)", not reg["codex"]),
        ("provider merge", not reg["provider_commit"]), ("lanes", not reg["lanes"]),
        (f">= {MIN_FAMILIES} families", reg["families"] < MIN_FAMILIES),
        ("4 tasks per family", len(reg["tasks"]) != 4 * reg["families"]),
        ("certification", not certified(reg["tasks"])),
    ) if bad]
    if problems:
        sys.exit("not registrable: " + ", ".join(problems))
    c3.DIR.mkdir(parents=True, exist_ok=True)
    shutil.copy(args.candidate_path, c3.DIR / "frozen.py")
    out.write_text(json.dumps(reg, indent=1) + "\n")
    print(json.dumps({"written": str(out), "tasks": len(reg["tasks"]), "families": reg["families"],
                      "frozen_sha": reg["frozen_sha"][:12], "suite_hash": reg["suite_hash"][:12]}))


if __name__ == "__main__":
    main()
