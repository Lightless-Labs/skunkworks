#!/usr/bin/env python3
"""Eval v2.2 for the nanoharness (A²D cycle 2): one keep/discard experiment.

    python3 -I nano/eval.py aa                  # 3-arm A/A + parity run; writes nano/aa.json
    python3 -I nano/eval.py aa-from-replicates --id E002b   # A/A of a promoted keep from its own two full replicates
    python3 -I nano/eval.py look --n 1 --registered-at <sha>  # dev-holdout look: incumbent vs D, r = 2, aggregates only
    python3 -I nano/eval.py run --id E002 --candidate <file.py> --hypothesis "..." [--proposer claude]

Protocol (docs/plans/2026-10-15-cycle2-nanoharness.md, Addendum after the design review):
- incumbent = nano/harness.py. Candidate and incumbent run in ONE `a2d lift`
  run as external arms X:inc and X:cand, paired by task (k = 5). Model lanes a
  harness needs are declared in a `# lanes: name=codex/<model>:<effort>`
  header line and passed to the controller.
- Every experiment is immutable: runs/c2-<id>/manifest.json records the code,
  controller and suite hashes; cached runs are reused only if it matches.
  The evaluated snapshot (not the mutable source path) is what is promoted.
- Screen on a half of the dev families, chosen by a seed derived from the
  experiment id. A screen delta below half the threshold is a discard.
- Full stage: run the other half and pool. A provisional keep needs
  delta > max(threshold, 1.645 * cluster SE), and the all-pass delta must
  not be significantly negative (> -1.645 * its cluster SE).
- Confirmation: a fresh full run (both halves, new seed). Promotion needs its
  delta > threshold / 2. Keeps are exploratory; claims come only from the
  dev-holdout and confirmation sets.
- Every scored run must have exactly one score per expected task for each
  arm; otherwise the run is invalid (operational failures are never
  silently dropped).
- v2.2 (LOG "metric amendment", 2026-10-16): the PRIMARY keep metric is the
  all-pass difference (working software), with its own A/A threshold
  (`threshold_allpass` in nano/aa.json); hidden-test fraction is reported as
  secondary. The proxy disagreed with all-pass on the first selection-type
  candidate (E002 screen).
- threshold = 95th percentile of the family-bootstrap null from the A/A run
  (centred per-task differences, one draw per task as in candidate runs).
  The A/A run is a 3-arm run (D, X:a, X:b) so it also checks parity with
  native D. It must match the current incumbent's hash.
- v2.3 (LOG "A/A for a promoted keep", 2026-10-16): a promoted candidate has
  two independent full runs on the dev set (screen + rest, then confirm).
  Pairing them by task is an incumbent-vs-itself run on the same protocol at
  no cost; its 95th percentile is the new incumbent's threshold.
- Dev-holdout looks (`look`): incumbent vs D on ~/.a2d-holdout/dev-holdout,
  r = 2, registered at a commit. Only the controller's aggregate
  summary.json is read; gate = all-pass lift >= +10 pp with CI low > 0.
  Only the gate outcome (pass/fail) and spend are printed during the cycle.
"""
import argparse, datetime, hashlib, json, os, pathlib, random, re, shutil, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
NANO = ROOT / "nano"
DEV = ROOT / "tasks/train-all"
A2D = str(ROOT / "target/debug/a2d")
PROTOCOL = "eval-v2.3"
HOLDOUT = pathlib.Path.home() / ".a2d-holdout"
DEV_HOLDOUT_HASH = "9114bae3b071c52b2e9e257bee26eb7bc0df4bec05b479f390f973f9db2072cb"
GATE_LIFT = 0.10
FIELDS = ("id", "date", "proposer", "stage", "hypothesis", "delta_frac", "se", "delta_allpass",
          "se_allpass", "threshold", "n_tasks", "n_families", "decision", "invocations", "calls_by_lane",
          "tokens", "candidate_sha", "note")


ALLOWED_IMPORTS = {"json", "re", "sys", "random", "math", "itertools", "collections", "functools",
                   "hashlib", "difflib", "textwrap", "string", "heapq", "dataclasses", "typing"}
FORBIDDEN_CALLS = {"open", "exec", "eval", "compile", "__import__", "globals", "locals", "vars",
                   "getattr", "setattr", "delattr", "input", "breakpoint"}
