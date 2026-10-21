"""Mechanical checks of the frozen gym v3 rubric (authoring/SPEC.md; plan 2026-10-21 "Frozen rubric").

Pure functions over task text; no I/O except `check_task_dir`. Every result is counts and
booleans (plus requirement ids, which are not content), so drivers can print them through the
gateway. Every pattern is ASCII-only (`[0-9]`, re.ASCII): `R١` or `// R２` never parse as ids.

- M1 traceability: every hidden test carries `// R<n>[,R<m>]` line(s) above its #[test]; a trace
  line cites at most 3 ids; every cited id exists; every requirement id is cited by at least one
  hidden test whose whole trace cites at most 3 ids (a catch-all trace covers nothing).
- M2 mutants: rubric.json labels each of m1..m4 with the requirement(s) it violates; at least 2
  are plausible misreadings, each refuted by `refuted_by`: a literal substring (no normalisation,
  >= 8 whitespace-separated words) of one of the clauses it violates.
- M3 adjudication: a DETERMINED verdict counts only with a quote that is a literal substring (no
  normalisation, >= 8 words) of a clause the test's `// R<n>` trace cites (a `*` verdict: of any
  numbered clause; text before R1 never counts) and a named excluded reading; a test is
  DETERMINED only if both adjudicators say so validly; anything else gets a requirements-only repair.
- M4 spec review: 2 seeded hidden tests per task go to the second adjudicator, under the M3 rule.
- M5 opaque names: hidden tests are named h_<hex> (>= 8 hex digits), unique, and no name occurs
  anywhere (literal substring, no token boundary) in the requirements, the public tests or the
  reference.
- Clause floors (SPEC rule 2), whatever the tags: l1/l2 >= 5 clauses, l3/l4 >= 10.
- Difficulty tags (rubric.json "difficulty"): >= 2 of the allowed sources (>= 3 at l3/l4); the
  family's class among them (a class is required); each tag needs its structured evidence in
  rubric.json, checked against the text (tags()).
- Never filter on solvability: EXCLUSION_REASONS has no audit-outcome reason.
"""
import hashlib
import json
import random
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymvendor import lines, strict_json, test_names  # noqa: E402

CLASSES = ("depth", "state", "scale", "surface", "edges")
CLAUSE = re.compile(r"^[ \t]*R([0-9]+)\.[ \t]", re.M | re.ASCII)
TRACE = re.compile(r"^//[ \t]*(R[0-9]+(?:[ \t]*,[ \t]*R[0-9]+)*)[ \t]*$", re.ASCII)
REF = re.compile(r"(?<![A-Za-z0-9_])R([0-9]+)\b", re.ASCII)
OPAQUE = re.compile(r"^h_[0-9a-f]{8,}$", re.ASCII)
RID = re.compile(r"R[0-9]+", re.ASCII)
DIGIT = re.compile(r"[0-9]", re.ASCII)
MIN_QUOTE_WORDS = 8
MIN_READING_WORDS = 4
MIN_BOUNDARY_WORDS = 3
MAX_TRACE_IDS = 3
CLAUSE_FLOOR = {1: 5, 2: 5, 3: 10, 4: 10}
MUTANTS = ("m1", "m2", "m3", "m4")
MUTANT_KINDS = ("misreading", "bug", "naive")
PUB_ITEM = re.compile(r"pub\s+(?:fn|struct|enum|trait|type|const)\s+([A-Za-z_][A-Za-z0-9_:]*)", re.ASCII)
CHECKS = ("m1", "m2", "m5", "clauses", "tags")
# A task leaves the gym only for these reasons; an audit (Sol) failure is not one of them.
EXCLUSION_REASONS = ("validation-cap", "author-exhausted", "similarity", "adjudication-underdetermined",
                     "adjudication-failed", "rubric")


def norm(text):
    return " ".join(str(text).split())


# ------------------------------------------------------------------ clauses

def clauses(requirements):
    """{id: clause text} for clauses written as `R<n>. ...` at the start of a line (the text from
    the marker to the next marker; the preamble before R1 belongs to none). Ids must be unique
    and contiguous from 1, else ValueError."""
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
    """Clause references in prose: `R<n>` not inside a longer word (COLOR21 is not R21)."""
    return [int(x) for x in REF.findall(text)]


