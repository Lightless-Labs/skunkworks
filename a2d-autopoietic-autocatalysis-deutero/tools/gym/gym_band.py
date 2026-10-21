"""Band selection, final selection and the acceptance check for gym v3 (plan 2026-10-21,
"Selection discipline").

Registry (repo, runs/c2-gym3/; every record content-free: opaque ids, hashes, counts):
  split.json        write-once, committed before any pilot (gym_split.py register)
  waves.json        {"1"|"2": {"slots": [split slot ids], "count", "split_sha256", "frozen_utc"}}:
                    each wave's membership, frozen once before any pilot of it (gym_split.py freeze)
  replicates.json   {run rel: {"seq", "purpose": pilot | repilot | acceptance, "wave", "threshold",
                    "status": reserved -> launched -> complete | failed, "families", "manifest_sha256",
                    ...}}; "reserved" means nothing was launched (a crash before launch); launched,
                    complete and failed are spent. One initial pilot per wave; re-pilots only of
                    families pending as void ("repilot") or "invalidated", at most one per family.
  band-w<W>.json    derived (recomputed, deterministic) from the registry: one entry per frozen slot,
                    {"family", "status": kept | regression-candidate | dropped | pending, "reason",
                    "decision", "run", "content": {task: hash}}; "finalized" when nothing is pending
  selection.json    gym_band.py finalize: kept + the regression stratum, bound to content hashes,
                    with the floors' status (ok | floors_unmet); frozen once the acceptance is spent
  acceptance.json   write-once decision on the acceptance replicate
Sealed (gym root): every pilot's out dir (prompts, submissions, transcripts, full ledger), suite
copy, harness snapshot and manifest (gym_pilot.py); the repo gets only the accounting ledger and a
content-free summary per run.

band --wave W [--run R]: recompute wave W's record from the registry. The authoritative data of a
  family are, deterministically, from the FIRST complete run (registry order) whose manifest binds
  the family's current certified content and whose results decide it (classify: <= 1 void and
  both completions agree): band (1-3 of 4) -> kept, easy (4/4) -> regression-candidate, hard
  (0/4) -> dropped. Data on other content are stale ("invalidated": the family needs a new
  eligible pilot); undecided -> "repilot". A family whose one re-pilot is spent and still has no
  decision is dropped as unknown. A slot with no active family is dropped with its reason.
  Only registered pilot/re-pilot runs of that wave count (never an acceptance or threshold run),
  and each must have been made on the current split (manifest split_sha256).
finalize: every frozen wave finalized (waves 1 and 2 frozen; wave 2 may have count 0); the
  regression stratum is drawn once over the pooled candidates (stratum_size); floors: at least
  dev_min_kept retained dev families (x 4 tasks) and holdout_min surviving holdout families, else
  status floors_unmet (gym_pilot.py --acceptance and gym_export.py export refuse it).
accept --run R: the first fresh E007 replicate of the selection (gym_pilot.py --acceptance), bound
  to the selection's and the current certification's content hashes (any mismatch refuses).
  Decision order: floors_unmet; incomplete (void share > 5%, or the voids counted all as passes
  or all as fails could put the rate on either side of a 35-65% edge); ok (35-65%) or
  adjust-next-wave. The non-void rate is reported with a Wilson 95% interval. Nothing is ever
  re-filtered on this replicate; it is the A/A threshold's first half.

usage: python3 -I tools/gym/gym_band.py band --wave W [--run runs/c2-gym3-pilot-wW/rN]
       python3 -I tools/gym/gym_band.py finalize
       python3 -I tools/gym/gym_band.py accept --run runs/c2-gym3-accept/r1
"""
import hashlib
import math
import random
import sys
import time
from fractions import Fraction
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import (OUT_REPO, RECORDS, RUNS, DriverLock, emit, gate_or_exit, gateway, load_json,  # noqa: E402
                    save_json)
from gymvendor import replicate_tuples  # noqa: E402

