"""Mechanical disjointness check for confirmation-3 (plan §0.6 step 5; rules frozen in §0.10/§0.11).

Metrics on word 5-gram shingles of each task's `requirements` (lower-cased, non-alphanumerics
collapsed): Jaccard, and containment |A∩B| / min(|A|, |B|) (catches a short task reused
inside a longer one). Corpus: every prior task (repo tasks/ and everything in the holdout
dir except confirmation-3, malformed records fail the run) plus confirmation-3 tasks of
OTHER families. A task conflicts if Jaccard >= 0.15 or containment >= 0.40 against any of
them. Calibrated 2026-10-18 on the repo's 167 visible tasks: max cross-family Jaccard 0.077,
max cross-family containment 0.170 (shared boilerplate); same family at another level:
median Jaccard 0.62, median containment 0.97.

State machine (records/similarity.json), deterministic:
- first conflict of a family -> "reauthor" (c3_author.py --reauthor, its 2nd attempt);
  it stays "reauthor" (pending) until re-authored;
- a successful re-author sets "reauthor-done"; only a check of a suite that holds the
  re-authored version (records/assembled.json) decides it: conflict -> replaced, no
  conflict -> "reauthored". A stale suite leaves it pending;
- a failed re-author is terminal ("reauthor-failed"): the assembly replaces the family;
- conflict with no attempt left -> family replaced by the next spare, or dropped when
  none remain (c3lib.replace_family).
Prints only gateway lines (counts, maximum similarities, opaque ids).

usage: python3 -I tools/holdout/c3_similarity.py
"""
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import (ID_RE, RECORDS, SUITE, DriverLock, emit, gateway, load_json, prior_manifest,  # noqa: E402
                   prior_tasks, replace_family, save_json, task_files)

JACCARD, CONTAIN, K = 0.15, 0.40, 5


def shingles(text):
    words = re.sub(r"[^a-z0-9]+", " ", text.lower()).split()
    return {" ".join(words[i:i + K]) for i in range(max(0, len(words) - K + 1))}


def scores(a, b):
    if not a or not b:
        return 0.0, 0.0
    inter = len(a & b)
    return inter / len(a | b), inter / min(len(a), len(b))


@gateway("similarity")
def main():
    with DriverLock():
        frozen = load_json(RECORDS / "prior_manifest.json")
        if not frozen or frozen != prior_manifest():
            raise ValueError("prior corpus changed since selection")
        prior, counts = prior_tasks()
        corpus = [(fam, shingles(req)) for fam, req in prior]
        mine = []
        for tj in task_files(SUITE):
            d = json.loads(Path(tj).read_text())
            if not ID_RE.match(str(d.get("family", ""))) or len(str(d.get("requirements", "")).split()) < 20:
                raise ValueError("malformed suite task")
            mine.append((d["family"], shingles(d["requirements"])))
        worst_j = worst_c = 0.0
        conflicted = set()
        for fam, sh in mine:
            for ofam, osh in corpus + [m for m in mine if m[0] != fam]:
                j, c = scores(sh, osh)
                worst_j, worst_c = max(worst_j, j), max(worst_c, c)
                if j >= JACCARD or c >= CONTAIN:
                    conflicted.add(fam)
        emit(stage="similarity", tasks=len(mine), n=len(corpus), max_sim=round(worst_j, 2),
             max_contain=round(worst_c, 2), conflicts=len(conflicted))
        state = load_json(RECORDS / "similarity.json", {})
        assembled = load_json(RECORDS / "assembled.json", {})

        def attempts(fam):
            return load_json(RECORDS / f"author-{fam}" / "status.json", {"attempts": 0})["attempts"]

        for fam in sorted(conflicted):
            s = state.get(fam)
            if s in ("reauthor", "reauthor-failed") or (s == "reauthor-done" and assembled.get(fam) != attempts(fam)):
                emit(stage="similarity-conflict", id=fam, status="pending")       # idempotent until resolved
            elif s is None and attempts(fam) < 2:
                state[fam] = "reauthor"
                emit(stage="similarity-conflict", id=fam, status="reauthor")
            else:
                state[fam] = "replaced"
                spare = replace_family(fam, "similarity")
                emit(stage="similarity-conflict", id=fam, status="replaced" if spare else "dropped")
        for fam in sorted(state):
            if state[fam] == "reauthor-done" and fam not in conflicted and assembled.get(fam) == attempts(fam):
                state[fam] = "reauthored"                                         # re-authored version is clean
        save_json(RECORDS / "similarity.json", state)


if __name__ == "__main__":
    main()