def _body(clause):
    """A clause's text after its own `R<n>.` marker."""
    return clause[clause.index(".") + 1:]


# ------------------------------------------------------------------ M1

def hidden_traces(source):
    """{test name: [cited ids]} from `// R<n>[,R<m>]` lines in the comment/attribute block
    directly above each #[test] function (stacked lines add up). A test without one maps to []."""
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


def overcited_lines(source):
    """Trace lines citing more than MAX_TRACE_IDS ids."""
    n = 0
    for raw in lines(source):
        m = TRACE.match(raw.strip())
        if m and len(_ids(m.group(1))) > MAX_TRACE_IDS:
            n += 1
    return n


def m1(requirements, hidden):
    try:
        ids = set(clauses(requirements))
    except ValueError:
        return {"ok": False, "clauses": 0, "untraced": None, "unknown": None, "uncovered": None, "overcited": None}
    traces = hidden_traces(hidden)
    names = test_names(hidden)
    untraced = sorted(n for n in names if not traces.get(n))
    cited = {i for refs in traces.values() for i in refs}
    unknown = sorted(cited - ids)
    covering = {i for refs in traces.values() if len(set(refs)) <= MAX_TRACE_IDS for i in refs}
    uncovered = sorted(ids - covering)
    over = overcited_lines(hidden)
    return {"ok": not untraced and not unknown and not uncovered and not over and bool(names),
            "clauses": len(ids), "untraced": len(untraced), "unknown": len(unknown), "uncovered": len(uncovered),
            "overcited": over, "uncovered_ids": [f"R{i}" for i in uncovered]}


# ------------------------------------------------------------------ quotes (M2, M3)

def quote_ok(quote, texts, min_words=MIN_QUOTE_WORDS):
    """A literal substring (no whitespace or case normalisation) of one of `texts`, with at least
    `min_words` whitespace-separated words."""
    return isinstance(quote, str) and len(quote.split()) >= min_words and any(quote in t for t in texts)


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
            isinstance(x, str) and RID.fullmatch(x) for x in v) else None
        if refs is None or any(r not in cl for r in refs) or e.get("kind") not in MUTANT_KINDS:
            continue
        labelled += 1
        if e["kind"] == "misreading" and quote_ok(e.get("refuted_by"), [cl[r] for r in refs]):
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
            e = strict_json(line)
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


def quotable(requirements, hidden, test):
    """The clause texts a verdict on `test` may quote: the clauses its `// R<n>` trace cites;
    for `*` (the submission did not compile), every numbered clause. Never the preamble."""
    try:
        cl = clauses(requirements)
    except ValueError:
        return []
    if test == "*":
        return list(cl.values())
    return [cl[i] for i in dict.fromkeys(hidden_traces(hidden).get(test, [])) if i in cl]


def valid_determined(entry, texts):
    return isinstance(entry, dict) and entry.get("verdict") == "DETERMINED" \
        and quote_ok(entry.get("quote"), texts) and reading_ok(entry.get("excluded_reading"), entry.get("quote", ""))


def combine(a, b, texts):
    """Two adjudicators' entries for one test -> DETERMINED (both valid DETERMINED) or REPAIR."""
    return "DETERMINED" if valid_determined(a, texts) and valid_determined(b, texts) else "REPAIR"


# ------------------------------------------------------------------ M4

def sample_passing(task, content, hidden_names, failing, seed, k=2):
    """k hidden tests the audit passed, chosen by a seed bound to the task and its content."""
    pool = sorted(set(hidden_names) - set(failing))
    rng = random.Random(hashlib.sha256(f"{seed}:{task}:{content}".encode()).hexdigest())
    return sorted(rng.sample(pool, min(k, len(pool))))


# ------------------------------------------------------------------ M5 and the seal

def leaked_names(names, *texts):
    """The names that occur in any of the texts as a literal substring (no token boundary:
    `x_h_deadbeef_y` leaks `h_deadbeef`)."""
    return sorted(n for n in set(names) if any(n in t for t in texts))


