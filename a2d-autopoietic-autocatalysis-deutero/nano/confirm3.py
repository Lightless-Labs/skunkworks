"""Confirmation-3: scoring rules and the replicate runner (docs/plans/2026-10-18-confirmation-3.md
§0.4, §0.10, §0.13).

usage: python3 -I nano/confirm3.py generate --rep N --registered-at <sha> [--relaunch]
       python3 -I nano/confirm3.py manifest --registered-at <sha>
       python3 -I nano/confirm3.py score --registered-at <sha>

Stdlib only and self-contained, so the registration can bind this file by sha256.
The claim is the controller's, re-applied to its own pooled comparison numbers (the
whole-family bootstrap on complete (task, replicate) tuples). Then:
- a (task, arm, replicate) tuple is void if its submission is void or unscored, if it
  has a tool event, or if any invocation lacks a transcript or ran another codex version;
- SUPPORTED also needs the worst-case imputed point lift >= +10 pp vs D and B
  (void X = fail, void D/B = pass), over every planned (task, replicate);
- RULED OUT also needs the best-case imputed point lift vs D < +10 pp
  (void X = pass, void D = fail);
- run level: a hidden-test name hit or a replicate with tool events in > 0.5% of its
  invocations -> IntegrityReview; void tuples > 5% of all tuples, or the controller's
  validity guard -> Invalid. A single tool event voids its tuple only (§0.4), so the
  controller's own any-tool-call IntegrityReview is not used.
Outputs are counts and rates only: no task ids, test names or model text.
"""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

MIN_EFFECT = 0.10
EPS = 1e-9                      # the controller's BOUNDARY_EPS
TOOL_RATE_MAX = 0.005
VOID_SHARE_MAX = 0.05
X = "X:frozen"
COMPARATORS = ("D", "B")
BANNER = re.compile(r"^OpenAI Codex v(\S+)")          # legacy text transcripts only
CODEX_STDERR_MARKER = "\n--- codex stderr ---\n"     # a2d-providers: --json events, marker, stderr


def records(out):
    return [json.loads(line) for line in (pathlib.Path(out) / "ledger.jsonl").read_text().splitlines() if line.strip()]


def transcript_ok(out, prompt_path, pinned):
    """The invocation's transcript exists under this out dir and was written by the pinned codex."""
    parts = pathlib.PurePath(prompt_path).parts
    if "invocations" not in parts or not parts[-1].endswith(".prompt.txt"):
        return False
    t = pathlib.Path(out).joinpath(*parts[parts.index("invocations"):-1], parts[-1][:-len(".prompt.txt")] + ".transcript.txt")
    if not t.is_file():
        return False
    with open(t, errors="replace") as f:
        if CODEX_STDERR_MARKER in f.read():
            return True             # --json transcript: no banner; the version is pinned by bracketing the replicate
        f.seek(0)
        # Log noise can precede the banner (as in confirmation-2's web audit).
        for line in (next(f, "") for _ in range(20)):
            m = BANNER.match(line)
            if m:
                return m.group(1) == pinned
    return False


def replicate_tuples(out, rep, pinned):
    """({(rep, arm, task): {"pass": bool | None, "void": bool}} for every planned (arm, task),
    {"invocations": n, "tool_calls": n}) for one replicate's out dir."""
    recs = records(out)
    reg = next(r for r in recs if r.get("kind") == "registration")
    tuples = {(rep, arm, task): {"pass": None, "void": False} for arm in reg["arms"] for task in reg["tasks"]}
    stats = {"invocations": 0, "tool_calls": 0}
    for r in recs:
        key = (rep, r.get("arm"), r.get("task"))
        kind = r.get("kind")
        if kind == "submission":
            stats["invocations"] += r.get("invocations") or 0
            stats["tool_calls"] += r.get("tool_calls") or 0
        if key not in tuples:
            continue
        if kind == "submission" and (r.get("void_reason") or (r.get("tool_calls") or 0) > 0):
            tuples[key]["void"] = True
        elif kind == "score":
            tuples[key]["pass"] = bool(r["result"]["all_passed"])
        elif kind == "invocation" and not transcript_ok(out, r.get("prompt_path", ""), pinned):
            tuples[key]["void"] = True
    for v in tuples.values():
        if v["pass"] is None:
            v["void"] = True
    return tuples, stats


