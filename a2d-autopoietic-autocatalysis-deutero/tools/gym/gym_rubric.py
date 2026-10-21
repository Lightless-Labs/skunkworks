"""Mechanical checks of the frozen gym v3 rubric (authoring/SPEC.md; plan 2026-10-21 "Frozen rubric").

Pure functions over task text; no I/O except `check_task_dir`. Every result is counts and
booleans (plus requirement ids, which are not content), so drivers can print them through the
gateway.

- M1 traceability: every hidden test carries a `// R<n>[,R<m>]` line above its #[test]; every
  cited id exists; every requirement id is cited by at least one hidden test.
- M2 mutants: rubric.json labels each of m1..m4 with the requirement(s) it violates; at least 2
  are plausible misreadings, each refuted by a verbatim requirements sentence (>= 8 words) inside
  one of the clauses it violates.
- M3 adjudication: a DETERMINED verdict counts only with a verbatim quote of >= 8 words that is a
  substring of the requirements (whitespace-normalised) and a named excluded reading; a test is
  DETERMINED only if both adjudicators say so validly; anything else gets a requirements-only repair.
- M4 spec review: 2 seeded passing hidden tests per task go to the second adjudicator.
- M5 opaque names: hidden tests are named h_<hex> (>= 8 hex digits), unique, and the names appear
  nowhere in the requirements, the public tests or the reference.
- Difficulty tags (rubric.json "difficulty"): >= 2 of the allowed sources (>= 3 at l3/l4); the
  family's class among them; depth at l3/l4 needs >= 10 clauses, >= 3 of which cite another
  clause; surface needs >= 6 public items named in the requirements; scale needs a mutant of kind
  "naive" (the validator times it).
- Never filter on solvability: EXCLUSION_REASONS has no audit-outcome reason.
"""
import hashlib
import json
import random
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymvendor import lines, test_names  # noqa: E402

CLASSES = ("depth", "state", "scale", "surface", "edges")
CLAUSE = re.compile(r"^[ \t]*R(\d+)\.[ \t]", re.M)
TRACE = re.compile(r"^//\s*(R\d+(?:\s*,\s*R\d+)*)\s*$")
OPAQUE = re.compile(r"^h_[0-9a-f]{8,}$")
MIN_QUOTE_WORDS = 8
MIN_READING_WORDS = 4
MUTANTS = ("m1", "m2", "m3", "m4")
MUTANT_KINDS = ("misreading", "bug", "naive")
PUB_ITEM = re.compile(r"\bpub\s+(?:fn|struct|enum|trait|type|const)\s+([A-Za-z_][A-Za-z0-9_:]*)")
# A task leaves the gym only for these reasons; an audit (Sol) failure is not one of them.
EXCLUSION_REASONS = ("validation-cap", "author-exhausted", "similarity", "adjudication-underdetermined",
                     "adjudication-failed", "rubric")


def norm(text):
    return " ".join(str(text).split())


# ------------------------------------------------------------------ clauses

def clauses(requirements):
    """{id: clause text} for clauses written as `R<n>. ...` at the start of a line. Ids must be
    unique and contiguous from 1, else ValueError."""
    marks = list(CLAUSE.finditer(requirements))
    ids = [int(m.group(1)) for m in marks]
    if not ids or ids != list(range(1, len(ids) + 1)):
        raise ValueError("clause ids must be R1..Rn in order")
    out = {}
    for i, m in enumerate(marks):
        end = marks[i + 1].start() if i + 1 < len(marks) else len(requirements)
        out[ids[i]] = requirements[m.start():end]
    return out


def _ids(text):
    return [int(x) for x in re.findall(r"R(\d+)", text)]


# ------------------------------------------------------------------ M1

def hidden_traces(source):
    """{test name: [cited ids]} from `// R<n>[,R<m>]` lines in the comment/attribute block
    directly above each #[test] function. A test without one maps to []."""
    out, block, armed = {}, [], False

    def name_of(rest):
        return re.split(r"[(< ]", rest[3:], maxsplit=1)[0]

    for raw in lines(source):
        line = raw.strip()
        if line.startswith("//"):
            m = TRACE.match(line)
            if m:
                block += _ids(m.group(1))
            continue
        if line.startswith("#[test]"):
            rest = line[len("#[test]"):].lstrip()
            if rest.startswith("fn "):
                out[name_of(rest)] = block
                block, armed = [], False
            else:
                armed = True
            continue
        if line.startswith("#[") or (armed and not line):
            continue
        if armed and line.startswith("fn "):
            out[name_of(line)] = block
        block, armed = [], False
    return out


def m1(requirements, hidden):
    try:
        ids = set(clauses(requirements))
    except ValueError:
        return {"ok": False, "clauses": 0, "untraced": None, "unknown": None, "uncovered": None}
    traces = hidden_traces(hidden)
    names = test_names(hidden)
    untraced = sorted(n for n in names if not traces.get(n))
    cited = {i for refs in traces.values() for i in refs}
    unknown = sorted(cited - ids)
    uncovered = sorted(ids - cited)
    return {"ok": not untraced and not unknown and not uncovered and bool(names), "clauses": len(ids),
            "untraced": len(untraced), "unknown": len(unknown), "uncovered": len(uncovered),
            "uncovered_ids": [f"R{i}" for i in uncovered]}


# ------------------------------------------------------------------ quotes (M2, M3)

def quote_ok(quote, text, min_words=MIN_QUOTE_WORDS):
    q = norm(quote)
    return isinstance(quote, str) and len(q.split()) >= min_words and q in norm(text)


def reading_ok(reading, quote):
    r = norm(reading)
    return isinstance(reading, str) and len(r.split()) >= MIN_READING_WORDS and r != norm(quote)


