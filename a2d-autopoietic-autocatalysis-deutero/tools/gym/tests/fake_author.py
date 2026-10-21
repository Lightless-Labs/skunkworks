"""Fake model outputs for the gym v3 tests (run by fake_codex.sh inside the job sandbox; never
calls a model).

  author <fid> <class> [dup] [bad-rubric]   write out/<fid>/<fid>-l1..l4 and REPORT.md in the cwd
  adjudicate                                 write VERDICTS.jsonl for LIST.tsv in the cwd
  repair                                     write out/<id>/task.json (requirements only) for LIST.tsv

Requirements are 20 numbered clauses of hash-derived words (unique per family and level, so the
similarity check sees no overlap unless `dup`), plus the fixture's API line, a state list (R19)
and, for scale, a size bound and complexity (R18). Hidden tests are the fixture's (opaque h_<hex>
names, `// R..` traces); rubric.json labels the fixture mutants and carries the structured
evidence of every declared tag (modifiers, states/sequences, signatures, boundaries, scale).
Adjudicators quote 8 words of a clause the listed test's trace cites (any clause for `*`).
Env hooks: GYM_TEST_ADJ_UNDER, GYM_TEST_ADJ_SPLIT, GYM_TEST_ADJ_BADQUOTE (space-separated task
ids), GYM_TEST_REPAIR_BAD.
"""
import hashlib
import json
import os
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIX = HERE / "fixture"
EXTRA = {"depth": ["edges", "state"], "state": ["edges", "depth"], "scale": ["edges", "depth"],
         "surface": ["edges", "depth"], "edges": ["depth", "state"]}
SIGNATURES = ["pub struct Customer", "pub enum Kind", "pub struct Event", "pub enum Error",
              "pub fn simulate(customers: &[Customer], wake_cost: u64) -> Result<Vec<Event>, Error>"]
API = "API: " + ", ".join(SIGNATURES)
SURFACE_EXTRA = "pub type Time = u64"
BOUND, COMPLEXITY = "at most 100 customers per call", "O(n log n) time"
SEQUENCE_TESTS = ["h_f520c34fab", "h_598fb0904c"]


def words(seed, n=12):
    h = hashlib.sha256(seed.encode()).hexdigest()
    return list(h[:n])


def states(seed):
    return [f"st{w}{i}" for i, w in enumerate(words(f"{seed}:states", 5))]


def requirements(fid, level, cls, dup):
    seed = f"dup-{level}" if dup else f"{fid}-{level}"
    out = ["SECRET-REQ Implement in Rust as a SINGLE source file that compiles with rustc --edition 2024, std only."]
    for n in range(1, 21):
        ref = f" This clause modifies R{n - 1}." if n in (6, 10, 14) else ""
        out.append(f"R{n}. " + " ".join(words(f"{seed}:{n}")) + ref)
    if cls == "scale":
        out[18] += f" The input holds {BOUND}; simulate runs in {COMPLEXITY}."
    out[19] += " States: " + ", ".join(states(seed)) + "."
    out[-1] += " " + API + (", " + SURFACE_EXTRA if cls == "surface" else "")
    return "\n".join(out)


def evidence(req, tags, cls, seed):
    ev = {}
    if "depth" in tags:
        ev["modifiers"] = [{"clause": f"R{n}", "modifies": f"R{n - 1}"} for n in (6, 10, 14)]
    if "state" in tags:
        ev["states"], ev["sequences"] = states(seed), list(SEQUENCE_TESTS)
    if "surface" in tags:
        ev["signatures"] = SIGNATURES + ([SURFACE_EXTRA] if cls == "surface" else [])
    if "edges" in tags:
        ev["boundaries"] = [" ".join(clause_words(req, 3)[:3]), " ".join(clause_words(req, 11)[:4])]
    if "scale" in tags:
        ev["scale"] = {"bound": BOUND, "complexity": COMPLEXITY}
    return ev


def clause_words(req, n):
    for line in req.split("\n"):
        if line.startswith(f"R{n}. "):
            return line.split()[1:9]
    raise KeyError(n)