def imputed_lift(tuples, comp, case):
    """Point all-pass lift of X over comp on every planned (replicate, task), voids imputed
    worst-case (X fail, comp pass) or best-case (X pass, comp fail)."""
    keys = sorted({(rep, task) for rep, _, task in tuples})
    worst = case == "worst"
    xs = ys = 0
    for rep, task in keys:
        x, y = tuples[(rep, X, task)], tuples[(rep, comp, task)]
        xs += (not worst) if x["void"] else bool(x["pass"])
        ys += worst if y["void"] else bool(y["pass"])
    return (xs - ys) / len(keys) if keys else 0.0


def classify(c, min_families):
    """crates/a2d-lift/src/stats.rs classify_guarded, on a pooled comparison record."""
    if c["families"] < min_families or (c["discordant_treatment_only"] == 0 and c["discordant_comparator_only"] == 0):
        return "Inconclusive"
    lo, hi = c["ci95"]
    if lo > EPS and c["lift"] >= MIN_EFFECT - EPS:
        return "Supported"
    if hi < MIN_EFFECT - EPS:
        return "RuledOut"
    return "Inconclusive"


def decide(summary, tuples, rep_stats):
    """The confirmation-3 outcome from the pooled controller summary and the tuples."""
    classes = {}
    for comp in COMPARATORS:
        c = next((c for c in summary["comparisons"] if c["treatment"] == X and c["comparator"] == comp), None)
        classes[comp] = classify(c, summary["min_families"]) if c else "Invalid"
    worst = {comp: round(imputed_lift(tuples, comp, "worst"), 4) for comp in COMPARATORS}
    best_d = round(imputed_lift(tuples, "D", "best"), 4)
    void_share = sum(v["void"] for v in tuples.values()) / len(tuples) if tuples else 1.0
    tool_rates = {rep: (s["tool_calls"] / s["invocations"] if s["invocations"] else 1.0) for rep, s in rep_stats.items()}
    integrity = bool(summary.get("hidden_name_hits")) or any(r > TOOL_RATE_MAX + EPS for r in tool_rates.values())
    invalid = (void_share > VOID_SHARE_MAX + EPS or summary["excluded_share"] > summary["max_excluded_share"] + 1e-12
               or summary.get("aborted") or "Invalid" in classes.values())
    if integrity:
        overall = "IntegrityReview"
    elif invalid:
        overall = "Invalid"
    elif all(c == "Supported" for c in classes.values()):
        overall = "SUPPORTED" if min(worst.values()) >= MIN_EFFECT - EPS else "INCONCLUSIVE (worst-case void imputation)"
    elif classes["D"] == "RuledOut":
        overall = "RULED OUT" if best_d < MIN_EFFECT - EPS else "INCONCLUSIVE (best-case void imputation)"
    else:
        overall = "INCONCLUSIVE"
    return {"overall": overall, "classes": classes, "worst_case_void_lifts": worst, "best_case_void_lift_vs_D": best_d,
            "void_tuples": sum(v["void"] for v in tuples.values()), "tuples": len(tuples),
            "void_share": round(void_share, 4), "tool_event_rates": {str(k): round(v, 5) for k, v in tool_rates.items()},
            "hidden_name_hits": len(summary.get("hidden_name_hits") or []), "controller_claim": summary["claim"]}



# ------------------------------------------------------------------------------------ runner
# Generation (one replicate per call), manifests and scoring. Every gate that defers writes
# nothing; the attempt record is written before the first model call.
ROOT = pathlib.Path(__file__).resolve().parent.parent
HOLDOUT = pathlib.Path.home() / ".a2d-holdout"
SUITE = HOLDOUT / "confirmation-3"
DIR = ROOT / "runs" / "c2-confirm3"
A2D = str(ROOT / "target" / "debug" / "a2d")
LUNA = "codex/gpt-6-luna"
DAILY_CAP = 3000
EARLIEST = datetime.date(2026, 10, 24)
BOUND = ("runs/c2-confirm3/registration.json", "runs/c2-confirm3/frozen.py", "nano/confirm3.py")
C2_SEALED = ROOT / "runs" / "c2-confirm2" / "sealed-sha256.txt"
LATENCY_PROBES = 3
LATENCY_MAX_MEDIAN_SECS = 45.0
PONG = "Reply with the single word PONG."


def emit(**kw):
    print(json.dumps(kw), flush=True)


def git(*a):
    r = subprocess.run(["git", "-C", str(ROOT), *a], capture_output=True, text=True)
    return r.returncode, r.stdout


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def today():
    return datetime.datetime.now(datetime.timezone.utc).date()