DIR = OUT_REPO / "runs" / "c2-gym3"
REGISTRY = DIR / "replicates.json"
SELECTION = DIR / "selection.json"
ACCEPTANCE = DIR / "acceptance.json"
ACCEPT_LO, ACCEPT_HI = Fraction(35, 100), Fraction(65, 100)
MAX_VOID_SHARE = Fraction(5, 100)
ARM = "X:inc"
SPENT = ("launched", "complete", "failed")
MAX_REPILOTS = 1
STATUS = {"band": "kept", "easy": "regression-candidate", "hard": "dropped"}


def band_path(wave):
    return DIR / f"band-w{wave}.json"


def run_name(rel):
    rel = Path(rel)
    return f"{rel.parent.name}-{rel.name}"


def sealed_out(rel):
    """A pilot run's a2d --out dir, under the gym root (never the repo)."""
    return RUNS / f"pilot-{run_name(rel)}"


def manifest_path(rel):
    return RECORDS / "pilots" / f"{run_name(rel)}.manifest.json"


def tids(fam):
    return [f"{fam}-l{lv}" for lv in (1, 2, 3, 4)]


def classify(results, keep_min=1, keep_max=3):
    """results: 4 values True / False / None (void). -> band | easy | hard | repilot."""
    if len(results) != 4:
        raise ValueError("a family has 4 tasks")
    voids = sum(r is None for r in results)
    if voids > 1:
        return "repilot"
    known = sum(1 for r in results if r is True)
    outcomes = set()
    for extra in ((0, 1) if voids else (0,)):
        p = known + extra
        outcomes.add("band" if keep_min <= p <= keep_max else ("easy" if p == 4 else "hard"))
    return outcomes.pop() if len(outcomes) == 1 else "repilot"


def family_results(tuples, fam):
    out = []
    for t in tids(fam):
        v = tuples.get(t)
        out.append(None if v is None or v["void"] else bool(v["pass"]))
    return out


# ------------------------------------------------------------------ regression stratum

def stratum_size(n_kept, n_candidates, share=Fraction(175, 1000)):
    """(k, rule) for the regression stratum: k = round(share x N) (half up) clamped to
    [ceil(0.15 N), floor(0.20 N)]; when that range is empty, the integer nearest to
    [0.15 N, 0.20 N] (the lower on a tie); never more than the candidates. Exact arithmetic."""
    share = Fraction(share).limit_denominator(1000)
    lo_x, hi_x = Fraction(15, 100) * n_kept, Fraction(20, 100) * n_kept
    lo, hi = math.ceil(lo_x), math.floor(hi_x)
    if lo <= hi:
        k, rule = min(max(math.floor(share * n_kept + Fraction(1, 2)), lo), hi), "clamped-15-20"
    else:
        below, above = math.floor(lo_x), math.ceil(hi_x)
        k, rule = (below, "nearest-feasible") if lo_x - below <= above - hi_x else (above, "nearest-feasible")
    if k > n_candidates:
        k, rule = n_candidates, rule + "+candidates-short"
    return k, rule


def draw_regression(candidates, k, seed):
    rng = random.Random(hashlib.sha256(f"gym-v3-regression:{seed}".encode()).hexdigest())
    return sorted(rng.sample(sorted(candidates), k))


# ------------------------------------------------------------------ band decisions

def decide_family(fam, cur, runs, keep):
    """(decision, run rel or None): the first complete run (registry order) whose manifest binds
    the family's current content and whose results decide it."""
    stale = seen = False
    for r in runs:
        if fam not in r["families"]:
            continue
        seen = seen or r["status"] in SPENT
        if r["status"] != "complete":
            continue
        if any(r["content"].get(t) != cur[t] for t in tids(fam)):
            stale = True
            continue
        d = classify(family_results(r["tuples"], fam), *keep)
        if d in STATUS:
            return d, r["rel"]
    return ("invalidated" if stale else ("repilot" if seen else "not-piloted")), None


