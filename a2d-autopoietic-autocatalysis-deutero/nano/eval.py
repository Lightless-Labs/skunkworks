#!/usr/bin/env python3
"""Eval v2.1 for the nanoharness (A²D cycle 2): one keep/discard experiment.

    python3 -I nano/eval.py aa                  # 3-arm A/A + parity run; writes nano/aa.json
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
- threshold = 95th percentile of the family-bootstrap null from the A/A run
  (centred per-task differences, one draw per task as in candidate runs).
  The A/A run is a 3-arm run (D, X:a, X:b) so it also checks parity with
  native D. It must match the current incumbent's hash.
"""
import argparse, datetime, hashlib, json, os, pathlib, random, re, shutil, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
NANO = ROOT / "nano"
DEV = ROOT / "tasks/train-all"
A2D = str(ROOT / "target/debug/a2d")
PROTOCOL = "eval-v2.1"
FIELDS = ("id", "date", "proposer", "stage", "hypothesis", "delta_frac", "se", "delta_allpass",
          "se_allpass", "threshold", "n_tasks", "n_families", "decision", "invocations", "calls_by_lane",
          "tokens", "candidate_sha", "note")


ALLOWED_IMPORTS = {"json", "re", "sys", "random", "math", "itertools", "collections", "functools",
                   "hashlib", "difflib", "textwrap", "string", "heapq", "dataclasses", "typing"}
FORBIDDEN_CALLS = {"open", "exec", "eval", "compile", "__import__", "globals", "locals", "vars",
                   "getattr", "setattr", "delattr", "input", "breakpoint"}


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
        elif isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id in FORBIDDEN_CALLS:
            problems.append(f"call {node.func.id}()")
        elif isinstance(node, ast.Attribute) and node.attr.startswith("__") and node.attr not in ("__name__",):
            problems.append(f"dunder attribute {node.attr}")
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


def aa_threshold(rows, reps=4000, seed=11):
    mean = sum(r[1] for r in rows) / len(rows)
    by = {}
    for fam, d, _ in rows:
        by.setdefault(fam, []).append(d - mean)
    fams = list(by)
    rng = random.Random(seed)
    means = sorted(
        (lambda vals: sum(vals) / len(vals))([v for f in (rng.choice(fams) for _ in fams) for v in by[f]])
        for _ in range(reps))
    return means[int(0.95 * (reps - 1))]


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
    parity = {f"{x} - D": {"delta_frac": mean(r, 1), "delta_allpass": mean(r, 2), "se_frac": cluster_se(r, 1)}
              for x in ("X:a", "X:b") for r in [paired([(out, DEV)], x, "D")]}
    total, calls, tokens = spend(out)
    result = {"protocol": PROTOCOL, "incumbent_sha": sha(snap), "n": len(rows),
              "aa_mean_delta_frac": mean(rows, 1), "aa_mean_delta_allpass": mean(rows, 2),
              "threshold_frac": thr, "parity_vs_D": parity, "run": str(out.relative_to(ROOT)),
              "invocations": total, "calls": calls, "tokens": tokens}
    (NANO / "aa.json").write_text(json.dumps(result, indent=1) + "\n")
    print(json.dumps(result, indent=1))


def cmd_run(args):
    aa = json.loads((NANO / "aa.json").read_text())
    inc_src = NANO / "harness.py"
    if aa["incumbent_sha"] != sha(inc_src):
        sys.exit("nano/aa.json does not match the current incumbent; re-run `eval.py aa` first")
    thr = aa["threshold_frac"]
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
                       "aa_threshold": thr, "suite_hash": suite_hash(DEV), "lanes": lanes,
                       "hypothesis": (exp_dir / "hypothesis.txt").read_text().strip()})
    if sha(inc) != aa["incumbent_sha"]:
        sys.exit("the snapshot incumbent differs from the A/A incumbent")
    fams = sorted(families(DEV))
    rng = random.Random(f"A2D-C2-SCREEN-{args.id}")
    rng.shuffle(fams)
    halves = {"S": sorted(fams[: len(fams) // 2]), "R": sorted(fams[len(fams) // 2:])}
    (exp_dir / "halves.json").write_text(json.dumps(halves, indent=1) + "\n")
    seed = int(hashlib.sha256(args.id.encode()).hexdigest()[:6], 16)
    ext = {"inc": inc, "cand": cand}
    arms = ["X:inc", "X:cand"]
    common = {"id": args.id, "date": datetime.date.today().isoformat(), "proposer": args.proposer,
              "hypothesis": (exp_dir / "hypothesis.txt").read_text().strip(), "threshold": f"{thr:.4f}",
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
    screen = exp_dir / "screen"
    if not (screen / "summary.json").exists():
        lift(f"C2-{args.id}-S", s_suite, arms, ext, lanes, screen, seed, args.jobs)
    rows, rec = stage("screen", [(screen, s_suite)], "")
    if mean(rows, 1) < thr / 2:
        rec["decision"] = "discard (screen)"
        log_result(rec)
        print(json.dumps(rec, indent=1))
        return
    rest = exp_dir / "rest"
    if not (rest / "summary.json").exists():
        lift(f"C2-{args.id}-R", r_suite, arms, ext, lanes, rest, seed + 1, args.jobs)
    rows, rec = stage("full", [(screen, s_suite), (rest, r_suite)], "")
    se, se_all = cluster_se(rows, 1), cluster_se(rows, 2)
    provisional = mean(rows, 1) > max(thr, 1.645 * se) and mean(rows, 2) > -1.645 * se_all
    if not provisional:
        rec["decision"] = "discard (full)"
        log_result(rec)
        print(json.dumps(rec, indent=1))
        return
    rec["decision"] = "provisional keep"
    log_result(rec)
    confirm = exp_dir / "confirm"
    if not (confirm / "summary.json").exists():
        lift(f"C2-{args.id}-C", DEV, arms, ext, lanes, confirm, seed + 2, args.jobs)
    rows_c, rec_c = stage("confirm", [(confirm, DEV)], "")
    keep = mean(rows_c, 1) > thr / 2
    rec_c["decision"] = "keep" if keep else "discard (confirm)"
    log_result(rec_c)
    if keep:
        hist = NANO / "history"
        hist.mkdir(exist_ok=True)
        shutil.copy(inc, hist / f"before-{args.id}.py")
        shutil.copy(cand, inc_src)
    print(json.dumps(rec_c, indent=1))


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--id", required=True)
    r.add_argument("--candidate", required=True)
    r.add_argument("--hypothesis", required=True)
    r.add_argument("--proposer", default="claude")
    r.add_argument("--jobs", type=int, default=4)
    a = sub.add_parser("aa")
    a.add_argument("--jobs", type=int, default=4)
    args = ap.parse_args()
    cmd_aa(args) if args.cmd == "aa" else cmd_run(args)


if __name__ == "__main__":
    main()