def utc_date(ms):
    return datetime.datetime.fromtimestamp(ms / 1000, datetime.timezone.utc).date()


def commit_day(sha):
    return utc_date(int(git("show", "-s", "--format=%ct", sha)[1].strip()) * 1000)


def out_dir(rep, relaunch=False):
    return HOLDOUT / "runs" / (f"c2-confirm3-r{rep}" + ("-relaunch1" if relaunch else ""))


def suffix(relaunch):
    return "-relaunch1" if relaunch else ""


def complete(out):
    """Generation finished and was not aborted."""
    if not (pathlib.Path(out) / "ledger.jsonl").exists():
        return False
    return any(r.get("kind") == "generation_complete" and r.get("aborted") is False for r in records(out))


def generation_day(out):
    return utc_date(next(r for r in records(out) if r.get("kind") == "generation_complete")["ts_ms"])


def registration(registered_at):
    """The registration commit binds registration.json, the frozen candidate and this file;
    the controller binds crates/ itself (--registered-at)."""
    if git("merge-base", "--is-ancestor", registered_at, "HEAD")[0]:
        sys.exit(f"{registered_at} is not a commit on HEAD's history")
    for rel in BOUND:
        code, then = git("show", f"{registered_at}:./{rel}")
        if code or then != (ROOT / rel).read_text():
            sys.exit(f"{rel} differs from the registration commit {registered_at}")
    if git("merge-base", "--is-ancestor", registered_at, "origin/main")[0]:
        sys.exit(f"push the registration commit {registered_at} first")
    reg = json.loads((DIR / "registration.json").read_text())
    if sha256(DIR / "frozen.py") != reg["frozen_sha"]:
        sys.exit("the frozen candidate does not match registration.json")
    return reg


def spend_gate(day, registered_day, c2_sealed, rep, rep1_day):
    """§0.10 spend day: None means go, otherwise the reason to defer."""
    if day < EARLIEST:
        return "before 2026-10-24"
    if day <= registered_day:
        return "registration day"
    if not c2_sealed:
        return "confirmation-2 decision missing"
    if rep == 2 and (rep1_day is None or day <= rep1_day):
        return "replicate 1's day"
    return None


def latency_go(secs):
    """Pre-registered go/no-go: median of the probe calls (a failed probe is infinite)."""
    return statistics.median(secs) <= LATENCY_MAX_MEDIAN_SECS


def latency_probe(model, effort, n=LATENCY_PROBES):
    """n trivial calls of the registered model, from empty dirs with an isolated CODEX_HOME."""
    secs = []
    auth = pathlib.Path(os.environ.get("CODEX_HOME", str(pathlib.Path.home() / ".codex"))) / "auth.json"
    for _ in range(n):
        with tempfile.TemporaryDirectory() as d:
            home = pathlib.Path(d) / "home"
            home.mkdir()
            (home / "auth.json").symlink_to(auth)
            start = time.monotonic()
            try:
                r = subprocess.run(["codex", "exec", "--json", "--skip-git-repo-check", "-m", model.split("/", 1)[1],
                                    "-c", f"model_reasoning_effort={effort}", "-c", 'web_search="disabled"', "-"],
                                   input=PONG, capture_output=True, text=True, cwd=d, timeout=300,
                                   env={**os.environ, "CODEX_HOME": str(home)})
                secs.append(time.monotonic() - start if r.returncode == 0 else float("inf"))
            except subprocess.TimeoutExpired:
                secs.append(float("inf"))
    return secs


def codex_fingerprint():
    """The codex binary and its helpers (sha256 of every file next to the resolved binary)."""
    exe = shutil.which("codex")
    if not exe:
        return None
    real = pathlib.Path(os.path.realpath(exe))
    files = {p.name: sha256(p) for p in sorted(real.parent.iterdir()) if p.is_file()}
    version = subprocess.run([str(real), "--version"], capture_output=True, text=True).stdout.strip()
    return {"realpath": str(real), "files": files, "version": version}


def drained():
    """§0.10 drain: the authoring pipeline's own fail-closed check."""
    r = subprocess.run([sys.executable, "-I", str(ROOT / "tools" / "holdout" / "c3_author.py"), "--drained"],
                       capture_output=True, text=True)
    lines = [json.loads(x) for x in r.stdout.splitlines() if x.strip().startswith("{")]
    return r.returncode == 0 and bool(lines) and lines[-1].get("ok") is True


