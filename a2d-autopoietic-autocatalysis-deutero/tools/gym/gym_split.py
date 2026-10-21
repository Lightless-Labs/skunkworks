"""Seeded family split and frozen wave membership for gym v3 (plan 2026-10-21, "Selection discipline" 1-2).

register: the selected families (FAMILIES.txt, with their rubric classes) are split, stratified by
  class, into the dev-holdout (n_holdout families, never piloted, never filtered on any arm's
  outcome), dev wave 1 (n_wave1 families) and dev wave 2 (the rest, in a seeded order: wave 2
  authors a prefix sized from the measured wave-1 keep rate). The seed is not chosen by anyone:
  int(sha256(families_sha256 + ":gym-v3-split-2026-10-21"), 16) mod 2**32. Registration is
  refused once any family has an authoring status (no split after any authoring outcome). The
  band rule (keep 1-3 of 4 E007 passes), the regression rule (gym_band.stratum_size), the floors
  (at least 45 retained dev families = 180 tasks, at least 25 surviving holdout families) and the
  stopping rule are registered in the same record, runs/c2-gym3/split.json, written once and
  committed before any pilot call (gym_pilot.py refuses otherwise).

freeze --wave W [--count N]: the wave's membership (its split slots; for wave 2 the first N of the
  seeded order, N chosen from wave 1's measured keep rate, so wave 1 must be finalized first; N may
  be 0) is frozen in runs/c2-gym3/waves.json before any pilot of that wave exists (write-once per
  wave). Every slot is then accounted for by gym_band.py (kept, dropped with a reason, pending).
  A spare that replaces a family holds its slot (gymlib.lineage); once the wave's initial pilot is
  reserved a replacement is a drop (gymlib.replace_family).

usage: python3 -I tools/gym/gym_split.py register [--holdout 25] [--wave1 12] [--regression-share 0.175]
           [--wave1-floor 4]
       python3 -I tools/gym/gym_split.py freeze --wave W [--count N]
       python3 -I tools/gym/gym_split.py status
"""
import hashlib
import json
import random
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import (CLASSES, GYM, OUT_REPO, RECORDS, _cfg, emit, gate_or_exit, gateway, lineage,  # noqa: E402
                    load_json, read_ids, save_json, spec_sha, _git)

SPLIT = OUT_REPO / "runs" / "c2-gym3" / "split.json"
WAVES = OUT_REPO / "runs" / "c2-gym3" / "waves.json"
PROTOCOL = "gym-v3-split-1"
SEED_CONSTANT = ":gym-v3-split-2026-10-21"
KEEP_MIN, KEEP_MAX = 1, 3
# The plan's floors; only a test root may lower them (gymlib._cfg ignores the variables otherwise).
DEV_FLOOR = int(_cfg("GYM_TEST_DEV_FLOOR", "45"))          # retained dev families (x 4 = 180 tasks)
HOLDOUT_FLOOR = int(_cfg("GYM_TEST_HOLDOUT_FLOOR", "25"))  # surviving dev-holdout families
STOPPING_RULE = ("no-replenishment; families dropped whole; replacements only for structural authoring "
                 "failures before the wave's initial pilot (same class, same slot, no inherited pilot data), "
                 "afterwards a drop; one initial pilot per wave, at most one re-pilot per family (void or "
                 "invalidated), else dropped as unknown; if wave 1 keeps fewer than wave1_min_kept families the "
                 "next wave's rubric is adjusted (never a re-filter); final selection needs at least "
                 "dev_min_kept retained dev families and holdout_min surviving holdout families, else "
                 "floors_unmet: stop and report")
REGRESSION_RULE = ("every E007 4/4 family of every wave is a candidate; drawn once at final selection, seeded, "
                   "over the pooled candidates: k = round(share x N) clamped to [ceil(0.15 N), floor(0.20 N)] "
                   "with N the kept dev families; if that range is empty, the nearest feasible integer to "
                   "[0.15 N, 0.20 N]; never more than the candidates; the achieved share is reported")


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


def derived_seed(families_sha256):
    """The split seed, fixed by the families themselves and a constant registered in the plan:
    nobody chooses it, so seeds cannot be tried offline."""
    return int(hashlib.sha256((families_sha256 + SEED_CONSTANT).encode()).hexdigest(), 16) % 2 ** 32


