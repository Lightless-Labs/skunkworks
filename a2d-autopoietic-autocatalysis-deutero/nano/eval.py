#!/usr/bin/env python3
"""Eval v2 for the nanoharness (A²D cycle 2): one keep/discard experiment.

    python3 -I nano/eval.py run --id E001 --candidate <file.py> --hypothesis "..." [--proposer me]
    python3 -I nano/eval.py aa [--reps 1]        # A/A: incumbent vs itself; writes nano/aa.json
    python3 -I nano/eval.py halves               # writes nano/halves.json (fixed seeded family split)

Protocol (docs/plans/2026-10-15-cycle2-nanoharness.md):
- incumbent = nano/harness.py; candidate and incumbent run in ONE `a2d lift`
  run as external arms X:inc and X:cand, paired by task (k = 5);
- screen on half S of the dev families; a candidate whose screen delta is
  below half the keep threshold is discarded there;
- otherwise run the other half and pool: delta = mean paired difference in
  hidden-test fraction over all dev tasks (all-pass difference reported);
- keep iff delta > threshold, the 95th percentile of the family-bootstrap
  null from the A/A run (centred per-task differences);
- every experiment appends one line to nano/results.tsv; a keep replaces
  nano/harness.py and archives the old one in nano/history/.
"""
import argparse, datetime, hashlib, json, os, pathlib, random, shutil, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
NANO = ROOT / "nano"
DEV = ROOT / "tasks/train-all"
A2D = str(ROOT / "target/debug/a2d")


def families(suite):
    fams = {}
    for t in sorted(p for p in suite.iterdir() if (p / "task.json").exists()):
        fams.setdefault(json.loads((t / "task.json").read_text())["family"], []).append(t.name)
    return fams