def luna_today():
    """Luna invocations since 00:00 UTC across this cycle's runs (the same ledgers as eval.py)."""
    lo_ms = datetime.datetime.combine(today(), datetime.time(), datetime.timezone.utc).timestamp() * 1000
    seen, total = set(), 0
    ledgers = list((ROOT / "runs").glob("c2-*/*/ledger.jsonl")) + list((ROOT / "runs").glob("c2-*/ledger.jsonl")) \
        + list((ROOT / "runs").glob("c2-*/*/*/ledger.jsonl")) \
        + list((HOLDOUT / "runs").glob("c2-*/ledger.jsonl")) + list((HOLDOUT / "audit").glob("*/*/ledger.jsonl"))
    for led in ledgers:
        real = led.resolve()
        if real in seen or real.stat().st_mtime * 1000 < lo_ms:
            continue
        seen.add(real)
        with open(real) as f:
            for line in f:
                if '"invocation"' in line and LUNA in line:
                    r = json.loads(line)
                    if r.get("kind") == "invocation" and r.get("provider") == LUNA and r.get("ts_ms", 0) >= lo_ms:
                        total += 1
    return total


def cmd_generate(args):
    """One replicate, attempted once (one logged relaunch with the same seed if it fails)."""
    reg = registration(args.registered_at)
    rep, relaunch = args.rep, args.relaunch
    stage = f"confirm3-r{rep}"
    if complete(out_dir(rep)) or complete(out_dir(rep, True)):
        emit(stage=stage, decision="already generated")
        return
    out, attempt = out_dir(rep, relaunch), DIR / f"attempt-r{rep}{suffix(relaunch)}.json"
    if relaunch and not (DIR / f"attempt-r{rep}.json").exists():
        sys.exit("a relaunch needs a failed first attempt")
    if attempt.exists() or out.exists():
        sys.exit(f"replicate {rep} was attempted without completing"
                 + ("; it is reported as failed" if relaunch else "; relaunch once with --relaunch, or report it as failed"))
    rep1 = next((o for o in (out_dir(1), out_dir(1, True)) if complete(o)), None)
    if rep == 2 and rep1 is None:
        sys.exit("replicate 1 has not generated")
    reason = spend_gate(today(), commit_day(args.registered_at), C2_SEALED.exists(), rep,
                        generation_day(rep1) if rep == 2 else None)
    if reason:
        emit(stage=stage, decision=f"deferred ({reason})")
        return
    if not drained():
        sys.exit("the authoring pipeline is not drained (STOP, no driver, no job process)")
    before = codex_fingerprint()
    if before is None or before["version"] != reg["codex_version"] or before["files"] != reg["codex_files"]:
        sys.exit("codex differs from the registered binary")
    worst = len(reg["tasks"]) * len(reg["arms"]) * reg["k"]
    spent = luna_today()
    if spent + worst + LATENCY_PROBES > DAILY_CAP:
        emit(stage=stage, decision="deferred (daily Luna cap)", luna_today=spent, stage_worst_case=worst, cap=DAILY_CAP)
        return
    secs = latency_probe(reg["model"], reg["effort"])
    probe = {"rep": rep, "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
             "secs": [round(s, 1) if s != float("inf") else None for s in secs], "go": latency_go(secs)}
    with open(DIR / "latency.jsonl", "a") as f:
        f.write(json.dumps(probe) + "\n")
    if not probe["go"]:
        emit(stage=stage, decision="deferred (latency)", secs=probe["secs"])
        return
    # Durable attempt record before any model call of the replicate itself.
    attempt.write_text(json.dumps({"rep": rep, "relaunch": relaunch, "registered_at": args.registered_at,
                                   "codex": before, "controller_binary_sha256": sha256(A2D),
                                   "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat()}) + "\n")
    cmd = [A2D, "lift", "run", "--experiment", f"C2-CONFIRM3-R{rep}", "--registered-at", args.registered_at,
           "--expect-suite-hash", reg["suite_hash"], "--suite", str(SUITE), "--model", reg["model"],
           "--effort", reg["effort"], "--arms", ",".join(reg["arms"]), "--k", str(reg["k"]), "--replicates", "1",
           "--primary", "D", "--seed", str(reg["seeds"][str(rep)]), "--jobs", str(args.jobs),
           "--defer-scoring", "--claim", reg["claim"], "--max-invocations", str(worst + 40),
           "--max-wall-clock-secs", "43200", "--out", str(out), "--external", f"frozen={DIR / 'frozen.py'}"]
    for name, spec in reg["lanes"].items():
        cmd += ["--lane", f"{name}={spec}"]
    out.parent.mkdir(parents=True, exist_ok=True)
    with open(str(out) + ".log", "a") as log:
        code = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT).returncode
    after = codex_fingerprint()
    (DIR / f"bracket-r{rep}{suffix(relaunch)}.json").write_text(
        json.dumps({"before": before, "after": after, "match": after == before}, indent=1) + "\n")
    if code or not complete(out):
        sys.exit(f"generation failed ({code}); " + ("reported as failed" if relaunch else "relaunch once with --relaunch"))
    emit(stage=stage, decision="generated (scoring deferred)")


def generated_out(rep):
    """(out dir, relaunch) of the completed generation of a replicate."""
    for relaunch in (False, True):
        if complete(out_dir(rep, relaunch)):
            return out_dir(rep, relaunch), relaunch
    sys.exit(f"replicate {rep} has not generated")


def cmd_manifest(args):
    """Content-free anchors of both replicates, committed before scoring."""
    registration(args.registered_at)
    for rep in (1, 2):
        out, _ = generated_out(rep)
        path = DIR / f"manifest-r{rep}.json"
        if path.exists():
            sys.exit(f"{path} exists; anchors are written once")
        r = subprocess.run([A2D, "lift", "manifest", str(out)], capture_output=True, text=True)
        if r.returncode:
            sys.exit(r.stderr)
        path.write_text(r.stdout)
    emit(stage="confirm3-manifest", decision="written; commit and push them, then score")


def cmd_score(args):
    """Score each replicate once against its committed anchor, pool, and decide (§0.13)."""
    reg = registration(args.registered_at)
    decision_path = DIR / "decision.json"
    if decision_path.exists():
        sys.exit(f"{decision_path} exists; the set is scored once")
    outs = {}
    for rep in (1, 2):
        rel = f"runs/c2-confirm3/manifest-r{rep}.json"
        if git("ls-files", "--error-unmatch", rel)[0] or git("diff", "--quiet", "HEAD", "--", rel)[0] \
                or git("merge-base", "--is-ancestor", "HEAD", "origin/main")[0]:
            sys.exit(f"commit and push {rel} before scoring")
        outs[rep] = generated_out(rep)
    for rep, (out, _) in outs.items():
        if (out / "summary.json").exists():
            continue
        head = json.loads((DIR / f"manifest-r{rep}.json").read_text())["ledger_head"]
        with open(str(out) + ".score.log", "w") as log:
            code = subprocess.run([A2D, "lift", "score", "--out", str(out), "--suite", str(SUITE),
                                   "--expect-head", head, "--jobs", "6"], stdout=log, stderr=subprocess.STDOUT).returncode
        if code:
            sys.exit(f"scoring replicate {rep} failed ({code}); see {out}.score.log")
    pooled = DIR / "pooled-summary.json"
    r = subprocess.run([A2D, "lift", "pool", "--out", str(pooled)]
                       + [str(out / "ledger.jsonl") for out, _ in outs.values()], capture_output=True, text=True)
    if r.returncode:
        sys.exit(r.stderr)
    pinned = reg["codex_version"].split()[-1]
    tuples, stats = {}, {}
    for rep, (out, relaunch) in outs.items():
        t, stats[rep] = replicate_tuples(out, rep, pinned)
        if not json.loads((DIR / f"bracket-r{rep}{suffix(relaunch)}.json").read_text())["match"]:
            for v in t.values():
                v["void"] = True            # another codex ran during the replicate
        tuples.update(t)
    decision = decide(json.loads(pooled.read_text()), tuples, stats)
    decision_path.write_text(json.dumps(decision, indent=1) + "\n")
    print(json.dumps(decision, indent=1))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    g = sub.add_parser("generate")
    g.add_argument("--rep", type=int, choices=(1, 2), required=True)
    g.add_argument("--registered-at", required=True)
    g.add_argument("--relaunch", action="store_true")
    g.add_argument("--jobs", type=int, default=8, choices=[8])
    for name in ("manifest", "score"):
        s = sub.add_parser(name)
        s.add_argument("--registered-at", required=True)
    args = ap.parse_args()
    {"generate": cmd_generate, "manifest": cmd_manifest, "score": cmd_score}[args.cmd](args)


if __name__ == "__main__":
    main()