def decide_wave(slots, act, cert_content, runs, keep, holder, drop_reason):
    """{slot: record} for a frozen wave: every slot accounted for (kept, regression-candidate,
    dropped with a reason, or pending with a reason)."""
    out = {}
    for slot in slots:
        fam = holder(slot, act)
        if fam is None:
            out[slot] = {"family": None, "status": "dropped", "reason": drop_reason(slot, act)}
            continue
        if any(t not in cert_content for t in tids(fam)):
            out[slot] = {"family": fam, "status": "pending", "reason": "not-certified"}
            continue
        cur = {t: cert_content[t] for t in tids(fam)}
        d, rel = decide_family(fam, cur, runs, keep)
        if rel:
            out[slot] = {"family": fam, "status": STATUS[d], "decision": d, "run": rel, "content": cur,
                         "reason": "hard (0/4)" if d == "hard" else None}
            continue
        used = sum(1 for r in runs if r["purpose"] == "repilot" and fam in r["families"] and r["status"] in SPENT)
        busy = any(r["status"] == "launched" and fam in r["families"] for r in runs)
        if d != "not-piloted" and used >= MAX_REPILOTS and not busy:
            out[slot] = {"family": fam, "status": "dropped", "reason": "unknown (re-pilot cap)", "decision": d}
        else:
            out[slot] = {"family": fam, "status": "pending", "reason": d, "repilots": used}
    return out


def load_runs(reg, wave, split_sha):
    """The wave's registered pilot and re-pilot runs in registry order, with their sealed
    manifests and (for complete runs) tuples. A run made on another split is refused."""
    out = []
    for rel, e in sorted(reg.items(), key=lambda kv: kv[1].get("seq", 0)):
        if e.get("wave") != wave or e.get("purpose") not in ("pilot", "repilot") or e.get("threshold"):
            continue
        man = load_json(manifest_path(rel), {}) if e.get("status") in SPENT else {}
        if e.get("status") in SPENT and man.get("split_sha256") != split_sha:
            raise ValueError("a pilot of this wave was made on another split")
        tuples = run_tuples(rel, man) if e.get("status") == "complete" else {}
        out.append({"rel": rel, "purpose": e["purpose"], "status": e.get("status"), "families": e.get("families", []),
                    "content": man.get("content", {}), "tuples": tuples})
    return out


def run_tuples(rel, manifest):
    tasks = sorted(manifest.get("content", {}))
    tuples, _ = replicate_tuples(sealed_out(rel), 0, [ARM], tasks)
    return {task: v for (_, _, task), v in tuples.items()}


def wave_record(wave):
    """The wave's band record recomputed from the registry, or None if the wave is not frozen."""
    from gym_export import certified_current
    from gym_split import committed, drop_reason, holder, load_split, load_waves, split_sha256
    sp = load_split()
    if not sp or not committed():
        raise ValueError("split not committed")
    waves = load_waves()
    if str(wave) not in waves:
        return None
    if waves[str(wave)].get("split_sha256") != split_sha256():
        raise ValueError("the wave was frozen on another split")
    cert = certified_current()
    if not cert:
        raise ValueError("suite not certified")
    act = load_json(RECORDS / "active.json")
    runs = load_runs(load_json(REGISTRY, {}), wave, split_sha256())
    keep = (sp["band"]["keep_min"], sp["band"]["keep_max"])
    slots = decide_wave(waves[str(wave)]["slots"], act, cert["content"], runs, keep, holder, drop_reason)
    by = {s: sorted(r["family"] for r in slots.values() if r["status"] == s and r["family"])
          for s in ("kept", "regression-candidate")}
    pending = sorted(s for s, r in slots.items() if r["status"] == "pending")
    rec = {"wave": wave, "slots": slots, "kept": by["kept"], "candidates": by["regression-candidate"],
           "dropped": sorted(s for s, r in slots.items() if r["status"] == "dropped"), "pending": pending,
           "finalized": not pending, "runs": [r["rel"] for r in runs], "cert_suite": cert["suite"]}
    if wave == 1:
        rec["below_floor"] = len(rec["kept"]) < sp["floors"]["wave1_min_kept"]
    return rec


