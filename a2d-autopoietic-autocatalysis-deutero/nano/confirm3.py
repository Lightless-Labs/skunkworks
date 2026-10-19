"""Confirmation-3: scoring rules and the replicate runner (docs/plans/2026-10-18-confirmation-3.md
§0.4, §0.10, §0.13; early reviews 2026-10-19).

usage: python3 -I nano/confirm3.py generate --rep N --registered-at <sha> [--relaunch]
       python3 -I nano/confirm3.py manifest --registered-at <sha>
       python3 -I nano/confirm3.py score --registered-at <sha>

Stdlib only and self-contained, so the registration commit binds this file by sha256.

Decision (decide):
- A (task, arm, replicate) tuple is void if its submission is void or unscored, if it has a
  tool event, if any invocation lacks a --json transcript, or if its invocation records do
  not match its submission's count. All tuples of a replicate whose codex fingerprint changed
  are void. Planned tuples come from the registration.
- The controller's own Invalid or IntegrityReview dominates, as do: a hidden-test name hit or
  tool events in > 0.5% of a replicate's invocations (IntegrityReview); void tuples > 5% of all
  tuples, the controller's excluded-share guard, an aborted generation (Invalid).
- SUPPORTED needs, vs both D and B: the controller rule on complete tuples (ci_low > 0, lift >=
  +10 pp, >= 10 families, discordance), the worst-case imputed point lift >= +10 pp (void X =
  fail, void D/B = pass) and the worst-case imputed whole-family bootstrap ci_low > 0.
- RULED OUT needs X vs D RuledOut on complete tuples and the best-case imputed (void X = pass,
  void D = fail) bootstrap ci_high < +10 pp.
Outputs are counts and rates only: no task ids, test names or model text.
"""
import argparse
import datetime
import fcntl
import hashlib
import json
import os
import pathlib
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
CODEX_STDERR_MARKER = "\n--- codex stderr ---\n"     # a2d-providers: --json events, marker, stderr
BOOTSTRAP_ITERATIONS = 10_000                         # the controller's
BOOTSTRAP_SEED = 0x5EED_A2D0
MASK64 = (1 << 64) - 1


def records(out):
    return [json.loads(line) for line in (pathlib.Path(out) / "ledger.jsonl").read_text().splitlines() if line.strip()]


def transcript_ok(out, prompt_path):
    """The invocation's transcript exists under this out dir in the merged provider's --json
    format (events, marker, stderr). A legacy text transcript means another provider: void.
    The codex version is pinned by bracketing the replicate (no banner under --json)."""
    parts = pathlib.PurePath(prompt_path).parts
    if "invocations" not in parts or not parts[-1].endswith(".prompt.txt"):
        return False
    t = pathlib.Path(out).joinpath(*parts[parts.index("invocations"):-1], parts[-1][:-len(".prompt.txt")] + ".transcript.txt")
    if not t.is_file() or t.is_symlink():
        return False
    with open(t, errors="replace") as f:
        return CODEX_STDERR_MARKER in f.read()


def replicate_tuples(out, rep, arms, tasks):
    """({(rep, arm, task): {"pass": bool | None, "void": bool}} for every planned (arm, task),
    {"invocations": n, "tool_calls": n}) for one replicate's out dir. The plan (arms, tasks)
    comes from the registration; a ledger planned otherwise voids every tuple."""
    recs = records(out)
    led = next((r for r in recs if r.get("kind") == "registration"), {})
    tuples = {(rep, arm, task): {"pass": None, "void": False} for arm in arms for task in tasks}
    stats = {"invocations": 0, "tool_calls": 0}
    if list(led.get("arms") or []) != list(arms) or sorted(led.get("tasks") or []) != sorted(tasks):
        for v in tuples.values():
            v["void"] = True
    claimed, seen = {}, {}
    for r in recs:
        key = (rep, r.get("arm"), r.get("task"))
        kind = r.get("kind")
        if kind == "submission":
            stats["invocations"] += r.get("invocations") or 0
            stats["tool_calls"] += r.get("tool_calls") or 0
        if key not in tuples:
            continue
        if kind == "submission":
            claimed[key] = claimed.get(key, 0) + (r.get("invocations") or 0)
            if r.get("void_reason") or (r.get("tool_calls") or 0) > 0:
                tuples[key]["void"] = True
        elif kind == "score":
            tuples[key]["pass"] = bool(r["result"]["all_passed"])
        elif kind == "invocation":
            seen[key] = seen.get(key, 0) + 1
            if not transcript_ok(out, r.get("prompt_path", "")):
                tuples[key]["void"] = True
    for key, v in tuples.items():
        if v["pass"] is None or claimed.get(key, 0) != seen.get(key, 0):
            v["void"] = True
    return tuples, stats