def m5(requirements, public, hidden, reference):
    names = test_names(hidden)
    declared = hidden.count("#[test]")
    bad = [n for n in names if not OPAQUE.match(n)]
    leaked = leaked_names(names, requirements, public, reference)
    return {"ok": bool(names) and not bad and not leaked and declared == len(names),
            "tests": declared, "not_opaque": len(bad), "leaked": len(leaked),
            "duplicates": declared - len(names)}


# ------------------------------------------------------------------ rubric.json shape

def _str_list(v, nonempty=False):
    return isinstance(v, list) and all(isinstance(x, str) for x in v) and (bool(v) or not nonempty)


EVIDENCE_LISTS = ("states", "sequences", "signatures", "boundaries")


def rubric_problems(rubric):
    """Shape errors of a parsed rubric.json, as per-task rejection reasons (never a crash):
    an object; `difficulty` a list of distinct class names; `mutants` exactly m1..m4, each an
    object with `violates` (a non-empty list of R<n>), `kind` and a string `refuted_by`; the
    evidence fields, when present, of their documented types."""
    if not isinstance(rubric, dict):
        return ["rubric.json is not an object"]
    out = []
    t = rubric.get("difficulty")
    if not _str_list(t) or len(set(t)) != len(t) or not set(t) <= set(CLASSES):
        out.append("rubric.json difficulty malformed")
    muts = rubric.get("mutants")
    if not isinstance(muts, dict) or sorted(muts) != list(MUTANTS):
        out.append("rubric.json mutants malformed")
    else:
        for m in MUTANTS:
            e = muts[m]
            if not isinstance(e, dict) or not _str_list(e.get("violates"), True) \
                    or not all(RID.fullmatch(x) for x in e["violates"]) or e.get("kind") not in MUTANT_KINDS \
                    or not isinstance(e.get("refuted_by"), str):
                out.append(f"rubric.json mutant {m} malformed")
    mods = rubric.get("modifiers")
    if mods is not None and not (isinstance(mods, list) and all(
            isinstance(x, dict) and set(x) == {"clause", "modifies"} and all(isinstance(v, str) for v in x.values())
            for x in mods)):
        out.append("rubric.json modifiers malformed")
    for k in EVIDENCE_LISTS:
        if k in rubric and not _str_list(rubric[k]):
            out.append(f"rubric.json {k} malformed")
    sc = rubric.get("scale")
    if sc is not None and not (isinstance(sc, dict) and set(sc) == {"bound", "complexity"}
                               and all(isinstance(v, str) for v in sc.values())):
        out.append("rubric.json scale malformed")
    return out


# ------------------------------------------------------------------ difficulty tags

def _distinct_nonempty(xs):
    return isinstance(xs, list) and bool(xs) and all(isinstance(x, str) and x.strip() for x in xs) \
        and len(set(xs)) == len(xs)