def band(wave, run_rel=None):
    reg = load_json(REGISTRY, {})
    if run_rel is not None:
        e = reg.get(run_rel)
        if not e or e.get("purpose") not in ("pilot", "repilot") or e.get("threshold") or e.get("wave") != wave \
                or e.get("status") != "complete":
            raise ValueError("not a registered, complete pilot run of this wave")
    rec = wave_record(wave)
    if rec is None:
        raise ValueError("wave not frozen")
    save_json(band_path(wave), rec)
    st = [r["status"] for r in rec["slots"].values()]
    emit(stage="band", wave=wave, families=len(st), kept=st.count("kept"), regression=st.count("regression-candidate"),
         dropped=st.count("dropped"), repilot=sum(1 for r in rec["slots"].values() if r.get("reason") == "repilot"),
         invalidated=sum(1 for r in rec["slots"].values() if r.get("reason") == "invalidated"),
         status="finalized" if rec["finalized"] else "pending")
    return rec


# ------------------------------------------------------------------ final selection

def acceptance_spent(reg=None):
    reg = load_json(REGISTRY, {}) if reg is None else reg
    return any(e.get("purpose") == "acceptance" and e.get("status") in SPENT for e in reg.values())


def finalize():
    from gym_export import certified_current
    from gym_split import DEV_FLOOR, HOLDOUT_FLOOR, holder, load_split, load_waves
    if acceptance_spent():
        raise RuntimeError("the acceptance replicate is spent: the selection is frozen")
    sp, waves = load_split(), load_waves()
    if not all(w in waves for w in ("1", "2")):
        emit(stage="finalize", status="wave-not-frozen")
        return None
    recs = {w: wave_record(int(w)) for w in sorted(waves)}
    for w, rec in recs.items():
        save_json(band_path(int(w)), rec)
    if not all(r["finalized"] for r in recs.values()):
        emit(stage="finalize", status="pending", pending=sum(len(r["pending"]) for r in recs.values()))
        return None
    cert = certified_current()
    act = load_json(RECORDS / "active.json")
    kept = sorted(f for r in recs.values() for f in r["kept"])
    candidates = sorted(f for r in recs.values() for f in r["candidates"])
    k, rule = stratum_size(len(kept), len(candidates), Fraction(str(sp["band"]["regression_share"])))
    regression = draw_regression(candidates, k, sp["seed"])
    retained = sorted(kept + regression)
    holdout = sorted(slot for slot, a in sp["assign"].items() if a["set"] == "holdout")
    surviving = sorted(f for f in (holder(s, act) for s in holdout)
                       if f and all(t in cert["content"] for t in tids(f)))
    dev_min = max(sp["floors"]["dev_min_kept"], DEV_FLOOR)
    hold_min = max(sp["floors"]["holdout_min"], HOLDOUT_FLOOR)
    floors_ok = len(retained) >= dev_min and 4 * len(retained) >= 4 * dev_min and len(surviving) >= hold_min
    sel = {"status": "ok" if floors_ok else "floors_unmet", "kept": kept, "regression": regression,
           "candidates": candidates, "retained": retained, "holdout": surviving,
           "content": {t: cert["content"][t] for f in retained for t in tids(f)},
           "holdout_content": {t: cert["content"][t] for f in surviving for t in tids(f)},
           "regression_k": k, "regression_rule": rule,
           "achieved_share": {"of_kept": round(k / len(kept), 4) if kept else None,
                              "of_candidates": round(k / len(candidates), 4) if candidates else None,
                              "of_retained": round(k / len(retained), 4) if retained else None},
           "floors": {"dev_min_kept": dev_min, "dev_retained": len(retained), "dev_tasks": 4 * len(retained),
                      "holdout_min": hold_min, "holdout_surviving": len(surviving)},
           "cert_suite": cert["suite"], "finalized_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    save_json(SELECTION, sel)
    emit(stage="finalize", kept=len(kept), regression=k, families=len(retained), n=len(surviving),
         status=sel["status"])
    return sel


# ------------------------------------------------------------------ acceptance

def wilson(passed, n, z=1.959963984540054):
    if n <= 0:
        return None
    p = passed / n
    d = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / d
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return [round(centre - half, 4), round(centre + half, 4)]


def acceptance_rate(tuples, planned):
    """Over the PLANNED tasks (a missing tuple is a void): the non-void rate with its Wilson 95%
    interval, and the bounds with every void counted as a fail (lo) and as a pass (hi)."""
    n = len(planned)
    voids = sum(1 for t in planned if t not in tuples or tuples[t]["void"])
    passed = sum(1 for t in planned if t in tuples and not tuples[t]["void"] and tuples[t]["pass"])
    informative = n - voids
    return {"tasks": n, "voids": voids, "passed": passed,
            "rate": round(passed / informative, 4) if informative else None,
            "wilson95": wilson(passed, informative), "lo": round(passed / n, 4) if n else None,
            "hi": round((passed + voids) / n, 4) if n else None}


def acceptance_decision(m):
    """incomplete | ok | adjust-next-wave (exact arithmetic on the counts)."""
    n, v, p = m["tasks"], m["voids"], m["passed"]
    if n == 0 or Fraction(v, n) > MAX_VOID_SHARE:
        return "incomplete"
    lo, hi = Fraction(p, n), Fraction(p + v, n)
    if ACCEPT_LO <= lo and hi <= ACCEPT_HI:
        return "ok"
    if hi < ACCEPT_LO or lo > ACCEPT_HI:
        return "adjust-next-wave"
    return "incomplete"                   # the voids could put the rate on either side of an edge


def accept(run_rel):
    from gym_export import certified_current
    if ACCEPTANCE.exists():
        raise RuntimeError("acceptance decided (write-once)")
    reg = load_json(REGISTRY, {})
    e = reg.get(run_rel)
    if not e or e.get("purpose") != "acceptance" or e.get("status") not in ("complete", "failed"):
        raise ValueError("not a finished acceptance run")
    manifest, sel, cert = load_json(manifest_path(run_rel), {}), load_json(SELECTION, {}), certified_current()
    if not cert or manifest.get("content") != sel.get("content") \
            or any(cert["content"].get(t) != h for t, h in sel.get("content", {}).items()):
        raise RuntimeError("acceptance content does not match the selection and the current certification")
    planned = sorted(manifest["content"])
    tuples = run_tuples(run_rel, manifest) if (sealed_out(run_rel) / "ledger.jsonl").exists() else {}
    m = acceptance_rate(tuples, planned)
    decision = "floors_unmet" if sel.get("status") != "ok" else acceptance_decision(m)
    m.update({"run": run_rel, "run_status": e["status"], "decision": decision,
              "note": "never a re-filter on this replicate; it is the A/A threshold's first half",
              "decided_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})
    save_json(ACCEPTANCE, m)
    emit(stage="accept", tasks=m["tasks"], voids=m["voids"], rate=m["rate"], decision=decision)
    return m


@gateway("band")
def main():
    args = sys.argv[1:]
    gate_or_exit("band")
    with DriverLock():
        if args[:1] == ["band"]:
            band(int(args[args.index("--wave") + 1]), args[args.index("--run") + 1] if "--run" in args else None)
        elif args[:1] == ["finalize"]:
            if finalize() is None:
                sys.exit(1)
        elif args[:1] == ["accept"]:
            accept(args[args.index("--run") + 1])
        else:
            raise ValueError("usage")


if __name__ == "__main__":
    main()
