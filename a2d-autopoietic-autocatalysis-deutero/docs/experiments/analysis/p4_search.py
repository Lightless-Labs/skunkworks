"""P4 search: lessons written by A²D's architect, with a fresh-incumbent ratchet.

Implements docs/plans/2026-10-15-p4-autopoietic-lift.md § Searches.
- A seeded family split of the train suite into halves (A, B), written to
  <out>/split.json, with symlink half-suites under <out>/suites/{A,B}.
- Generation g: `a2d distill` reads the incumbent's latest failures on the
  proposal half X only (`--families`), then C(candidate) and C(incumbent) run
  fresh in ONE run (`--arms C:cand,C:inc`) on the other half Y. The candidate
  is accepted if it gains >= 4 net tasks. The halves then swap.
- Every search starts from the base harness's lessons (empty for H0 v5).
- Tripwires are recorded per generation: E0601 prompts, hidden compile
  failures, and any harness change other than `lessons` and `name`.

usage: python3 -I p4_search.py --search I1 --mode informed --seed 101
         --base harnesses/h0-v5.json --suite tasks/train-all
         --g0-run runs/<G0 run dir> [--g0-arm C] [--generations 3]
         [--accept-net 4] [--jobs 4] --out runs/<dir>
"""
import argparse, hashlib, json, os, pathlib, subprocess, sys

A2D = "./target/debug/a2d"


def sh(cmd, log):
    with open(log, "w") as f:
        r = subprocess.run(cmd, stdout=f, stderr=subprocess.STDOUT)
    if r.returncode:
        sys.exit(f"command failed ({r.returncode}): {' '.join(cmd)}; see {log}")


def families_of(suite):
    fams = {}
    for task in sorted(p for p in suite.iterdir() if (p / "task.json").exists()):
        fams.setdefault(json.loads((task / "task.json").read_text())["family"], []).append(task.name)
    return fams


def split(fams, seed):
    order = sorted(fams, key=lambda f: hashlib.sha256(f"{seed}:{f}".encode()).hexdigest())
    half = len(order) // 2
    return sorted(order[:half]), sorted(order[half:])


def half_suite(out, name, suite, fams, families):
    d = out / "suites" / name
    d.mkdir(parents=True, exist_ok=True)
    for f in families:
        for task in fams[f]:
            link = d / task
            if not link.exists():
                link.symlink_to(os.path.relpath((suite / task).resolve(), d))
    return d


def summary_arm(run, label):
    s = json.loads((run / "summary.json").read_text())
    return next(a for a in s["arms"] if a["arm"] == label), s


def e0601(run, label):
    d = run / "invocations" / label.replace(":", "-")
    return sum("E0601" in p.read_text() for p in d.glob("*/r*/*.prompt.txt"))


def main():
    ap = argparse.ArgumentParser()
    for flag in ["--search", "--mode", "--seed", "--base", "--suite", "--g0-run", "--out"]:
        ap.add_argument(flag, required=True)
    ap.add_argument("--g0-arm", default="C")
    ap.add_argument("--generations", type=int, default=3)
    ap.add_argument("--accept-net", type=int, default=4)
    ap.add_argument("--jobs", default="4")
    a = ap.parse_args()
    out, suite = pathlib.Path(a.out), pathlib.Path(a.suite)
    out.mkdir(parents=True, exist_ok=True)
    fams = families_of(suite)
    half_a, half_b = split(fams, a.seed)
    (out / "split.json").write_text(json.dumps({"seed": a.seed, "A": half_a, "B": half_b}, indent=1))
    suites = {"A": half_suite(out, "A", suite, fams, half_a), "B": half_suite(out, "B", suite, fams, half_b)}
    for name, fl in (("A", half_a), ("B", half_b)):
        (out / f"families-{name}.json").write_text(json.dumps(fl))

    incumbent = pathlib.Path(a.base)
    inc_json = json.loads(incumbent.read_text())
    # Latest run holding the incumbent's outcomes: (run dir, suite, arm label).
    latest = (pathlib.Path(a.g0_run), suite, a.g0_arm)
    trajectory = []
    for g in range(1, a.generations + 1):
        x, y = ("A", "B") if g % 2 == 1 else ("B", "A")
        gdir = out / f"g{g}"
        gdir.mkdir(exist_ok=True)
        cand = gdir / "candidate.json"
        sh([A2D, "distill", "--from", f"{latest[0]}={latest[1]}", "--arm", latest[2],
            "--families", str(out / f"families-{x}.json"), "--harness", str(incumbent),
            "--out", str(cand), "--log", str(gdir / "distill.json"),
            "--name", f"{a.search}-g{g}", "--mode", a.mode, "--seed", str(int(a.seed) + g),
            "--max-lessons", "8", "--jobs", a.jobs], gdir / "distill.log")
        cand_json = json.loads(cand.read_text())
        other = {k: v for k, v in cand_json.items() if k not in ("lessons", "name")}
        tripwire_harness = other != {k: v for k, v in inc_json.items() if k not in ("lessons", "name")}
        record = {"generation": g, "proposal_half": x, "eval_half": y,
                  "distill": json.loads((gdir / "distill.json").read_text()) | {"diagnoses": None},
                  "candidate_lessons": cand_json.get("lessons", []),
                  "incumbent_lessons": inc_json.get("lessons", []),
                  "tripwire_harness_changed": tripwire_harness}
        if tripwire_harness or cand_json.get("lessons", []) == inc_json.get("lessons", []):
            record["evaluated"] = False
            record["accepted"] = False
            trajectory.append(record)
            (out / "trajectory.json").write_text(json.dumps(trajectory, indent=1))
            continue
        run = gdir / "eval"
        sh([A2D, "lift", "run", "--experiment", f"{a.search}-g{g}", "--unregistered",
            "--suite", str(suites[y]), "--model", "codex/gpt-6-luna", "--effort", "low",
            "--arms", "C:cand,C:inc", "--harness", f"cand={cand}", "--harness", f"inc={incumbent}",
            "--k", "5", "--seed", str(int(a.seed) * 10 + g), "--jobs", a.jobs, "--min-families", "1",
            "--max-invocations", "700", "--max-wall-clock-secs", "10800", "--out", str(run)],
           gdir / "eval.log")
        c, s = summary_arm(run, "C:cand")
        i, _ = summary_arm(run, "C:inc")
        net = c["passed"] - i["passed"]
        accepted = net >= a.accept_net and s["tuples_void"] == 0
        record |= {"evaluated": True, "cand_passed": c["passed"], "inc_passed": i["passed"], "n": c["n"],
                   "net": net, "accepted": accepted, "void": s["tuples_void"],
                   "invocations": c["invocations"] + i["invocations"], "tokens": c["tokens"] + i["tokens"],
                   "tripwire_e0601": {"cand": e0601(run, "C:cand"), "inc": e0601(run, "C:inc")},
                   "tripwire_hidden_compile_failures": {"cand": c["hidden_compile_failures"],
                                                        "inc": i["hidden_compile_failures"]}}
        trajectory.append(record)
        (out / "trajectory.json").write_text(json.dumps(trajectory, indent=1))
        if accepted:
            incumbent, inc_json = cand, cand_json
            latest = (run, suites[y], "C:cand")
        else:
            latest = (run, suites[y], "C:inc")
    final = out / "hstar.json"
    final.write_text(json.dumps(inc_json | {"name": f"{a.search}-final"}, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps({"search": a.search, "final_lessons": inc_json.get("lessons", []),
                      "accepted": [r["generation"] for r in trajectory if r["accepted"]]}, indent=1))


if __name__ == "__main__":
    main()