# Attribute routes from allowlisted modules to the filesystem or other modules
# (sys.modules["os"], random._os, json.codecs.open, dataclasses.inspect.os...).
# Defence in depth only: the boundary is the controller's OS sandbox.
FORBIDDEN_ATTRS = {"modules", "os", "io", "open", "system", "popen", "codecs", "inspect", "importlib",
                   "builtins", "sys", "subprocess", "socket", "shutil", "pathlib", "environ", "listdir",
                   "scandir", "walk", "remove", "unlink", "rmdir", "fdopen", "read_text", "read_bytes",
                   "write_text", "write_bytes", "f_globals", "f_locals", "f_back", "f_code", "gi_frame",
                   "cr_frame", "tb_frame", "co_code", "loader", "spec"}
SYS_ATTRS = {"stdin", "stdout", "stderr", "exit"}


def guard(path):
    """Static isolation check for a harness file: only allowlisted stdlib
    imports and no file, process, network or dynamic-execution calls. Hidden
    tests and holdouts are on this machine; a harness must not reach them."""
    import ast
    tree = ast.parse(pathlib.Path(path).read_text())
    problems = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            problems += [a.name for a in node.names if a.name.split(".")[0] not in ALLOWED_IMPORTS]
        elif isinstance(node, ast.ImportFrom):
            if (node.module or "").split(".")[0] not in ALLOWED_IMPORTS or node.level:
                problems.append(f"from {node.module}")
        elif isinstance(node, ast.Name) and node.id in FORBIDDEN_CALLS:
            problems.append(f"name {node.id}")
        elif isinstance(node, ast.Name) and node.id.startswith("__") and node.id != "__name__":
            problems.append(f"dunder name {node.id}")
        elif isinstance(node, ast.Attribute) and node.attr.startswith("_") and node.attr != "__name__":
            problems.append(f"private attribute {node.attr}")
        elif isinstance(node, ast.Attribute) and node.attr in FORBIDDEN_ATTRS:
            problems.append(f"attribute {node.attr}")
        elif isinstance(node, ast.Attribute) and isinstance(node.value, ast.Name) and node.value.id == "sys" \
                and node.attr not in SYS_ATTRS:
            problems.append(f"sys.{node.attr}")
    if problems:
        sys.exit(f"{path}: harness isolation guard failed: {sorted(set(problems))}")