# ------------------------------------------------------------------ M2

def m2(requirements, rubric):
    try:
        cl = clauses(requirements)
    except ValueError:
        return {"ok": False, "labelled": 0, "misreadings": 0}
    muts = rubric.get("mutants") if isinstance(rubric, dict) else None
    if not isinstance(muts, dict) or sorted(muts) != list(MUTANTS):
        return {"ok": False, "labelled": 0, "misreadings": 0}
    labelled = misreadings = 0
    for m in MUTANTS:
        e = muts[m]
        if not isinstance(e, dict):
            continue
        v = e.get("violates")
        refs = [int(x[1:]) for x in v] if isinstance(v, list) and v and all(
            isinstance(x, str) and re.fullmatch(r"R\d+", x) for x in v) else None
        if refs is None or any(r not in cl for r in refs) or e.get("kind") not in MUTANT_KINDS:
            continue
        labelled += 1
        if e["kind"] == "misreading" and any(quote_ok(e.get("refuted_by", ""), cl[r]) for r in refs):
            misreadings += 1
    return {"ok": labelled == 4 and misreadings >= 2, "labelled": labelled, "misreadings": misreadings}


# ------------------------------------------------------------------ M3

VERDICT_KEYS = {"task", "test", "verdict", "quote", "excluded_reading"}


def parse_verdicts(text, expected):
    """Strict parse of an adjudicator's VERDICTS.jsonl against {task: [test, ...]}: every expected
    (task, test) exactly once, no other line, exact keys and string values. Else None."""
    got = {}
    for line in lines(text):
        if not line.strip():
            continue
        try:
            e = json.loads(line)
        except ValueError:
            return None
        if not isinstance(e, dict) or set(e) != VERDICT_KEYS or not all(isinstance(e[k], str) for k in VERDICT_KEYS):
            return None
        if e["task"] not in expected or e["test"] not in expected[e["task"]] \
                or e["verdict"] not in ("DETERMINED", "UNDERDETERMINED") or e["test"] in got.get(e["task"], {}):
            return None
        got.setdefault(e["task"], {})[e["test"]] = {k: e[k] for k in ("verdict", "quote", "excluded_reading")}
    if {t: sorted(v) for t, v in got.items()} != {t: sorted(v) for t, v in expected.items() if v}:
        return None
    return got


def valid_determined(entry, requirements):
    return entry is not None and entry.get("verdict") == "DETERMINED" \
        and quote_ok(entry.get("quote", ""), requirements) and reading_ok(entry.get("excluded_reading", ""),
                                                                          entry.get("quote", ""))


def combine(a, b, requirements):
    """Two adjudicators' entries for one test -> DETERMINED (both valid DETERMINED) or REPAIR."""
    return "DETERMINED" if valid_determined(a, requirements) and valid_determined(b, requirements) else "REPAIR"


# ------------------------------------------------------------------ M4

def sample_passing(task, content, hidden_names, failing, seed, k=2):
    """k hidden tests the audit passed, chosen by a seed bound to the task and its content."""
    pool = sorted(set(hidden_names) - set(failing))
    rng = random.Random(hashlib.sha256(f"{seed}:{task}:{content}".encode()).hexdigest())
    return sorted(rng.sample(pool, min(k, len(pool))))


# ------------------------------------------------------------------ M5

def _token(name):
    return re.compile(r"(?<![A-Za-z0-9_])" + re.escape(name) + r"(?![A-Za-z0-9_])")


def m5(requirements, public, hidden, reference):
    names = test_names(hidden)
    declared = hidden.count("#[test]")
    bad = [n for n in names if not OPAQUE.match(n)]
    leaked = [n for n in names if any(_token(n).search(t) for t in (requirements, public, reference))]
    return {"ok": bool(names) and not bad and not leaked and declared == len(names),
            "tests": declared, "not_opaque": len(bad), "leaked": len(leaked),
            "duplicates": declared - len(names)}


# ------------------------------------------------------------------ difficulty tags

def tags(requirements, rubric, level, family_class=None):
    t = rubric.get("difficulty") if isinstance(rubric, dict) else None
    if not isinstance(t, list) or len(set(t)) != len(t) or not set(t) <= set(CLASSES):
        return {"ok": False, "tags": 0}
    need = 3 if level >= 3 else 2
    ok = len(t) >= need and (family_class is None or family_class in t)
    try:
        cl = clauses(requirements)
    except ValueError:
        return {"ok": False, "tags": len(t)}
    if "depth" in t and level >= 3:
        modifying = sum(1 for i, c in cl.items() if any(j != i for j in _ids(c[c.index(".") + 1:])))
        ok = ok and len(cl) >= 10 and modifying >= 3
    if "surface" in t:
        ok = ok and len(set(PUB_ITEM.findall(requirements))) >= 6
    if "scale" in t:
        muts = rubric.get("mutants") or {}
        ok = ok and any(isinstance(e, dict) and e.get("kind") == "naive" for e in muts.values())
    return {"ok": ok, "tags": len(t)}


def check_task_dir(d, level, family_class=None):
    """All mechanical rubric checks for one task dir (task.json, tests, reference, rubric.json)."""
    d = Path(d)
    req = json.loads((d / "task.json").read_text())["requirements"]
    hidden = (d / "hidden_tests.rs").read_text()
    public = (d / "public_tests.rs").read_text()
    reference = (d / "reference.rs").read_text()
    rubric = json.loads((d / "rubric.json").read_text())
    r = {"m1": m1(req, hidden), "m2": m2(req, rubric), "m5": m5(req, public, hidden, reference),
         "tags": tags(req, rubric, level, family_class)}
    r["ok"] = all(v["ok"] for v in r.values())
    return r
