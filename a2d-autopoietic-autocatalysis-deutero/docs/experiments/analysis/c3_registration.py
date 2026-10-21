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
LOOK2_MANIFEST = ROOT / "runs" / "c2-look2" / "manifest.json"
LOOK2_REGISTERED_AT = ROOT / "runs" / "c2-look2-registration" / "registered_at.txt"
BATCH = ("E005", "E006", "E007", "E008", "E009", "E010")
T = datetime.datetime(2026, 10, 23, 23, 59, tzinfo=datetime.timezone.utc)
MIN_FAMILIES = 30

SEEDS = {"1": 9191, "2": 9292}          # new: not confirmation-2's 8181/8282 nor the looks' 6161/6262
TASK = re.compile(r"^c3-f\d{2}-l\d$")
FIXED_DISCLOSURES = [
    "Authorship: the tasks were authored blind by Sol (gpt-6.1-sol, high) and audited by an independent Sol solve "
    "(medium); a Sol judge lane in the candidate is an author-judge alignment. The claim stays narrow, to this "
    "authoring distribution.",
    "Mid-authoring pipeline fixes (plan §0.13): the command scan's prose false positive (ecfb1c65), the retry "
    "codex-home reset (17da7217) and the mutants-directory check that allows non-source extras (99890cd2). Jobs "
    "before 08:00 UTC 10-19 ran under the earlier pipeline commit.",
    "Step order (plan §0.13): the step-5 audit ran from the provider branch (2830f5ba, 3ca9102b) before main merged "
    "the provider (c14956a0, 10-21); certification checks that each audit head contains the provider commit.",
    "Audit round 1's 4 voids were an instrument fault (§0.14: two concurrent calls sharing a cwd and CODEX_HOME), "
    "fixed before registration (36bea1e1); the 4 tasks were re-audited and solved in round 2.",
    "Provenance (Sol registration review): the last validation (10-21) built its own controller and recorded its "
    "git head and binary sha256, and certification requires that record. The audit runs recorded binary sha256 "
    "790bcf91... at heads 2830f5ba and 3ca9102b; debug builds are not byte-reproducible here, so the audit binary's "
    "provenance is that record plus functional evidence: every audited tuple has --json transcripts with the "
    "provider's marker, the codex bracket held, and each head contains the provider commit.",
    "Compile-time file reads (Sol registration review): the validator's API-surface check compiles model-written "
    "Rust outside the sandbox. A counts-only audit found 0 include_str!/include_bytes!/include!/env!/option_env! "
    "uses and 0 sealed-path mentions in the 1,152 suite files and the 5,456 readable authoring files.",
    "Instrument errors count as tool events (fail closed): codex `error` items, top-level `error` events and "
    "`turn.failed` count, so a burst of stream errors could void tuples or reach IntegrityReview; the rate is "
    "reported from the dev runs on the same provider.",
    "Operations during the spend windows: generation runs detached; other Luna work (phase-B rounds, dev runs) is "
    "held while a replicate generates; codex is not upgraded (a fingerprint change voids the replicate).",
    "Injected context: codex 0.162 injects its 4 bundled skills (~7.3k chars) and multi-agent role text (~2.8k) "
    "that no switch removes; identical in every arm, no task information.",
    "Instrument versus dev: the dev and dev-holdout estimates were measured with web search open (unused), plugins "
    "and user skills loaded; confirmation-3 runs with them off (C2-WEB, C2-MCP, plan §0.13). Symmetric across arms.",
    "Integrity policy: tuple-level voids with worst-case imputation, weaker than confirmation-2's any-event "
    "amendment but conservative for the claim; IntegrityReview above 0.5% tool events per replicate.",
    "Voids are imputed on the CI as well as the point (early reviews, plan §0.13): SUPPORTED needs the controller "
    "rule on complete tuples AND the worst-case imputed point >= +10 pp AND the worst-case imputed whole-family "
    "bootstrap ci_low > 0 vs both comparators; RULED OUT needs the controller RuledOut AND the best-case imputed "
    "ci_high < +10 pp. The bootstrap is the controller's (ported; exact on 4 dev comparisons).",
    "Codex version: --json transcripts carry no banner, so the binary is pinned by a sha256 fingerprint bracketing "
    "each replicate; a change voids that replicate's tuples.",
    "Spent sets: confirmation-2 (IntegrityReview, web search in arm B); its scored data stays sealed until this "
    "decision and is then reported descriptively. Dev-holdout looks: look 1 (E002, PASS) and look 2 (E007, invalid: "
    "one coincidental hidden-test name in a judge prompt, echoed from a Luna draft).",
    "Possible model drift between the dev runs (10-15 to 10-23) and the spend days.",
    "SPEC.md was kept byte-identical through authoring (Sol's suggestion #22 not adopted); the limits disclosed in "
    "§0.11 and the step-5 tooling choices of §0.12 apply (a void audit is no information and is re-audited once; a "
    "repair may change only the requirements, never the hidden tests, and must pass validation and similarity again; "
    "one task was so repaired).",
    "Authoring sandbox: a copied `security` binary can list the world-readable system keychain's metadata (no "
    "secrets); the login keychain file is unreadable (§0.12).",
    "The controller's own excluded-share guard (5% of (task, replicate) pairs, any arm void excludes the pair) is "
    "stricter than §0.13's 5% of (task, arm, replicate) tuples and also applies; c3_power assumed no voids, so the "
    "best-case RULED OUT guard lowers P(RULED OUT | 0) below 0.91.",
    "Latency go/no-go probes are model calls (3 Luna canary coding calls plus 1 per registered lane, per spend "
    "attempt) made before the attempt record; they count toward the daily cap.",
    "Launch isolation before each replicate (§0.10): no codex context in any ancestor of the temp root; every "
    "call's auth-only CODEX_HOME under the holdout dir (A2D_CODEX_HOME_ROOT); a canary preflight through the "
    "real provider on every lane (the secret in no response, 0 tool events); the codex fingerprint.",
    "Detector inventory (codex 0.162 exec item types): command_execution, file_change, mcp_tool_call, web_search, "
    "collab_tool_call and error have live positive controls on every lane (controls.json); todo_list could not be "
    "triggered in exec mode even with every feature on and is covered by the allowlist only; image tools have no "
    "exec item type and are disabled by feature flags.",
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


def committed(path):
    rel = str(path.relative_to(ROOT))
    return not c3.git("ls-files", "--error-unmatch", rel)[0] and not c3.git("diff", "--quiet", "HEAD", "--", rel)[0]


def promoted():
    """Batch ids promoted by confirm_best (a confirm-stage keep in results.tsv)."""
    lines = (ROOT / "nano" / "results.tsv").read_text().splitlines()
    rows = [dict(zip(lines[0].split("\t"), x.split("\t"))) for x in lines[1:] if x.strip()]
    return sorted({r["id"] for r in rows if r["id"] in BATCH and r["stage"] == "confirm" and r["decision"] == "keep"})


def look2_result():
    """None while look 2 has no committed result; else (PASS by T, why), with its provenance
    checked: exactly one promoted keep, the look run at its registration commit on that keep's
    evaluated snapshot, and the gate committed (content-free evidence only)."""
    if not LOOK2_GATE.exists():
        return None
    keeps = promoted()
    if len(keeps) != 1:
        sys.exit(f"look 2 exists but {len(keeps)} promoted keeps")
    manifest = json.loads(LOOK2_MANIFEST.read_text())
    snapshot_sha = c3.sha256(LOOK2_SNAPSHOT)
    if not (committed(LOOK2_GATE) and committed(LOOK2_MANIFEST) and committed(LOOK2_SNAPSHOT)):
        sys.exit("look 2's gate, manifest and snapshot must be committed and unmodified")
    if len(c3.git("log", "--format=%H", "--", str(LOOK2_GATE.relative_to(ROOT)))[1].split()) != 1:
        sys.exit("look 2's gate must have been committed exactly once")
    if manifest.get("registered_at") != LOOK2_REGISTERED_AT.read_text().strip():
        sys.exit("look 2 did not run at its registration commit")
    if not (manifest.get("incumbent_sha") == snapshot_sha == c3.sha256(ROOT / "runs" / f"c2-{keeps[0]}" / "candidate.py")):
        sys.exit("look 2's snapshot is not the promoted keep")
    gate = json.loads(LOOK2_GATE.read_text())
    led = LOOK2_RUN / "ledger.jsonl"
    stamps = [json.loads(x).get("ts_ms", 0) for x in led.read_text().splitlines() if '"summary"' in x] if led.exists() else []
    on_time = bool(stamps) and datetime.datetime.fromtimestamp(max(stamps) / 1000, datetime.timezone.utc) <= T
    if gate.get("valid") is True and gate.get("gate") == "PASS" and on_time:
        return True, f"look 2 PASS by T for {keeps[0]}"
    return False, f"look 2 {'not on time' if not on_time else 'not a valid PASS'} for {keeps[0]}"


def candidate(at=None):
    """§0.1, mechanically: (path, why). Fixed at the look's result, or at T; unresolved before."""
    result = look2_result()
    at = at or datetime.datetime.now(datetime.timezone.utc)
    if result is None and at <= T:
        sys.exit("the candidate is unresolved until T (2026-10-23 23:59 UTC) or a look-2 result")
    if result and result[0]:
        return LOOK2_SNAPSHOT, result[1]
    if c3.sha256(E002) != E002_SHA:
        sys.exit("E002's frozen file changed")
    return E002, (result[1] if result else "no look-2 result by T") + "; candidate E002"


def certified(tasks, reg=None):
    """Step 5 is complete on exactly this suite (False on any error: fail closed)."""
    if not tasks:
        return False
    try:
        return _certified(tasks, reg or {})
    except (OSError, ValueError, KeyError, TypeError):
        return False


def audit_instrument_ok(run, reg):
    """The audit run behind a task's final round ran on the registered provider (its git head
    contains the provider commit), with the registered codex, and its bracket held."""
    return (bool(run) and run.get("bracket_ok") is True and bool(reg.get("provider_commit"))
            and not c3.git("merge-base", "--is-ancestor", reg["provider_commit"], run.get("head") or "")[0]
            and (run.get("codex") or {}).get("files") == reg.get("codex_files"))


def _certified(tasks, reg):
    import c3_adjudicate
    import c3_audit
    from c3lib import RECORDS, load_json
    active = load_json(RECORDS / "active.json", {}).get("active")
    if not active or {t.rsplit("-l", 1)[0] for t in tasks} != set(active):
        return False                    # every active family, and nothing else, is in the suite
    for fid in active:
        if not load_json(RECORDS / f"author-{fid}" / "status.json", {}).get("done"):
            return False                # an unfinished family: authoring, then assembly, first
    audit = load_json(RECORDS / "audit.json", {})
    adj = load_json(RECORDS / "adjudication.json", {})
    excluded = set(load_json(RECORDS / "excluded.json", {"tasks": []})["tasks"])
    val = load_json(RECORDS / "validate.json", {})
    if not (val.get("built") is True and val.get("binary_sha256") and val.get("head") and reg.get("provider_commit")
            and not c3.git("merge-base", "--is-ancestor", reg["provider_commit"], val["head"])[0]):
        return False                    # the last validation built its own controller from a checkout with the provider
    from c3lib import SUITE, hashes_digest, tree_hashes
    runs = load_json(RECORDS / "audit-runs.json", {}).get("runs", {})
    for t in tasks:                     # the final informative audit round saw exactly the current task
        rounds = audit.get(t, {})
        final = rounds.get("r2") or rounds.get("r1") or {}
        if final.get("content") != hashes_digest(tree_hashes(SUITE / t)):
            return False
        if not audit_instrument_ok(runs.get(str(final.get("run"))), reg):
            return False
    return (c3_audit.clean_suite(tasks) and all("r1" in audit.get(t, {}) for t in tasks)
            and all(c3_audit.needs(audit, adj, t) is None for t in tasks)
            and not c3_adjudicate.due(audit, adj, tasks) and not (excluded & set(tasks)))


CONTROLS = c3.DIR / "controls.json"


def controls_ok(reg):
    """The live detector controls (c3_controls.py) cover the coder lane and every registered lane,
    are complete (negative 0 events; shell, web, file, MCP, sub-agent and error each detected),
    ran on a checkout containing the registered provider commit, with the registered codex."""
    if not CONTROLS.exists() or not committed(CONTROLS) or not reg.get("provider_commit"):
        return False
    rec = json.loads(CONTROLS.read_text())
    needed = {f"{reg['model']}:{reg['effort']}"} | set(reg["lanes"].values())
    head = rec.get("provider_checkout_head") or ""
    return (rec.get("complete") is True and needed <= set(rec.get("lanes", []))
            and bool(head) and not c3.git("merge-base", "--is-ancestor", reg["provider_commit"], head)[0]
            and (rec.get("codex") or {}).get("files") == reg.get("codex_files"))


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
    events = []
    try:
        from c3lib import RECORDS, load_json
        events = load_json(RECORDS / "active.json", {}).get("events", [])
    except (OSError, ValueError):
        pass
    shortfall = ([f"Assembly replaced or dropped {len(events)} active families by recorded events "
                  f"(authoring exhaustion or validation caps, §0.11 and the 10-18 addendum)."] if events else []) + \
        ([] if len(families) >= 36 else [
        f"{len(families)} families (< 36 registered in §0.3): power is below the §0.3 statement; recompute with "
        f"c3_power.py before the review."])
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
            "SUPPORTED": "X vs D and X vs B both Supported by the controller rule on complete tuples (ci_low > 0, lift "
                         ">= +10 pp, >= 10 families, discordance) AND, over every planned (task, replicate) with voids "
                         "imputed worst-case (void X = fail, void D/B = pass), lift >= +10 pp and whole-family "
                         "bootstrap ci_low > 0 vs both",
            "RULED OUT": "X vs D RuledOut on complete tuples (ci_high < +10 pp) AND best-case imputed (void X = pass, "
                         "void D = fail) point and bootstrap ci_high both < +10 pp",
            "IntegrityReview": "any hidden-test name hit (pooled summary, either replicate's own summary, or a "
                               "failed first attempt's prompts), tool events > 0.5% of a replicate's invocations "
                               "(the larger of the submission and invocation-record sums, a failed first attempt's "
                               "calls included, each call without an auditable transcript counted as one event), or "
                               "the controller's own IntegrityReview",
            "Invalid": "void tuples > 5% of all (task, arm, replicate) tuples, the controller's excluded-share guard or "
                       "own Invalid, or a missing comparison (an aborted generation is a failed attempt: failure_policy)",
            "void tuple": "void or unscored submission; any tool event on the submission or on any of its "
                          "invocation records (lane calls included); any invocation without a --json transcript "
                          "carrying the stderr marker (a legacy text transcript is void); recorded invocations != "
                          "the submission's claimed count; a malformed record; all tuples of a replicate whose codex "
                          "fingerprint changed or whose bracket record is missing",
            "otherwise": "INCONCLUSIVE",
        },
        "codex": fp,
        "codex_version": fp["version"] if fp else None,
        "codex_files": fp["files"] if fp else None,
        "spend": {"earliest_utc_day": str(c3.EARLIEST), "after_registration_day": True,
                  "requires": "confirmation-2 sealed (runs/c2-confirm2/sealed-sha256.txt), the authoring pipeline "
                              "drained, codex = the registered fingerprint",
                  "latency_go": f"median of {c3.LATENCY_PROBES} canary coding calls on the coder lane through "
                                f"`a2d lift run` <= {c3.LATENCY_MAX_MEDIAN_SECS:g} s, plus one canary call per registered "
                                "lane; all with 0 tool events and the canary secret in no response",
                  "daily_cap": c3.DAILY_CAP, "replicate_2": "a later UTC day than replicate 1's generation"},
        "failure_policy": "each replicate is attempted once (attempt record before any model call); a deferral is not "
                          "an attempt; a generation that fails before generation_complete may be relaunched once with "
                          "the same seed in its own dir, logged; a second failure ends confirmation-3 as not run",
        "instrument": {"controller_last_change": controller,
                       "provider": "c2-web-provider merged into main at c14956a0 (branch head ecbb9611, including the "
                                   "§0.14 cwd/CODEX_HOME uniqueness fix 36bea1e1): no web search, isolated CODEX_HOME per "
                                   "call, tool features disabled, --json allowlist tool counting, failed-call transcripts "
                                   "(plan §0.13)",
                       "launch": "each generate runs detached (own session, caffeinate) from the queue runner; the "
                                 "controller is built before the preflight, and the suite hash and the crates binding "
                                 "are checked before the attempt record",
                       "binding": "the controller refuses to run or score if crates/, Cargo.toml or Cargo.lock differ "
                                  "from the registration commit"},
        "power": "docs/experiments/analysis/c3_power.py (dev summaries only): P(RULED OUT | 0) = 0.91; P(SUPPORTED) = "
                 "0.14 / 0.33 / 0.47 / 0.96 at +7.1 / +8.9 / +10 / +15 pp",
        "expected_spend_per_replicate": "about 1.6k invocations, Luna and Sol together (confirmation-2: 1,618 and "
                                        "1,617), of which about 1.5k Luna",
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
    from c3lib import HOLDOUT as PIPELINE_HOLDOUT
    if PIPELINE_HOLDOUT.resolve() != c3.HOLDOUT.resolve():
        sys.exit("the pipeline's holdout dir (A2D_HOLDOUT_DIR) differs from the runner's")
    args.candidate_path, args.candidate_why = candidate(
        datetime.datetime.max.replace(tzinfo=datetime.timezone.utc) if args.dry_run else None)
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
        ("certification", not certified(reg["tasks"], reg)),
        ("live detector controls on every lane (c3_controls.py)", not controls_ok(reg)),
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