def family(task):
    return task.rsplit("-l", 1)[0]


def imputed(tuples, comp, case):
    """[(family, x_pass, y_pass)] over every planned (replicate, task) in sorted order, voids
    imputed worst-case (X fail, comp pass) or best-case (X pass, comp fail)."""
    worst = case == "worst"
    obs = []
    for rep, task in sorted({(rep, task) for rep, _, task in tuples}):
        x, y = tuples[(rep, X, task)], tuples[(rep, comp, task)]
        obs.append((family(task), (not worst) if x["void"] else bool(x["pass"]),
                    worst if y["void"] else bool(y["pass"])))
    return obs


def bootstrap(obs, iterations=BOOTSTRAP_ITERATIONS, seed=BOOTSTRAP_SEED):
    """(lift, ci_low, ci_high): crates/a2d-lift/src/stats.rs paired_summary's whole-family
    percentile bootstrap (SplitMix64, families in first-observation order)."""
    ids, clusters = {}, []
    for fam, xp, yp in obs:
        if fam not in ids:
            ids[fam] = len(clusters)
            clusters.append([0, 0, 0])
        c = clusters[ids[fam]]
        c[0] += 1
        c[1] += xp
        c[2] += yp
    n = len(obs)
    lift = (sum(c[1] for c in clusters) - sum(c[2] for c in clusters)) / n if n else 0.0
    if len(clusters) < 2 or iterations == 0:
        return lift, lift, lift
    state, k, samples = seed & MASK64, len(clusters), []
    for _ in range(iterations):
        size = sx = sy = 0
        for _ in range(k):
            state = (state + 0x9E3779B97F4A7C15) & MASK64
            z = state
            z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
            z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
            z ^= z >> 31
            count, cx, cy = clusters[z % k]
            size += count
            sx += cx
            sy += cy
        samples.append(sx / size - sy / size)
    samples.sort()
    last = iterations - 1
    return lift, samples[int(0.025 * last)], samples[_ceil(0.975 * last)]


def _ceil(x):
    i = int(x)
    return i if i == x else i + 1


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


def decide(summary, tuples, rep_stats, iterations=BOOTSTRAP_ITERATIONS):
    """The confirmation-3 outcome from the pooled controller summary and the tuples."""
    comparisons, classes = {}, {}
    for comp in COMPARATORS:
        c = next((c for c in summary["comparisons"] if c["treatment"] == X and c["comparator"] == comp), None)
        comparisons[comp] = c
        classes[comp] = classify(c, summary["min_families"]) if c else "Invalid"
    worst = {comp: bootstrap(imputed(tuples, comp, "worst"), iterations) for comp in COMPARATORS}
    best_d = bootstrap(imputed(tuples, "D", "best"), iterations)
    voids = sum(v["void"] for v in tuples.values())
    void_share = voids / len(tuples) if tuples else 1.0
    tool_rates = {rep: (s["tool_calls"] / s["invocations"] if s["invocations"] else 1.0) for rep, s in rep_stats.items()}
    controller_classes = [c.get("class") for c in comparisons.values() if c]
    integrity = (bool(summary.get("hidden_name_hits")) or any(r > TOOL_RATE_MAX + EPS for r in tool_rates.values())
                 or summary.get("claim") == "IntegrityReview" or summary.get("integrity_ok") is False
                 or "IntegrityReview" in controller_classes)
    invalid = (void_share > VOID_SHARE_MAX + EPS or summary["excluded_share"] > summary["max_excluded_share"] + 1e-12
               or bool(summary.get("aborted")) or summary.get("claim") == "Invalid" or "Invalid" in controller_classes
               or "Invalid" in classes.values())
    if integrity:
        overall = "IntegrityReview"
    elif invalid:
        overall = "Invalid"
    elif all(c == "Supported" for c in classes.values()):
        ok = all(lift >= MIN_EFFECT - EPS and lo > EPS for lift, lo, _ in worst.values())
        overall = "SUPPORTED" if ok else "INCONCLUSIVE (worst-case void imputation)"
    elif classes["D"] == "RuledOut":
        overall = "RULED OUT" if best_d[2] < MIN_EFFECT - EPS else "INCONCLUSIVE (best-case void imputation)"
    else:
        overall = "INCONCLUSIVE"
    r4 = lambda t: [round(v, 4) for v in t]  # noqa: E731 (display only; decisions use the unrounded values)
    return {"overall": overall, "classes": classes,
            "worst_case_void": {comp: dict(zip(("lift", "ci_low", "ci_high"), r4(v))) for comp, v in worst.items()},
            "best_case_void_vs_D": dict(zip(("lift", "ci_low", "ci_high"), r4(best_d))),
            "void_tuples": voids, "tuples": len(tuples), "void_share": round(void_share, 4),
            "tool_event_rates": {str(k): round(v, 5) for k, v in tool_rates.items()},
            "hidden_name_hits": len(summary.get("hidden_name_hits") or []), "controller_claim": summary.get("claim")}