def sha(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def git_head():
    return subprocess.run(["git", "-C", str(ROOT), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()


def suite_hash(suite):
    return subprocess.run([A2D, "lift", "hash", str(suite)], capture_output=True, text=True).stdout.strip()


def families(suite):
    fams = {}
    for t in sorted(p for p in suite.iterdir() if (p / "task.json").exists()):
        fams.setdefault(json.loads((t / "task.json").read_text())["family"], []).append(t.name)
    return fams


def lanes_of(*files):
    lanes = {}
    for f in files:
        for m in re.finditer(r"^# lanes:\s*(.+)$", pathlib.Path(f).read_text(), re.M):
            for spec in m.group(1).split():
                name, value = spec.split("=", 1)
                lanes[name] = value
    return lanes


def make_suite(d, fam_names):
    if d.exists():
        return d
    d.mkdir(parents=True)
    fams = families(DEV)
    for f in fam_names:
        for task in fams[f]:
            (d / task).symlink_to(os.path.relpath((DEV / task).resolve(), d))
    return d


def lift(exp, suite, arms, externals, lanes, out, seed, jobs):
    cmd = [A2D, "lift", "run", "--experiment", exp, "--unregistered", "--suite", str(suite),
           "--model", "codex/gpt-6-luna", "--effort", "low", "--arms", ",".join(arms), "--k", "5",
           "--seed", str(seed), "--jobs", str(jobs), "--min-families", "1",
           "--max-invocations", "2000", "--max-wall-clock-secs", "21600", "--out", str(out)]
    if "D" in arms:
        cmd += ["--primary", "D"]
    for name, path in externals.items():
        cmd += ["--external", f"{name}={path}"]
    for name, spec in lanes.items():
        cmd += ["--lane", f"{name}={spec}"]
    with open(str(out) + ".log", "w") as log:
        code = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT).returncode
    if code or not (out / "summary.json").exists():
        sys.exit(f"lift run failed ({code}); see {out}.log")


def scores(run, suite):
    """{(arm, task): (fraction, all_passed, family)}; validates completeness."""
    expected = {p.name for p in suite.iterdir() if (p / "task.json").exists()}
    out, arms = {}, set()
    for line in (run / "ledger.jsonl").read_text().splitlines():
        r = json.loads(line)
        if r["kind"] == "score":
            key = (r["arm"], r["task"])
            if key in out:
                sys.exit(f"{run}: duplicate score for {key}")
            res = r["result"]
            exp = res.get("expected") or 0
            frac = res["passed"] / exp if exp and res.get("compiled") else 0.0
            out[key] = (frac, 1.0 if res["all_passed"] else 0.0, r["family"])
            arms.add(r["arm"])
    for arm in arms:
        got = {t for (a, t) in out if a == arm}
        if got != expected:
            sys.exit(f"{run}: arm {arm} scored {len(got)} of {len(expected)} expected tasks; run invalid")
    return out


def spend(run):
    """Invocations, calls by provider, and tokens by arm (from the ledger)."""
    calls, tokens, total = {}, 0, 0
    for line in (run / "ledger.jsonl").read_text().splitlines():
        r = json.loads(line)
        if r["kind"] == "invocation":
            total += 1
            key = f'{r.get("arm")}/{r.get("provider", "?")}'
            calls[key] = calls.get(key, 0) + 1
            usage = r.get("usage") or {}
            tokens += usage.get("reported_total") or sum(
                usage.get(k, 0) or 0 for k in ("prompt_tokens", "completion_tokens", "thinking_tokens"))
    return total, calls, tokens


def paired(runs_suites, x, y):
    rows = []
    for run, suite in runs_suites:
        s = scores(run, suite)
        for (arm, task), (fx, ax, fam) in s.items():
            if arm == x:
                fy, ay, _ = s[(y, task)]
                rows.append((fam, fx - fy, ax - ay))
    return rows


def cluster_se(rows, idx, reps=2000, seed=7):
    by = {}
    for r in rows:
        by.setdefault(r[0], []).append(r[idx])
    fams = list(by)
    rng = random.Random(seed)
    means = []
    for _ in range(reps):
        vals = [v for f in (rng.choice(fams) for _ in fams) for v in by[f]]
        means.append(sum(vals) / len(vals))
    mu = sum(means) / len(means)
    return (sum((m - mu) ** 2 for m in means) / (len(means) - 1)) ** 0.5


def aa_threshold(rows, reps=4000, seed=11, idx=1):
    mean = sum(r[idx] for r in rows) / len(rows)
    by = {}
    for r in rows:
        by.setdefault(r[0], []).append(r[idx] - mean)
    fams = list(by)
    rng = random.Random(seed)
    means = sorted(
        (lambda vals: sum(vals) / len(vals))([v for f in (rng.choice(fams) for _ in fams) for v in by[f]])
        for _ in range(reps))
    return means[int(0.95 * (reps - 1))]


LUNA = "codex/gpt-6-luna"
DAILY_CAP = 3000


def luna_today():
    """Luna invocations since 00:00 UTC today across this cycle's runs (dev and
    holdout ledgers; invocation records only, no content is read)."""
    lo = datetime.datetime.now(datetime.timezone.utc).replace(hour=0, minute=0, second=0, microsecond=0)
    lo_ms = lo.timestamp() * 1000
    seen, total = set(), 0
    ledgers = list((ROOT / "runs").glob("c2-*/*/ledger.jsonl")) + list((ROOT / "runs").glob("c2-*/ledger.jsonl")) \
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


def within_cap(stage, n_calls_worst, cap):
    spent = luna_today()
    if spent + n_calls_worst > cap:
        print(json.dumps({"stage": stage, "decision": "deferred (daily Luna cap)", "luna_today": spent,
                          "stage_worst_case": n_calls_worst, "cap": cap}, indent=1))
        return False
    return True


def mean(rows, idx):
    return sum(r[idx] for r in rows) / len(rows)


def log_result(fields):
    path = NANO / "results.tsv"
    if not path.exists() or not path.read_text().startswith("id\t"):
        path.write_text("\t".join(FIELDS) + "\n")
    with open(path, "a") as f:
        f.write("\t".join(str(fields.get(k, "")).replace("\t", " ") for k in FIELDS) + "\n")


def manifest(exp_dir, data):
    path = exp_dir / "manifest.json"
    if path.exists():
        old = json.loads(path.read_text())
        if old != data:
            sys.exit(f"{exp_dir}: manifest mismatch (ids are immutable); differing keys: "
                     f"{[k for k in data if old.get(k) != data.get(k)]}")
    else:
        path.write_text(json.dumps(data, indent=1) + "\n")


def cmd_aa(args):
    inc = NANO / "harness.py"
    exp_dir = ROOT / "runs" / f"c2-aa-{sha(inc)[:12]}"
    exp_dir.mkdir(parents=True, exist_ok=True)
    snap = exp_dir / "harness.py"
    if not snap.exists():
        shutil.copy(inc, snap)
    guard(snap)
    lanes = lanes_of(snap)
    manifest(exp_dir, {"protocol": PROTOCOL, "incumbent_sha": sha(snap), "controller": git_head(),
                       "suite_hash": suite_hash(DEV), "lanes": lanes})
    out = exp_dir / "run"
    if not (out / "summary.json").exists():
        lift("C2-AA", DEV, ["D", "X:a", "X:b"], {"a": snap, "b": snap}, lanes, out, 4242, args.jobs)
    rows = paired([(out, DEV)], "X:b", "X:a")
    thr = aa_threshold(rows)
    thr_all = aa_threshold(rows, idx=2)
    parity = {f"{x} - D": {"delta_frac": mean(r, 1), "delta_allpass": mean(r, 2), "se_frac": cluster_se(r, 1)}
              for x in ("X:a", "X:b") for r in [paired([(out, DEV)], x, "D")]}
    total, calls, tokens = spend(out)
    result = {"protocol": PROTOCOL, "incumbent_sha": sha(snap), "n": len(rows),
              "aa_mean_delta_frac": mean(rows, 1), "aa_mean_delta_allpass": mean(rows, 2),
              "threshold_frac": thr, "threshold_allpass": thr_all,
              "threshold_allpass_net_tasks": thr_all * len(rows), "parity_vs_D": parity, "run": str(out.relative_to(ROOT)),
              "invocations": total, "calls": calls, "tokens": tokens}
    (NANO / "aa.json").write_text(json.dumps(result, indent=1) + "\n")
    print(json.dumps(result, indent=1))


def cmd_run(args):
    inc_src = NANO / "harness.py"
    aa_path = NANO / "aa.json"
    aa = json.loads(aa_path.read_text()) if aa_path.exists() else None
    if aa is not None and aa["incumbent_sha"] != sha(inc_src):
        sys.exit("nano/aa.json does not match the current incumbent; re-run `eval.py aa` first")
    # Without a threshold yet, only the screen stage is generated (and cached);
    # the decision is made on a later invocation once nano/aa.json exists.
    if aa is not None and "threshold_allpass" not in aa:
        sys.exit("nano/aa.json lacks threshold_allpass; run `eval.py aa-allpass` (v2.2)")
    thr = aa["threshold_allpass"] if aa is not None else None
    thr_frac = aa["threshold_frac"] if aa is not None else None
    exp_dir = ROOT / "runs" / f"c2-{args.id}"
    exp_dir.mkdir(parents=True, exist_ok=True)
    inc, cand = exp_dir / "incumbent.py", exp_dir / "candidate.py"
    if not cand.exists():
        shutil.copy(inc_src, inc)
        shutil.copy(args.candidate, cand)
        (exp_dir / "hypothesis.txt").write_text(args.hypothesis + "\n")
    guard(inc)
    guard(cand)
    lanes = lanes_of(inc, cand)
    manifest(exp_dir, {"protocol": PROTOCOL, "id": args.id, "incumbent_sha": sha(inc), "candidate_sha": sha(cand),
                       "suite_hash": suite_hash(DEV), "lanes": lanes,
                       "hypothesis": (exp_dir / "hypothesis.txt").read_text().strip()})
    if aa is not None and sha(inc) != aa["incumbent_sha"]:
        sys.exit("the snapshot incumbent differs from the A/A incumbent")
    if args.reuse_screen_from:
        # Re-registration of the same candidate under an amended rule: reuse the
        # already observed screen stage (same halves), run rest/confirm fresh.
        src = ROOT / "runs" / f"c2-{args.reuse_screen_from}"
        if sha(src / "candidate.py") != sha(cand) or sha(src / "incumbent.py") != sha(inc):
            sys.exit("--reuse-screen-from needs the same candidate and incumbent")
        halves = json.loads((src / "halves.json").read_text())
        if not (exp_dir / "screen").exists():
            shutil.copytree(src / "screen", exp_dir / "screen", symlinks=True)
            shutil.copy(str(src / "screen") + ".log", str(exp_dir / "screen") + ".log")
    else:
        fams = sorted(families(DEV))
        rng = random.Random(f"A2D-C2-SCREEN-{args.id}")
        rng.shuffle(fams)
        halves = {"S": sorted(fams[: len(fams) // 2]), "R": sorted(fams[len(fams) // 2:])}
    (exp_dir / "halves.json").write_text(json.dumps(halves, indent=1) + "\n")
    seed = int(hashlib.sha256(args.id.encode()).hexdigest()[:6], 16)
    ext = {"inc": inc, "cand": cand}
    arms = ["X:inc", "X:cand"]
    common = {"id": args.id, "date": datetime.date.today().isoformat(), "proposer": args.proposer,
              "hypothesis": (exp_dir / "hypothesis.txt").read_text().strip(),
              "threshold": (f"allpass {thr:.4f} / frac {thr_frac:.4f}" if thr is not None else ""),
              "candidate_sha": sha(cand)[:12]}

    def stage(name, runs_suites, decision, note=""):
        rows = paired(runs_suites, "X:cand", "X:inc")
        spent = [spend(r) for r, _ in runs_suites]
        calls = {}
        for _, c, _ in spent:
            for k, v in c.items():
                calls[k] = calls.get(k, 0) + v
        rec = {**common, "stage": name, "delta_frac": f"{mean(rows, 1):+.4f}", "se": f"{cluster_se(rows, 1):.4f}",
               "delta_allpass": f"{mean(rows, 2):+.4f}", "se_allpass": f"{cluster_se(rows, 2):.4f}",
               "n_tasks": len(rows), "n_families": len({r[0] for r in rows}), "decision": decision,
               "invocations": sum(t for t, _, _ in spent), "calls_by_lane": json.dumps(calls, sort_keys=True),
               "tokens": sum(k for _, _, k in spent), "note": note}
        return rows, rec

    s_suite = make_suite(exp_dir / "suites/S", halves["S"])
    r_suite = make_suite(exp_dir / "suites/R", halves["R"])
    worst = 5 + 5  # Luna calls per task, both arms, if no call went to a lane
    screen = exp_dir / "screen"
    if not (screen / "summary.json").exists():
        if not within_cap("screen", worst * len(list(s_suite.iterdir())), args.daily_cap):
            return
        lift(f"C2-{args.id}-S", s_suite, arms, ext, lanes, screen, seed, args.jobs)
    rows, rec = stage("screen", [(screen, s_suite)], "")
    if thr is None:
        print(json.dumps({**rec, "decision": "pending (no A/A threshold yet)"}, indent=1))
        return
    if mean(rows, 2) < thr / 2:
        rec["decision"] = "discard (screen)"
        log_result(rec)
        print(json.dumps(rec, indent=1))
        return
    rest = exp_dir / "rest"
    if not (rest / "summary.json").exists():
        if not within_cap("rest", worst * len(list(r_suite.iterdir())), args.daily_cap):
            return
        lift(f"C2-{args.id}-R", r_suite, arms, ext, lanes, rest, seed + 1, args.jobs)
    rows, rec = stage("full", [(screen, s_suite), (rest, r_suite)], "")
    se, se_all = cluster_se(rows, 1), cluster_se(rows, 2)
    # Primary: all-pass. Guard: the fraction delta must not be significantly negative.
    provisional = mean(rows, 2) > max(thr, 1.645 * se_all) and mean(rows, 1) > -1.645 * se
    if not provisional:
        rec["decision"] = "discard (full)"
        log_result(rec)
        print(json.dumps(rec, indent=1))
        return
    rec["decision"] = "provisional keep"
    confirm = exp_dir / "confirm"
    if not (confirm / "summary.json").exists():
        log_result(rec)
    if args.stop_after == "full" and not (confirm / "summary.json").exists():
        # Budget pacing: the confirmation replicate runs on a later invocation.
        print(json.dumps({**rec, "decision": "provisional keep (confirmation deferred)"}, indent=1))
        return
    if not (confirm / "summary.json").exists():
        if not within_cap("confirm", worst * len(list(DEV.iterdir())), args.daily_cap):
            return
        lift(f"C2-{args.id}-C", DEV, arms, ext, lanes, confirm, seed + 2, args.jobs)
    rows_c, rec_c = stage("confirm", [(confirm, DEV)], "")
    keep = mean(rows_c, 2) > thr / 2
    rec_c["decision"] = "keep" if keep else "discard (confirm)"
    log_result(rec_c)
    if keep:
        hist = NANO / "history"
        hist.mkdir(exist_ok=True)
        shutil.copy(inc, hist / f"before-{args.id}.py")
        shutil.copy(cand, inc_src)
    print(json.dumps(rec_c, indent=1))


def cmd_aa_from_replicates(args):
    exp_dir = ROOT / "runs" / f"c2-{args.id}"
    cand = exp_dir / "candidate.py"
    if sha(cand) != sha(NANO / "harness.py"):
        sys.exit(f"{cand} is not the current incumbent")
    by_run = []
    for runs_suites in ([(exp_dir / "screen", exp_dir / "suites/S"), (exp_dir / "rest", exp_dir / "suites/R")],
                        [(exp_dir / "confirm", DEV)]):
        got = {}
        for run, suite in runs_suites:
            got.update({t: v for (a, t), v in scores(run, suite).items() if a == "X:cand"})
        by_run.append(got)
    first, second = by_run
    if set(first) != set(second) or set(first) != {t for f in families(DEV).values() for t in f}:
        sys.exit("the two replicates do not cover the same dev tasks")
    rows = [(second[t][2], second[t][0] - first[t][0], second[t][1] - first[t][1]) for t in sorted(first)]
    old_path = NANO / "aa.json"
    if old_path.exists():
        old = json.loads(old_path.read_text())
        hist = NANO / "history"
        hist.mkdir(exist_ok=True)
        shutil.copy(old_path, hist / f"aa-{old['incumbent_sha'][:12]}.json")
    result = {"protocol": PROTOCOL, "method": "replicates of the promoted keep (full stage vs confirm), paired by task",
              "incumbent_sha": sha(cand), "source": str(exp_dir.relative_to(ROOT)), "n": len(rows),
              "aa_mean_delta_frac": mean(rows, 1), "aa_mean_delta_allpass": mean(rows, 2),
              "discordant_allpass": sum(1 for r in rows if r[2] != 0),
              "threshold_frac": aa_threshold(rows), "threshold_allpass": aa_threshold(rows, idx=2)}
    result["threshold_allpass_net_tasks"] = result["threshold_allpass"] * len(rows)
    old_path.write_text(json.dumps(result, indent=1) + "\n")
    print(json.dumps(result, indent=1))


def look_report(summary, treatment, comparator="D"):
    """Aggregate-only view of a dev-holdout look: rates, lift, CI, counts.
    No per-task or per-family field is read or printed."""
    comps = [c for c in summary["comparisons"] if c["treatment"] == treatment and c["comparator"] == comparator]
    if len(comps) != 1:
        sys.exit(f"expected one {treatment} vs {comparator} comparison, found {len(comps)}")
    c = comps[0]
    arms = {a["arm"]: {k: a[k] for k in ("n", "passed", "rate", "invocations", "tokens")} for a in summary["arms"]}
    valid = bool(summary["complete"]) and summary["tuples_void"] == 0 and bool(summary["integrity_ok"]) \
        and not summary["aborted"]
    gate = valid and c["lift"] >= GATE_LIFT - 1e-9 and c["ci95"][0] > 0
    return {"valid": valid, "complete": summary["complete"], "tuples_void": summary["tuples_void"],
            "integrity_ok": summary["integrity_ok"], "n": c["n"], "families": c["families"],
            "treatment_rate": c["treatment_rate"], "comparator_rate": c["comparator_rate"],
            "lift_allpass": c["lift"], "ci95": c["ci95"], "hidden_fraction_lift": c["hidden_fraction_lift"],
            "arms": arms, "gate": "PASS" if gate else "FAIL"}


def cmd_look(args):
    out = HOLDOUT / "runs" / f"c2-look{args.n}"
    snap_dir = ROOT / "runs" / f"c2-look{args.n}"
    snap_dir.mkdir(parents=True, exist_ok=True)
    snap = snap_dir / "harness.py"
    if not snap.exists():
        shutil.copy(NANO / "harness.py", snap)
    guard(snap)
    manifest(snap_dir, {"protocol": PROTOCOL, "look": args.n, "incumbent_sha": sha(snap),
                        "registered_at": args.registered_at, "suite_hash": DEV_HOLDOUT_HASH,
                        "seed": args.seed, "replicates": 2, "arms": ["D", "X:inc"]})
    if not (out / "summary.json").exists():
        if not within_cap("look", 100 * 2 * (5 + 5), args.daily_cap):
            return
        cmd = [A2D, "lift", "run", "--experiment", f"C2-LOOK{args.n}", "--registered-at", args.registered_at,
               "--expect-suite-hash", DEV_HOLDOUT_HASH, "--suite", str(HOLDOUT / "dev-holdout"),
               "--model", "codex/gpt-6-luna", "--effort", "low", "--arms", "D,X:inc", "--k", "5",
               "--replicates", "2", "--primary", "D", "--seed", str(args.seed), "--jobs", str(args.jobs),
               "--min-families", "1", "--max-invocations", "3000", "--max-wall-clock-secs", "36000",
               "--out", str(out), "--external", f"inc={snap}"]
        for name, spec in lanes_of(snap).items():
            cmd += ["--lane", f"{name}={spec}"]
        out.parent.mkdir(parents=True, exist_ok=True)
        with open(str(out) + ".log", "w") as log:
            code = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT).returncode
        if code or not (out / "summary.json").exists():
            sys.exit(f"look run failed ({code}); see the exit line of {out}.log")
    # Proposers learn only pass or fail: the full aggregate report stays next
    # to the holdout run until the cycle's final report; the repo and stdout
    # get the gate outcome and spend only.
    report = look_report(json.loads((out / "summary.json").read_text()), "X:inc")
    (out / "report.json").write_text(json.dumps(report, indent=1) + "\n")
    public = {"look": args.n, "valid": report["valid"], "gate": report["gate"], "n": report["n"],
              "invocations": {a: v["invocations"] for a, v in report["arms"].items()}}
    (snap_dir / "gate.json").write_text(json.dumps(public, indent=1) + "\n")
    print(json.dumps(public, indent=1))


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--id", required=True)
    r.add_argument("--candidate", required=True)
    r.add_argument("--hypothesis", required=True)
    r.add_argument("--proposer", default="claude")
    r.add_argument("--jobs", type=int, default=4)
    r.add_argument("--reuse-screen-from", default=None)
    r.add_argument("--stop-after", choices=["full"], default=None)
    r.add_argument("--daily-cap", type=int, default=DAILY_CAP)
    a = sub.add_parser("aa")
    sub.add_parser("aa-allpass")
    a.add_argument("--jobs", type=int, default=4)
    ar = sub.add_parser("aa-from-replicates")
    ar.add_argument("--id", required=True)
    lk = sub.add_parser("look")
    lk.add_argument("--n", type=int, required=True)
    lk.add_argument("--registered-at", required=True)
    lk.add_argument("--seed", type=int, default=6161)
    lk.add_argument("--jobs", type=int, default=8)
    lk.add_argument("--daily-cap", type=int, default=DAILY_CAP)
    sub.add_parser("budget")
    args = ap.parse_args()
    if args.cmd == "aa-allpass":
        # v2.2: add the all-pass threshold from the existing A/A run (no new calls).
        aa = json.loads((NANO / "aa.json").read_text())
        rows = paired([(ROOT / aa["run"], DEV)], "X:b", "X:a")
        aa["threshold_allpass"] = aa_threshold(rows, idx=2)
        aa["threshold_allpass_net_tasks"] = aa["threshold_allpass"] * len(rows)
        aa["protocol"] = PROTOCOL
        (NANO / "aa.json").write_text(json.dumps(aa, indent=1) + "\n")
        print(json.dumps({k: aa[k] for k in ("threshold_frac", "threshold_allpass", "threshold_allpass_net_tasks")}, indent=1))
    elif args.cmd == "aa":
        cmd_aa(args)
    elif args.cmd == "aa-from-replicates":
        cmd_aa_from_replicates(args)
    elif args.cmd == "look":
        cmd_look(args)
    elif args.cmd == "budget":
        print(json.dumps({"luna_today_utc": luna_today(), "cap": DAILY_CAP}))
    else:
        cmd_run(args)


if __name__ == "__main__":
    main()
