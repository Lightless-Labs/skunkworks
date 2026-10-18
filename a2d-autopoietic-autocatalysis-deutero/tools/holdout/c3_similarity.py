"""Mechanical disjointness check for confirmation-3 (plan §0.6 step 5; rule frozen in §0.10).

Metric: Jaccard similarity of word 5-gram shingles of a task's `requirements` text
(lower-cased, non-alphanumerics collapsed to spaces). Corpus: every task.json in the
repo's tasks/ tree and in the holdout sets confirmation-1, confirmation-2, dev-holdout
and c1-validation. Also compared: confirmation-3 tasks of DIFFERENT families with each
other. A task conflicts if its maximum similarity to any corpus or other-family task is
>= THRESHOLD. Prints only the overall maximum (2 dp), the conflict count and the opaque
ids of families with a conflict.

Conflict handling (frozen before authoring): a conflicting family is re-authored once
(new job, same brief, told only "too similar to an existing task; change the domain
details"); if it conflicts again it is replaced by the next spare or excluded before
the hash.

usage: python3 -I tools/holdout/c3_similarity.py
"""
import json
import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import HOLDOUT, SUITE, emit, gateway  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
# Calibrated 2026-10-18 on the repo's 167 visible tasks: max cross-family similarity per
# task median 0.023, p95 0.044, max 0.077; same family at another level median 0.62.
# 0.15 is about twice the highest cross-family value seen.
THRESHOLD = 0.15
K = 5


def shingles(text):
    words = re.sub(r"[^a-z0-9]+", " ", text.lower()).split()
    return {" ".join(words[i:i + K]) for i in range(max(0, len(words) - K + 1))}


def jaccard(a, b):
    if not a or not b:
        return 0.0
    return len(a & b) / len(a | b)


def task_files(root):
    """Every task.json under root, following symlinked task dirs (train-all is symlinks)."""
    seen, out = set(), []
    for dirpath, _dirs, files in os.walk(root, followlinks=True):
        if "task.json" in files:
            p = Path(dirpath) / "task.json"
            real = p.resolve()
            if real not in seen:
                seen.add(real)
                out.append(p)
    return sorted(out)


def load(root):
    out = []
    for tj in task_files(root):
        try:
            d = json.loads(tj.read_text())
        except (OSError, ValueError):
            continue
        out.append((d.get("family", ""), shingles(d.get("requirements", ""))))
    return out


@gateway("similarity")
def main():
    corpus = load(REPO / "tasks")
    for s in ("confirmation-1", "confirmation-2", "dev-holdout", "c1-validation"):
        corpus += load(HOLDOUT / s)
    mine = load(SUITE)
    worst, conflicts, fams = 0.0, 0, set()
    for i, (fam, sh) in enumerate(mine):
        others = corpus + [m for m in mine if m[0] != fam]
        best = max((jaccard(sh, o[1]) for o in others), default=0.0)
        worst = max(worst, best)
        if best >= THRESHOLD:
            conflicts += 1
            fams.add(fam)
    emit(stage="similarity", tasks=len(mine), n=len(corpus), max_sim=round(worst, 2), conflicts=conflicts)
    for f in sorted(fams):
        emit(stage="similarity-conflict", id=f)


if __name__ == "__main__":
    main()