# ------------------------------------------------------------------------------------ runner
# Generation (one replicate per call), manifests and scoring, under one exclusive lock. A gate
# that defers records the deferral and writes nothing else; the attempt record is reserved
# atomically before the replicate's first model call.
ROOT = pathlib.Path(__file__).resolve().parent.parent
HOLDOUT = pathlib.Path.home() / ".a2d-holdout"
SUITE = HOLDOUT / "confirmation-3"
DIR = ROOT / "runs" / "c2-confirm3"
A2D = str(ROOT / "target" / "debug" / "a2d")
LUNA = "codex/gpt-6-luna"
DAILY_CAP = 3000
EARLIEST = datetime.date(2026, 10, 24)
START = datetime.time(0, 5)                       # replicates start at or after 00:05 UTC
REGISTRATION = "runs/c2-confirm3/registration.json"
BOUND = (REGISTRATION, "runs/c2-confirm3/frozen.py", "nano/confirm3.py")
C2_SEALED = ROOT / "runs" / "c2-confirm2" / "sealed-sha256.txt"
LATENCY_PROBES = 3
LATENCY_MAX_MEDIAN_SECS = 45.0
PONG = "Reply with the single word PONG."
AMBIENT = (".codex", "AGENTS.md", "AGENTS.override.md", ".agents", "skills")
MACHO = (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe")   # 64-bit Mach-O, universal


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


def now():
    return datetime.datetime.now(datetime.timezone.utc)


def today():
    return now().date()


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


def registration_commit():
    """The one commit that added registration.json: the only valid --registered-at."""
    code, out = git("log", "--diff-filter=A", "--format=%H", "--", REGISTRATION)
    shas = out.split()
    if code or len(shas) != 1:
        sys.exit("registration.json must have been added by exactly one commit")
    return shas[0]


def registration(registered_at):
    """The registration commit binds registration.json, the frozen candidate and this file;
    the controller binds crates/ itself (--registered-at)."""
    sha = registration_commit()
    if git("rev-parse", "--verify", f"{registered_at}^{{commit}}")[1].strip() != sha:
        sys.exit("--registered-at is not the commit that added registration.json")
    if git("merge-base", "--is-ancestor", sha, "HEAD")[0]:
        sys.exit(f"{sha} is not on HEAD's history")
    for rel in BOUND:
        code, then = git("show", f"{sha}:./{rel}")
        if code or then != (ROOT / rel).read_text():
            sys.exit(f"{rel} differs from the registration commit")
    if git("merge-base", "--is-ancestor", sha, "origin/main")[0]:
        sys.exit("push the registration commit first")
    reg = json.loads((DIR / "registration.json").read_text())
    if sha256(DIR / "frozen.py") != reg["frozen_sha"]:
        sys.exit("the frozen candidate does not match registration.json")
    if git("merge-base", "--is-ancestor", reg["provider_commit"], sha)[0]:
        sys.exit("the registered provider commit is not in the registration's history")
    return reg


def check_attempts(sha):
    """Every attempt so far ran under this registration."""
    for p in sorted(DIR.glob("attempt-r*.json")):
        if json.loads(p.read_text()).get("registered_at") != sha:
            sys.exit(f"{p.name} ran under another registration commit")


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


def deferred_today(rep, day):
    """A cap or latency no-go moves the start to the next day's 00:05 UTC (§0.10)."""
    path = DIR / "deferrals.jsonl"
    return path.exists() and any(json.loads(x).get("rep") == rep and json.loads(x).get("utc_day") == str(day)
                                 for x in path.read_text().splitlines() if x.strip())


def record_deferral(rep, day, reason, **kw):
    with open(DIR / "deferrals.jsonl", "a") as f:
        f.write(json.dumps({"rep": rep, "utc_day": str(day), "reason": reason, **kw}) + "\n")


def latency_go(secs):
    """Pre-registered go/no-go: median of the probe calls (a failed probe is infinite)."""
    return statistics.median(secs) <= LATENCY_MAX_MEDIAN_SECS


def latency_probe(model, effort, n=LATENCY_PROBES):
    """n trivial calls of the registered model, from empty dirs with an auth-only CODEX_HOME."""
    secs = []
    auth = pathlib.Path(os.environ.get("CODEX_HOME", str(pathlib.Path.home() / ".codex"))) / "auth.json"
    for _ in range(n):
        with tempfile.TemporaryDirectory() as d:
            home = pathlib.Path(d) / "home"
            home.mkdir()
            (home / "auth.json").symlink_to(auth)
            start = time.monotonic()
            try:
                r = subprocess.run(["codex", "exec", "--json", "--skip-git-repo-check", "--ephemeral", "-m",
                                    model.split("/", 1)[1], "-c", f"model_reasoning_effort={effort}",
                                    "-c", 'web_search="disabled"', "-"],
                                   input=PONG, capture_output=True, text=True, cwd=d, timeout=300,
                                   env={**os.environ, "CODEX_HOME": str(home)})
                secs.append(time.monotonic() - start if r.returncode == 0 else float("inf"))
            except subprocess.TimeoutExpired:
                secs.append(float("inf"))
    return secs


def probes_today(day):
    path = DIR / "latency.jsonl"
    if not path.exists():
        return 0
    return sum(len(json.loads(x)["secs"]) for x in path.read_text().splitlines()
               if x.strip() and json.loads(x)["utc"][:10] == str(day))


def codex_fingerprint():
    """The native codex binary and its helpers (sha256 of every file beside it), or None."""
    exe = shutil.which("codex")
    if not exe:
        return None
    real = pathlib.Path(os.path.realpath(exe))
    with open(real, "rb") as f:
        if f.read(4) not in MACHO:
            return None                 # a script shim: the native binary is not beside it
    files = {p.name: sha256(p) for p in sorted(real.parent.iterdir()) if p.is_file()}
    version = subprocess.run([str(real), "--version"], capture_output=True, text=True).stdout.strip()
    return {"realpath": str(real), "files": files, "version": version}


def isolation_ok():
    """§0.10 launch isolation, the parts that apply to a no-tool replicate: no codex context
    (.codex, AGENTS.md, skills, ...) in any ancestor of the temp root where the provider and
    the latency probes create their cwd. The rest is by construction (provider: auth-only
    CODEX_HOME per call, tools off) and by the fingerprint."""
    tmp = pathlib.Path(tempfile.gettempdir()).resolve()
    return not any(os.path.lexists(d / name) for d in [tmp, *tmp.parents] for name in AMBIENT)


def drained():
    """§0.10 drain: the authoring pipeline's own fail-closed check."""
    r = subprocess.run([sys.executable, "-I", str(ROOT / "tools" / "holdout" / "c3_author.py"), "--drained"],
                       capture_output=True, text=True)
    lines = [json.loads(x) for x in r.stdout.splitlines() if x.strip().startswith("{")]
    return r.returncode == 0 and bool(lines) and lines[-1].get("ok") is True


def luna_today():
    """Luna invocations since 00:00 UTC across this cycle's runs (the same ledgers as eval.py),
    plus this runner's latency probes."""
    lo_ms = datetime.datetime.combine(today(), datetime.time(), datetime.timezone.utc).timestamp() * 1000
    seen, total = set(), probes_today(today())
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


class Lock:
    """One runner at a time (generate, manifest, score)."""

    def __enter__(self):
        DIR.mkdir(parents=True, exist_ok=True)
        self.f = open(DIR / "runner.lock", "w")
        try:
            fcntl.flock(self.f, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            sys.exit("another confirm3 runner holds the lock")
        return self

    def __exit__(self, *exc):
        fcntl.flock(self.f, fcntl.LOCK_UN)
        self.f.close()


def reserve(path, data):
    """Atomic attempt reservation: the file must not exist yet."""
    try:
        fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o644)
    except FileExistsError:
        sys.exit(f"{path.name} exists")
    with os.fdopen(fd, "w") as f:
        f.write(json.dumps(data) + "\n")
        f.flush()
        os.fsync(f.fileno())


def cmd_generate(args):
    """One replicate, attempted once (one logged relaunch with the same seed after a recorded
    terminal failure)."""
    with Lock():
        reg = registration(args.registered_at)
        sha = registration_commit()
        check_attempts(sha)
        rep, relaunch = args.rep, args.relaunch
        stage = f"confirm3-r{rep}"
        if complete(out_dir(rep)) or complete(out_dir(rep, True)):
            emit(stage=stage, decision="already generated")
            return
        out, attempt = out_dir(rep, relaunch), DIR / f"attempt-r{rep}{suffix(relaunch)}.json"
        first, failure = DIR / f"attempt-r{rep}.json", DIR / f"failed-r{rep}.json"
        if relaunch:
            if not first.exists():
                sys.exit("a relaunch needs a failed first attempt")
            if not failure.exists():
                # Under the lock no first-attempt runner is alive, and its generation never completed.
                failure.write_text(json.dumps({"rep": rep, "recorded_utc": now().isoformat(),
                                               "reason": "no generation_complete; no runner alive"}) + "\n")
        if attempt.exists() or out.exists():
            sys.exit(f"replicate {rep} was attempted without completing"
                     + ("; it is reported as failed" if relaunch else "; relaunch once with --relaunch, or report it as failed"))
        rep1 = next((o for o in (out_dir(1), out_dir(1, True)) if complete(o)), None)
        if rep == 2 and rep1 is None:
            sys.exit("replicate 1 has not generated")
        day = today()
        reason = spend_gate(day, commit_day(sha), C2_SEALED.exists(), rep, generation_day(rep1) if rep == 2 else None)
        if reason:
            emit(stage=stage, decision=f"deferred ({reason})")
            return
        if now().time() < START:
            emit(stage=stage, decision="deferred (before 00:05 UTC)")
            return
        if deferred_today(rep, day):
            emit(stage=stage, decision="deferred (a no-go today moves the start to the next 00:05 UTC)")
            return
        if not drained():
            sys.exit("the authoring pipeline is not drained (STOP, no driver, no job process)")
        if not isolation_ok():
            sys.exit("launch isolation failed (codex context in an ancestor of the temp root)")
        before = codex_fingerprint()
        if before is None or before["version"] != reg["codex_version"] or before["files"] != reg["codex_files"]:
            sys.exit("codex differs from the registered binary")
        worst = len(reg["tasks"]) * len(reg["arms"]) * reg["k"]
        spent = luna_today()
        if spent + worst + LATENCY_PROBES > DAILY_CAP:
            record_deferral(rep, day, "daily Luna cap", luna_today=spent)
            emit(stage=stage, decision="deferred (daily Luna cap)", luna_today=spent, stage_worst_case=worst, cap=DAILY_CAP)
            return
        secs = latency_probe(reg["model"], reg["effort"])
        probe = {"rep": rep, "utc": now().isoformat(),
                 "secs": [round(s, 1) if s != float("inf") else None for s in secs], "go": latency_go(secs)}
        with open(DIR / "latency.jsonl", "a") as f:
            f.write(json.dumps(probe) + "\n")
        if not probe["go"]:
            record_deferral(rep, day, "latency", secs=probe["secs"])
            emit(stage=stage, decision="deferred (latency)", secs=probe["secs"])
            return
        reserve(attempt, {"rep": rep, "relaunch": relaunch, "registered_at": sha, "pid": os.getpid(), "codex": before,
                          "controller_binary_sha256": sha256(A2D), "started_utc": now().isoformat()})
        cmd = [A2D, "lift", "run", "--experiment", f"C2-CONFIRM3-R{rep}", "--registered-at", sha,
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
            (DIR / f"failed-r{rep}{suffix(relaunch)}.json").write_text(
                json.dumps({"rep": rep, "exit": code, "recorded_utc": now().isoformat()}) + "\n")
            sys.exit(f"generation failed ({code}); " + ("reported as failed" if relaunch else "relaunch once with --relaunch"))
        emit(stage=stage, decision="generated (scoring deferred)")


def generated_out(rep):
    """(out dir, relaunch) of the completed generation of a replicate, or None."""
    for relaunch in (False, True):
        if complete(out_dir(rep, relaunch)):
            return out_dir(rep, relaunch), relaunch
    return None


def cmd_manifest(args):
    """Content-free anchors of both replicates, committed before scoring (both or none)."""
    with Lock():
        registration(args.registered_at)
        check_attempts(registration_commit())
        outs = {rep: generated_out(rep) for rep in (1, 2)}
        if None in outs.values():
            sys.exit("both replicates must have generated before any manifest is written")
        for rep in (1, 2):
            if (DIR / f"manifest-r{rep}.json").exists():
                sys.exit(f"manifest-r{rep}.json exists; anchors are written once")
        for rep, (out, _) in outs.items():
            r = subprocess.run([A2D, "lift", "manifest", str(out)], capture_output=True, text=True)
            if r.returncode:
                sys.exit(f"manifest of replicate {rep} failed ({r.returncode})")
            (DIR / f"manifest-r{rep}.json").write_text(r.stdout)
        emit(stage="confirm3-manifest", decision="written; commit and push them, then score")


def cmd_score(args):
    """Score each replicate once against its committed anchor, pool, and decide."""
    with Lock():
        reg = registration(args.registered_at)
        check_attempts(registration_commit())
        decision_path = DIR / "decision.json"
        if decision_path.exists():
            sys.exit("decision.json exists; the set is scored once")
        outs = {}
        for rep in (1, 2):
            rel = f"runs/c2-confirm3/manifest-r{rep}.json"
            if git("ls-files", "--error-unmatch", rel)[0] or git("diff", "--quiet", "HEAD", "--", rel)[0] \
                    or git("merge-base", "--is-ancestor", "HEAD", "origin/main")[0]:
                sys.exit(f"commit and push {rel} before scoring")
            outs[rep] = generated_out(rep)
            if outs[rep] is None:
                sys.exit(f"replicate {rep} has not generated")
        for rep, (out, _) in outs.items():
            if (out / "summary.json").exists():
                continue
            head = json.loads((DIR / f"manifest-r{rep}.json").read_text())["ledger_head"]
            with open(str(out) + ".score.log", "w") as log:
                code = subprocess.run([A2D, "lift", "score", "--out", str(out), "--suite", str(SUITE),
                                       "--expect-head", head, "--jobs", "6"], stdout=log,
                                      stderr=subprocess.STDOUT).returncode
            if code:
                sys.exit(f"scoring replicate {rep} failed ({code})")
        pooled = DIR / "pooled-summary.json"
        r = subprocess.run([A2D, "lift", "pool", "--out", str(pooled)]
                           + [str(out / "ledger.jsonl") for out, _ in outs.values()], capture_output=True, text=True)
        if r.returncode:
            sys.exit(f"pooling failed ({r.returncode})")
        tuples, stats = {}, {}
        for rep, (out, relaunch) in outs.items():
            t, stats[rep] = replicate_tuples(out, rep, reg["arms"], reg["tasks"])
            if not json.loads((DIR / f"bracket-r{rep}{suffix(relaunch)}.json").read_text())["match"]:
                for v in t.values():
                    v["void"] = True            # another codex ran during the replicate
            tuples.update(t)
        if len(tuples) != 2 * len(reg["arms"]) * len(reg["tasks"]):
            sys.exit("planned tuples do not match the registration")
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