def record(classes, n_holdout, n_wave1, regression_share, wave1_floor):
    if not 0.15 <= regression_share <= 0.20:
        raise ValueError("regression share must be 15-20%")
    digest = families_digest(classes)
    seed = derived_seed(digest)
    s = split(classes, seed, n_holdout, n_wave1)
    return {"protocol": PROTOCOL, "seed": seed,
            "seed_rule": f"int(sha256(families_sha256 + {SEED_CONSTANT!r}), 16) mod 2**32",
            "families_sha256": digest, "n_holdout": n_holdout, "n_wave1": n_wave1, **s,
            "band": {"keep_min": KEEP_MIN, "keep_max": KEEP_MAX, "of": 4, "regression_share": regression_share,
                     "regression_rule": REGRESSION_RULE},
            "floors": {"wave1_min_kept": wave1_floor, "dev_min_kept": DEV_FLOOR, "dev_min_tasks": 4 * DEV_FLOOR,
                       "holdout_min": HOLDOUT_FLOOR},
            "stopping_rule": STOPPING_RULE, "spec_sha256": spec_sha(),
            "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}


def load_split():
    return load_json(SPLIT)


def load_waves():
    return load_json(WAVES, {})


def committed(path=SPLIT):
    """The file is tracked, and identical to HEAD, in the repo that holds it."""
    rel = Path(path).relative_to(OUT_REPO)
    tracked = _git("ls-files", "--error-unmatch", "--", str(rel), repo=OUT_REPO).returncode == 0
    clean = _git("diff", "--quiet", "HEAD", "--", str(rel), repo=OUT_REPO).returncode == 0
    return tracked and clean


def split_sha256():
    return hashlib.sha256(SPLIT.read_bytes()).hexdigest()


def assignment(fid, sp, act=None):
    """The split slot a family holds (a spare holds the slot of the family it replaced)."""
    return sp["assign"].get(lineage(fid, act))


def holder(slot, act):
    """The active family holding a split slot, or None (dropped)."""
    for f in act.get("active", []):
        if lineage(f, act) == slot:
            return f
    return None


def drop_reason(slot, act):
    """Why a slot has no active family: the last exclusion event in its replacement chain."""
    reason, cur = None, slot
    for ev in act.get("events", []):
        if ev.get("family") == cur:
            reason = ev.get("reason")
            if ev.get("replacement"):
                cur = ev["replacement"]
    return reason or "dropped"


def wave_slots(sp, wave, count=None):
    if wave == 1:
        return sorted(f for f, a in sp["assign"].items() if a["set"] == "dev" and a["wave"] == 1)
    if wave == 2:
        if count is None or not 0 <= count <= len(sp["wave2_order"]):
            raise ValueError("wave 2 needs a count within the seeded order")
        return list(sp["wave2_order"][:count])
    raise ValueError("unknown wave")


def families_for(sp, act, set_=None, wave=None, count=None):
    """Active families in a split set/wave; for wave 2, the first `count` of the seeded order (the
    frozen count once wave 2 is frozen)."""
    if wave == 2 and count is None:
        count = (load_waves().get("2") or {}).get("count")
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


def freeze(wave, count=None):
    """Write wave W's membership to the registry, once, before any pilot of it exists."""
    from gym_band import band_path
    sp = load_split()
    if not sp or not committed():
        raise ValueError("split not committed")
    waves = load_waves()
    if str(wave) in waves:
        raise RuntimeError("wave frozen (write-once)")
    reg = load_json(OUT_REPO / "runs" / "c2-gym3" / "replicates.json", {})
    if any(r.get("wave") == wave for r in reg.values()):
        raise RuntimeError("the wave has a pilot: membership can no longer be frozen")
    band1 = load_json(band_path(1)) or {}
    if wave == 2 and not band1.get("finalized"):
        raise RuntimeError("wave 2 is sized from wave 1's finalized band")
    if wave == 1 and count is not None:
        raise ValueError("wave 1 is the split's wave-1 slots")
    slots = wave_slots(sp, wave, count)
    waves[str(wave)] = {"slots": slots, "count": count, "split_sha256": split_sha256(),
                        "frozen_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    if wave == 2:
        waves["2"]["wave1_kept"] = len(band1.get("kept", []))
    save_json(WAVES, waves)
    return waves[str(wave)]


@gateway("split")
def main():
    args = sys.argv[1:]
    if args[:1] == ["status"]:
        sp = load_split()
        emit(stage="split", ok=sp is not None, status="committed" if sp and committed() else "uncommitted")
        return
    gate_or_exit("split")

    def opt(name, default, cast):
        return cast(args[args.index(name) + 1]) if name in args else default
    if args[:1] == ["freeze"]:
        wave = opt("--wave", None, int)
        rec = freeze(wave, opt("--count", None, int))
        emit(stage="split", wave=wave, n=len(rec["slots"]), status="wave-frozen")
        return
    if args[:1] != ["register"]:
        raise ValueError("usage")
    if "--seed" in args:
        emit(stage="split", status="operator-seed-refused")      # the seed is derived, never chosen
        sys.exit(2)
    if SPLIT.exists():
        raise RuntimeError("split exists (write-once)")
    if any(RECORDS.glob("author-*/status.json")):
        emit(stage="split", status="authoring-started")          # no split after any authoring outcome
        sys.exit(2)
    fams = read_ids(GYM / "FAMILIES.txt")
    act = load_json(RECORDS / "active.json")
    classes = {f: act["classes"][f] for f in fams}
    rec = record(classes, opt("--holdout", 25, int), opt("--wave1", 12, int), opt("--regression-share", 0.175, float),
                 opt("--wave1-floor", 4, int))
    save_json(SPLIT, rec)
    sets = [a["set"] for a in rec["assign"].values()]
    emit(stage="split", families=len(sets), n=sets.count("holdout"), count=len(rec["wave2_order"]), status="registered")


if __name__ == "__main__":
    main()