def tag_evidence(requirements, rubric, level, hidden=""):
    """{tag: ok} for every declared tag, from rubric.json's structured evidence:
    - depth: `modifiers` [{"clause": "R9", "modifies": "R4"}, ...]: >= 3 distinct modifying clauses
      at l3/l4 (>= 1 at l1/l2), each existing, distinct from the clause it modifies, whose own
      text names that clause (`R4` as a reference, not inside a word);
    - state: `states` (>= 5 at l4, >= 3 otherwise), distinct, each verbatim in the requirements;
      `sequences`: >= 1 distinct hidden test names (they stay in the sealed rubric);
    - surface: `signatures`, >= 6 distinct public items, each a verbatim substring of the
      requirements that starts with `pub <fn|struct|enum|trait|type|const> <name>`;
    - edges: `boundaries`, >= 1, each >= 3 words and a verbatim substring of the requirements;
    - scale: `scale` {"bound", "complexity"}, both verbatim substrings of the requirements, the
      bound with a number; and a mutant of kind "naive" (the validator times it)."""
    t = rubric.get("difficulty") or []
    try:
        cl = clauses(requirements)
    except ValueError:
        return {k: False for k in t}
    out = {}
    if "depth" in t:
        mods, modifying = rubric.get("modifiers"), set()
        for x in mods if isinstance(mods, list) else []:
            if not (isinstance(x, dict) and isinstance(x.get("clause"), str) and isinstance(x.get("modifies"), str)
                    and RID.fullmatch(x["clause"]) and RID.fullmatch(x["modifies"])):
                modifying = None
                break
            a, b = int(x["clause"][1:]), int(x["modifies"][1:])
            if a not in cl or b not in cl or a == b or b not in _ids(_body(cl[a])):
                modifying = None
                break
            modifying.add(a)
        out["depth"] = modifying is not None and len(modifying) >= (3 if level >= 3 else 1)
    if "state" in t:
        states, seqs = rubric.get("states"), rubric.get("sequences")
        names = test_names(hidden)
        out["state"] = _distinct_nonempty(states) and len(states) >= (5 if level == 4 else 3) \
            and all(s in requirements for s in states) \
            and _distinct_nonempty(seqs) and all(s in names for s in seqs)
    if "surface" in t:
        sigs = rubric.get("signatures")
        items = set()
        ok = _distinct_nonempty(sigs)
        for s in sigs if ok else []:
            m = PUB_ITEM.match(s)
            if not m or s not in requirements:
                ok = False
                break
            items.add(m.group(1))
        out["surface"] = ok and len(items) >= 6
    if "edges" in t:
        b = rubric.get("boundaries")
        out["edges"] = _distinct_nonempty(b) and all(len(x.split()) >= MIN_BOUNDARY_WORDS and x in requirements
                                                     for x in b)
    if "scale" in t:
        sc = rubric.get("scale")
        muts = rubric.get("mutants") or {}
        out["scale"] = isinstance(sc, dict) and isinstance(sc.get("bound"), str) and isinstance(sc.get("complexity"), str) \
            and sc["bound"].strip() != "" and sc["complexity"].strip() != "" \
            and sc["bound"] in requirements and sc["complexity"] in requirements and bool(DIGIT.search(sc["bound"])) \
            and any(isinstance(e, dict) and e.get("kind") == "naive" for e in muts.values())
    return out


def tags(requirements, rubric, level, family_class, hidden=""):
    t = rubric.get("difficulty") if isinstance(rubric, dict) else None
    if not isinstance(t, list) or len(set(t)) != len(t) or not set(t) <= set(CLASSES):
        return {"ok": False, "tags": 0}
    need = 3 if level >= 3 else 2
    ev = tag_evidence(requirements, rubric, level, hidden)
    ok = len(t) >= need and family_class in CLASSES and family_class in t and all(ev.get(k) for k in t)
    return {"ok": ok, "tags": len(t), "evidenced": sum(bool(v) for v in ev.values())}


def clause_floor(requirements, level):
    try:
        n = len(clauses(requirements))
    except ValueError:
        return {"ok": False, "clauses": 0}
    return {"ok": n >= CLAUSE_FLOOR[level], "clauses": n}


def check_task(requirements, hidden, public, reference, rubric, level, family_class):
    """Every mechanical rubric check on one task's texts."""
    r = {"m1": m1(requirements, hidden), "m2": m2(requirements, rubric), "m5": m5(requirements, public, hidden, reference),
         "clauses": clause_floor(requirements, level), "tags": tags(requirements, rubric, level, family_class, hidden)}
    r["ok"] = all(r[k]["ok"] for k in CHECKS)
    return r


def check_task_dir(d, level, family_class):
    """All mechanical rubric checks for one task dir (task.json, tests, reference, rubric.json);
    a malformed task.json or rubric.json raises ValueError."""
    from gymlib import parse_task_json, strict_json_doc
    d = Path(d)
    req = parse_task_json((d / "task.json").read_bytes())["requirements"]
    rubric = strict_json_doc((d / "rubric.json").read_bytes())
    if rubric_problems(rubric):
        raise ValueError("rubric.json malformed")
    return check_task(req, (d / "hidden_tests.rs").read_text(), (d / "public_tests.rs").read_text(),
                      (d / "reference.rs").read_text(), rubric, level, family_class)
