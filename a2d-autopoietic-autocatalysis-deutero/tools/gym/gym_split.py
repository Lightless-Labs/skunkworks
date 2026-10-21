"""Seeded family split for gym v3 (plan 2026-10-21, "Selection discipline" 1-2).

The selected families (FAMILIES.txt, with their rubric classes) are split, stratified by class,
into the dev-holdout (n_holdout families, never piloted, never filtered on any arm's outcome),
dev wave 1 (n_wave1 families) and dev wave 2 (the rest, in a seeded order: wave 2 authors a
prefix sized from the measured wave-1 keep rate). The band rule (keep 1-3 of 4 E007 passes), the
regression share (15-20% of the 4/4 families), the attrition floors and the stopping rule are
registered in the same record, which is written once to runs/c2-gym3/split.json and must be
committed before any pilot call (gym_pilot.py refuses otherwise). Deterministic in (families,
classes, seed). A spare that replaces a family inherits its slot (gymlib.lineage).

usage: python3 -I tools/gym/gym_split.py register --seed N [--holdout 25] [--wave1 12]
           [--regression-share 0.175] [--wave1-floor 4] [--dev-floor 45]
       python3 -I tools/gym/gym_split.py status
"""
import hashlib
import json
import random
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import (CLASSES, GYM, OUT_REPO, RECORDS, emit, gate_or_exit, gateway, lineage, load_json,  # noqa: E402
                    read_ids, save_json, spec_sha, _git)

SPLIT = OUT_REPO / "runs" / "c2-gym3" / "split.json"
PROTOCOL = "gym-v3-split-1"
KEEP_MIN, KEEP_MAX = 1, 3
STOPPING_RULE = ("no-replenishment; families dropped whole; if wave 1 keeps fewer than wave1_min_kept families "
                 "the next wave's rubric is adjusted (never a re-filter); if the kept dev set after wave 2 has "
                 "fewer than dev_min_kept families, stop and report")


def _rng(seed, label):
    return random.Random(hashlib.sha256(f"{PROTOCOL}:{seed}:{label}".encode()).hexdigest())


def _allocate(counts, total, seed, label):
    """Largest-remainder allocation of `total` across classes in proportion to counts; ties in
    the remainder are broken by a seeded order of the classes."""
    n = sum(counts.values())
    if total > n:
        raise ValueError("not enough families")
    exact = {c: total * counts[c] / n if n else 0 for c in counts}
    out = {c: int(exact[c]) for c in counts}
    order = sorted(counts)
    _rng(seed, label).shuffle(order)
    order.sort(key=lambda c: -(exact[c] - out[c]))           # stable: seeded order breaks ties
    for c in order[:total - sum(out.values())]:
        out[c] += 1
    return out


def split(classes, seed, n_holdout=25, n_wave1=12):
    """{"assign": {fid: {"set", "wave", "class"}}, "wave2_order": [...]} for {fid: class}."""
    if set(classes.values()) - set(CLASSES):
        raise ValueError("unknown class")
    by = {c: sorted(f for f, k in classes.items() if k == c) for c in CLASSES}
    for c in CLASSES:
        _rng(seed, f"order:{c}").shuffle(by[c])
    counts = {c: len(by[c]) for c in CLASSES}
    hold = _allocate(counts, n_holdout, seed, "holdout")
    rest = {c: counts[c] - hold[c] for c in CLASSES}
    w1 = _allocate(rest, n_wave1, seed, "wave1")
    assign, wave2 = {}, []
    for c in CLASSES:
        fams = by[c]
        for f in fams[:hold[c]]:
            assign[f] = {"set": "holdout", "wave": None, "class": c}
        for f in fams[hold[c]:hold[c] + w1[c]]:
            assign[f] = {"set": "dev", "wave": 1, "class": c}
        for f in fams[hold[c] + w1[c]:]:
            assign[f] = {"set": "dev", "wave": 2, "class": c}
            wave2.append(f)
    wave2.sort()
    _rng(seed, "wave2").shuffle(wave2)
    return {"assign": dict(sorted(assign.items())), "wave2_order": wave2}


def families_digest(classes):
    return hashlib.sha256(json.dumps(sorted(classes.items())).encode()).hexdigest()


def record(classes, seed, n_holdout, n_wave1, regression_share, wave1_floor, dev_floor):
    if not 0.15 <= regression_share <= 0.20:
        raise ValueError("regression share must be 15-20%")
    s = split(classes, seed, n_holdout, n_wave1)
    return {"protocol": PROTOCOL, "seed": seed, "families_sha256": families_digest(classes),
            "n_holdout": n_holdout, "n_wave1": n_wave1, **s,
            "band": {"keep_min": KEEP_MIN, "keep_max": KEEP_MAX, "of": 4, "regression_share": regression_share},
            "floors": {"wave1_min_kept": wave1_floor, "dev_min_kept": dev_floor},
            "stopping_rule": STOPPING_RULE, "spec_sha256": spec_sha(),
            "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}


def load_split():
    return load_json(SPLIT)


def committed(path=SPLIT):
    """The file is tracked, and identical to HEAD, in the repo that holds it."""
    rel = Path(path).relative_to(OUT_REPO)
    tracked = _git("ls-files", "--error-unmatch", "--", str(rel), repo=OUT_REPO).returncode == 0
    clean = _git("diff", "--quiet", "HEAD", "--", str(rel), repo=OUT_REPO).returncode == 0
    return tracked and clean


def assignment(fid, sp, act=None):
    """The split slot a family holds (a spare holds the slot of the family it replaced)."""
    return sp["assign"].get(lineage(fid, act))


def families_for(sp, act, set_=None, wave=None, count=None):
    """Active families in a split set/wave; for wave 2, the first `count` of the seeded order."""
    out = []
    for f in act["active"]:
        a = assignment(f, sp, act)
        if a is None or (set_ and a["set"] != set_) or (wave is not None and a["wave"] != wave):
            continue
        out.append(f)
    if wave == 2 and count is not None:
        rank = {f: i for i, f in enumerate(sp["wave2_order"])}
        out = sorted(out, key=lambda f: rank[lineage(f, act)])[:count]
    return sorted(out)


@gateway("split")
def main():
    args = sys.argv[1:]
    if args[:1] == ["status"]:
        sp = load_split()
        emit(stage="split", ok=sp is not None, status="committed" if sp and committed() else "uncommitted")
        return
    if args[:1] != ["register"]:
        raise ValueError("usage")
    gate_or_exit("split")

    def opt(name, default, cast):
        return cast(args[args.index(name) + 1]) if name in args else default
    seed = opt("--seed", None, int)
    if seed is None:
        raise ValueError("seed required")
    if SPLIT.exists():
        raise RuntimeError("split exists (write-once)")
    fams = read_ids(GYM / "FAMILIES.txt")
    act = load_json(RECORDS / "active.json")
    classes = {f: act["classes"][f] for f in fams}
    rec = record(classes, seed, opt("--holdout", 25, int), opt("--wave1", 12, int),
                 opt("--regression-share", 0.175, float), opt("--wave1-floor", 4, int), opt("--dev-floor", 45, int))
    save_json(SPLIT, rec)
    sets = [a["set"] for a in rec["assign"].values()]
    emit(stage="split", families=len(sets), n=sets.count("holdout"), count=len(rec["wave2_order"]), status="registered")


if __name__ == "__main__":
    main()
