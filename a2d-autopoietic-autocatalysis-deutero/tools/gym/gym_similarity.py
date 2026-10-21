"""Mechanical disjointness check for gym v3 (copy of tools/holdout/c3_similarity.py, adapted).

Metrics on word 5-gram shingles of each task's `requirements`: Jaccard and containment
|A∩B| / min(|A|, |B|). Corpus: the read-only prior corpus (gymlib.PriorCorpus: repo tasks/ and
the holdout sets; confirmation-3 joins only once its decision.json exists) plus the gym tasks of
OTHER families. Conflict if Jaccard >= 0.15 or containment >= 0.40 (thresholds calibrated for
confirmation-3 on 2026-10-18). The prior corpus's base manifest is frozen at selection.

State machine (records/similarity.json), as confirmation-3's: first conflict -> "reauthor"
(gym_author.py --reauthor, the 2nd attempt); "reauthor-done" is decided only on a suite that
holds the re-authored version; a failed re-author is terminal; a conflict with no attempt left
replaces the family by a same-class spare (it inherits the split slot) or drops it.
Prints only gateway lines.

usage: python3 -I tools/gym/gym_similarity.py
"""
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import (ID_RE, RECORDS, SUITE, DriverLock, PriorCorpus, emit, gate_or_exit, gateway,  # noqa: E402
                    load_json, replace_family, save_json, suite_digest, task_files)

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
    gate_or_exit("similarity")
    with DriverLock():
        corpus_view = PriorCorpus()
        frozen = load_json(RECORDS / "prior_manifest.json")
        if not frozen or frozen != corpus_view.manifest():
            raise ValueError("prior corpus changed since selection")
        prior, counts = corpus_view.tasks()
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
                emit(stage="similarity-conflict", id=fam, status="pending")
            elif s is None and attempts(fam) < 2:
                state[fam] = "reauthor"
                emit(stage="similarity-conflict", id=fam, status="reauthor")
            else:
                state[fam] = "replaced"
                spare = replace_family(fam, "similarity")
                emit(stage="similarity-conflict", id=fam, status="replaced" if spare else "dropped")
        for fam in sorted(state):
            if state[fam] == "reauthor-done" and fam not in conflicted and assembled.get(fam) == attempts(fam):
                state[fam] = "reauthored"
        save_json(RECORDS / "similarity.json", state)
        save_json(RECORDS / "similarity-last.json", {"suite": suite_digest(), "conflicts": len(conflicted),
                                                      "confirmation_3": bool(corpus_view.c3)})


if __name__ == "__main__":
    main()