def halves():
    path = NANO / "halves.json"
    if path.exists():
        return json.loads(path.read_text())
    fams = sorted(families(DEV))
    rng = random.Random("A2D-C2-HALVES")
    rng.shuffle(fams)
    split = {"S": sorted(fams[: len(fams) // 2]), "R": sorted(fams[len(fams) // 2:])}
    path.write_text(json.dumps(split, indent=1) + "\n")
    return split


def half_suite(name):
    d = NANO / "suites" / name
    if not d.exists():
        d.mkdir(parents=True)
        fams = families(DEV)
        for f in halves()[name]:
            for task in fams[f]:
                (d / task).symlink_to(os.path.relpath((DEV / task).resolve(), d))
    return d


def lift(exp, suite, inc, cand, out, seed, jobs):
    cmd = [A2D, "lift", "run", "--experiment", exp, "--unregistered", "--suite", str(suite),
           "--model", "codex/gpt-6-luna", "--effort", "low", "--arms", "X:inc,X:cand",
           "--external", f"inc={inc}", "--external", f"cand={cand}", "--k", "5",
           "--seed", str(seed), "--jobs", str(jobs), "--min-families", "1",
           "--max-invocations", "1500", "--max-wall-clock-secs", "14400", "--out", str(out)]
    with open(str(out) + ".log", "w") as log:
        code = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT).returncode
    if code:
        sys.exit(f"lift run failed ({code}); see {out}.log")


def scores(run):
    """{(arm, task): (fraction, all_passed, family)} plus invocations by arm."""
    out, inv = {}, {}
    for line in (run / "ledger.jsonl").read_text().splitlines():
        r = json.loads(line)
        if r["kind"] == "score":
            res = r["result"]
            exp = res.get("expected") or 0
            frac = res["passed"] / exp if exp and res.get("compiled") else 0.0
            out[(r["arm"], r["task"])] = (frac, 1.0 if res["all_passed"] else 0.0, r["family"])
        elif r["kind"] == "invocation":
            inv[r["arm"]] = inv.get(r["arm"], 0) + 1
    return out, inv


def paired(runs, x="X:cand", y="X:inc"):
    """Per-task differences (fraction, all-pass) with families, over runs."""
    rows = []
    for run in runs:
        s, _ = scores(run)
        for (arm, task), (fx, ax, fam) in s.items():
            if arm == x and (y, task) in s:
                fy, ay, _ = s[(y, task)]
                rows.append((fam, fx - fy, ax - ay))
    return rows


def cluster_se(rows, idx=1, reps=2000, seed=7):
    by = {}
    for r in rows:
        by.setdefault(r[0], []).append(r[idx])
    fams = list(by)
    rng = random.Random(seed)
    means = []
    for _ in range(reps):
        pick = [rng.choice(fams) for _ in fams]
        vals = [v for f in pick for v in by[f]]
        means.append(sum(vals) / len(vals))
    mu = sum(means) / len(means)
    return (sum((m - mu) ** 2 for m in means) / (len(means) - 1)) ** 0.5


def aa_threshold(rows, reps=4000, seed=11):
    """95th percentile of the family-bootstrap mean of centred A/A differences."""
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


def log_result(fields):
    with open(NANO / "results.tsv", "a") as f:
        f.write("\t".join(str(fields.get(k, "")) for k in (
            "id", "date", "proposer", "hypothesis", "delta_frac", "se", "delta_allpass",
            "threshold", "decision", "invocations", "note")) + "\n")


def cmd_aa(args):
    inc = NANO / "harness.py"
    runs = []
    for rep in range(args.reps):
        for half in ("S", "R"):
            out = ROOT / "runs" / f"c2-aa-{half}-r{rep}"
            if not (out / "summary.json").exists():
                lift(f"C2-AA-{half}{rep}", half_suite(half), inc, inc, out, 1000 + rep * 10 + (half == "R"), args.jobs)
            runs.append(out)
    rows = paired(runs)
    thr = aa_threshold(rows)
    inv = sum(sum(scores(r)[1].values()) for r in runs)
    result = {"harness_sha256": hashlib.sha256(inc.read_bytes()).hexdigest(), "n": len(rows),
              "mean_delta_frac": sum(r[1] for r in rows) / len(rows),
              "sd_task_frac": (sum((r[1]) ** 2 for r in rows) / len(rows)) ** 0.5,
              "threshold_frac": thr, "runs": [str(r.relative_to(ROOT)) for r in runs], "invocations": inv}
    (NANO / "aa.json").write_text(json.dumps(result, indent=1) + "\n")
    print(json.dumps(result, indent=1))


def cmd_run(args):
    aa = json.loads((NANO / "aa.json").read_text())
    thr = aa["threshold_frac"]
    inc = NANO / "harness.py"
    cand = pathlib.Path(args.candidate).resolve()
    exp_dir = ROOT / "runs" / f"c2-{args.id}"
    exp_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy(inc, exp_dir / "incumbent.py")
    shutil.copy(cand, exp_dir / "candidate.py")
    (exp_dir / "hypothesis.txt").write_text(args.hypothesis + "\n")
    seed = int(hashlib.sha256(args.id.encode()).hexdigest()[:6], 16)
    runs = []
    screen = exp_dir / "screen"
    if not (screen / "summary.json").exists():
        lift(f"C2-{args.id}-S", half_suite("S"), exp_dir / "incumbent.py", exp_dir / "candidate.py", screen, seed, args.jobs)
    runs.append(screen)
    rows = paired(runs)
    d_screen = sum(r[1] for r in rows) / len(rows)
    decision, note = None, f"screen delta {d_screen:+.4f} on {len(rows)}"
    if d_screen < thr / 2:
        decision = "discard (screen)"
    else:
        rest = exp_dir / "rest"
        if not (rest / "summary.json").exists():
            lift(f"C2-{args.id}-R", half_suite("R"), exp_dir / "incumbent.py", exp_dir / "candidate.py", rest, seed + 1, args.jobs)
        runs.append(rest)
        rows = paired(runs)
    delta = sum(r[1] for r in rows) / len(rows)
    d_all = sum(r[2] for r in rows) / len(rows)
    se = cluster_se(rows)
    if decision is None:
        decision = "keep" if delta > thr else "discard"
    inv = sum(sum(scores(r)[1].values()) for r in runs)
    log_result({"id": args.id, "date": datetime.date.today().isoformat(), "proposer": args.proposer,
                "hypothesis": args.hypothesis.replace("\t", " "), "delta_frac": f"{delta:+.4f}",
                "se": f"{se:.4f}", "delta_allpass": f"{d_all:+.4f}", "threshold": f"{thr:.4f}",
                "decision": decision, "invocations": inv, "note": note})
    if decision == "keep":
        hist = NANO / "history"
        hist.mkdir(exist_ok=True)
        shutil.copy(inc, hist / f"before-{args.id}.py")
        shutil.copy(cand, inc)
    print(json.dumps({"id": args.id, "delta_frac": delta, "se": se, "delta_allpass": d_all,
                      "threshold": thr, "decision": decision, "invocations": inv, "n": len(rows)}, indent=1))


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
    a.add_argument("--reps", type=int, default=1)
    a.add_argument("--jobs", type=int, default=4)
    sub.add_parser("halves")
    args = ap.parse_args()
    if args.cmd == "halves":
        print(json.dumps(halves(), indent=1))
    elif args.cmd == "aa":
        cmd_aa(args)
    else:
        cmd_run(args)


if __name__ == "__main__":
    main()