def author(fid, cls, dup=False, bad=False):
    for lv in (1, 2, 3, 4):
        tid = f"{fid}-l{lv}"
        d = Path("out") / fid / tid
        (d / "mutants").mkdir(parents=True, exist_ok=True)
        req = requirements(fid, lv, cls, dup)
        json.dump({"id": tid, "family": fid, "requirements": req, "test_timeout_secs": 5}, open(d / "task.json", "w"))
        for f in ("public_tests.rs", "reference.rs"):
            shutil.copyfile(FIX / f, d / f)
        hidden = (FIX / "hidden_tests.rs").read_text()
        if bad:
            hidden = hidden.replace("// R4\n", "", 1)              # one untraced hidden test: M1 fails
        (d / "hidden_tests.rs").write_text(hidden)
        for m in (1, 2, 3, 4):
            shutil.copyfile(FIX / "mutants" / f"m{m}.rs", d / "mutants" / f"m{m}.rs")
        tags = [cls] + EXTRA[cls][:1 if lv <= 2 else 2]
        seed = f"dup-{lv}" if dup else f"{fid}-{lv}"
        muts = {"m1": {"violates": ["R5"], "kind": "misreading", "refuted_by": " ".join(clause_words(req, 5))},
                "m2": {"violates": ["R4"], "kind": "misreading", "refuted_by": " ".join(clause_words(req, 4))},
                "m3": {"violates": ["R12"], "kind": "bug", "refuted_by": ""},
                "m4": {"violates": ["R16"], "kind": "bug", "refuted_by": ""}}
        if cls == "scale":
            shutil.copyfile(FIX / "naive.rs", d / "mutants" / "m4.rs")
            muts["m4"] = {"violates": ["R20"], "kind": "naive", "refuted_by": ""}
        json.dump({"difficulty": tags, "mutants": muts, **evidence(req, tags, cls, seed)}, open(d / "rubric.json", "w"))
    (Path("out") / fid / "REPORT.md").write_text("report\n")


def listed():
    rows = [x.split("\t") for x in Path("LIST.tsv").read_text().split("\n") if x.strip()]
    return rows


def traces(hidden):
    """{test: [cited clause numbers]} from the `// R..` line right above each #[test] fn."""
    out, last = {}, []
    rows = [x.strip() for x in hidden.split("\n")]
    for i, line in enumerate(rows):
        if line.startswith("// R"):
            last = [int(x.strip()[1:]) for x in line[3:].split(",")]
        elif line.startswith("fn ") and i and rows[i - 1] == "#[test]":
            out[line[3:].split("(")[0]] = last
            last = []
    return out


def adjudicate():
    role = os.path.basename(os.getcwd()).rsplit("-", 1)[-1]
    under = set(os.environ.get("GYM_TEST_ADJ_UNDER", "").split())
    split = set(os.environ.get("GYM_TEST_ADJ_SPLIT", "").split())
    badq = set(os.environ.get("GYM_TEST_ADJ_BADQUOTE", "").split())
    lines = []
    for row in listed():
        tid, test = row[0], row[1]
        req = json.loads((Path("tasks") / tid / "task.json").read_text())["requirements"]
        cited = traces((Path("tasks") / tid / "hidden_tests.rs").read_text()).get(test) or [7]
        verdict = "UNDERDETERMINED" if tid in under or (tid in split and role == "B") else "DETERMINED"
        quote = " ".join(clause_words(req, cited[0])) if verdict == "DETERMINED" else ""
        if tid in badq:
            quote = "this sentence is certainly not anywhere in the requirements text"
        reading = "the other order of the cohort" if verdict == "DETERMINED" else ""
        lines.append(json.dumps({"task": tid, "test": test, "verdict": verdict, "quote": quote,
                                 "excluded_reading": reading}))
    Path("VERDICTS.jsonl").write_text("\n".join(lines) + "\n")


def repair():
    for tid in sorted({row[0] for row in listed()}):
        t = json.loads((Path("tasks") / tid / "task.json").read_text())
        t["requirements"] += " (clarified)"
        (Path("out") / tid).mkdir(parents=True, exist_ok=True)
        json.dump(t, open(Path("out") / tid / "task.json", "w"))
        if os.environ.get("GYM_TEST_REPAIR_BAD"):
            (Path("out") / tid / "hidden_tests.rs").write_text("// edited\n")


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "author":
        author(sys.argv[2], sys.argv[3], "dup" in sys.argv[4:], "bad-rubric" in sys.argv[4:])
    elif cmd == "adjudicate":
        adjudicate()
    elif cmd == "repair":
        repair()
