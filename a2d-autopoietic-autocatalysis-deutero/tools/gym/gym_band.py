"""Band selection and the acceptance check for gym v3 (plan 2026-10-21, "Selection discipline").

band: from one E007 pilot of a dev wave (gym_pilot.py), decide each family once:
  - only dev families of that wave; a dev-holdout family in the pilot is refused (the holdout
    is never piloted or filtered);
  - only pilot runs: a run registered as an A/A-threshold or acceptance replicate is refused;
  - pilot data bind the certified content: a family repaired since its pilot is "invalidated"
    and must be re-piloted;
  - voids are unknown: more than 1 void in a family's 4 tasks means "repilot"; with 1 void the
    family is decided only if both completions give the same decision (else "repilot");
  - keep a family with KEEP_MIN-KEEP_MAX (1-3) of 4 passes ("band"); 4/4 is "easy", 0/4 "hard";
  - once no family of the wave is pending, a seeded share (registered, 15-20%) of the easy
    families is drawn into the labelled regression stratum and kept; the other easy and all
    hard families are dropped whole. There is no replenishment.
  Decisions are written once per family to runs/c2-gym3/band-w<W>.json.
accept: the first fresh E007 replicate of the kept dev set (gym_pilot.py --acceptance): the
  all-pass rate must be 35-65%. Outside, the NEXT wave is adjusted; nothing is re-filtered on it.
  The replicate is the A/A threshold's first half, so no selection may ever use it.

usage: python3 -I tools/gym/gym_band.py band --wave W --run runs/c2-gym3-pilot-wW/rN
       python3 -I tools/gym/gym_band.py accept --run runs/c2-gym3-accept/r1
"""
import hashlib
import random
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import OUT_REPO, RECORDS, emit, gate_or_exit, gateway, load_json, save_json  # noqa: E402
from gymvendor import replicate_tuples  # noqa: E402

DIR = OUT_REPO / "runs" / "c2-gym3"
REGISTRY = DIR / "replicates.json"
ACCEPT_LO, ACCEPT_HI = 0.35, 0.65
ARM = "X:inc"


def band_path(wave):
    return DIR / f"band-w{wave}.json"


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


def regression_stratum(easy, share, seed, wave):
    """A seeded round(share * n) of the easy (4/4) families, kept and labelled."""
    if not 0.15 <= share <= 0.20:
        raise ValueError("regression share must be 15-20%")
    pool = sorted(easy)
    k = int(round(share * len(pool)))
    rng = random.Random(hashlib.sha256(f"gym-v3-regression:{seed}:{wave}".encode()).hexdigest())
    return sorted(rng.sample(pool, k))


def family_results(tuples, fam):
    out = []
    for lv in (1, 2, 3, 4):
        v = tuples.get(f"{fam}-l{lv}")
        out.append(None if v is None or v["void"] else bool(v["pass"]))
    return out


def decide(split, act, wave, manifest, tuples, certified, assignment):
    """{family: decision} for the families of one pilot. Raises on a holdout or off-wave family."""
    out = {}
    for fam in manifest["families"]:
        a = assignment(fam, split, act)
        if not a or a["set"] != "dev":
            raise ValueError("a non-dev family in a pilot")
        if a["wave"] != wave:
            raise ValueError("a family from another wave")
        tids = [f"{fam}-l{lv}" for lv in (1, 2, 3, 4)]
        if any(manifest["content"].get(t) != certified["content"].get(t) for t in tids):
            out[fam] = "invalidated"
            continue
        out[fam] = classify(family_results(tuples, fam), split["band"]["keep_min"], split["band"]["keep_max"])
    return out


def run_tuples(run_dir, manifest):
    tasks = sorted(manifest["content"])
    tuples, _ = replicate_tuples(run_dir, 0, [ARM], tasks)
    return {task: v for (_, _, task), v in tuples.items()}


def registered(run_rel):
    return load_json(REGISTRY, {}).get(run_rel)


def band(wave, run_rel):
    from gym_export import certified_current
    from gym_split import assignment, committed, load_split
    sp = load_split()
    if not sp or not committed():
        raise ValueError("split not committed")
    reg = registered(run_rel)
    if not reg or reg.get("purpose") != "pilot" or reg.get("threshold") or reg.get("wave") != wave \
            or reg.get("status") != "complete":
        raise ValueError("not a completed pilot run of this wave")
    manifest = load_json(OUT_REPO / f"{run_rel}.manifest.json")
    cert = certified_current()
    if not cert:
        raise ValueError("suite not certified")
    act = load_json(RECORDS / "active.json")
    tuples = run_tuples(OUT_REPO / run_rel, manifest)
    rec = load_json(band_path(wave), {"wave": wave, "decisions": {}, "runs": [], "finalized": False})
    if rec["finalized"]:
        raise RuntimeError("band decided (write-once)")
    new = decide(sp, act, wave, manifest, tuples, cert, assignment)
    for fam, d in new.items():
        prior = rec["decisions"].get(fam)
        if prior in ("band", "easy", "hard"):
            continue                                       # a decision is final
        rec["decisions"][fam] = d
    rec["runs"].append(run_rel)
    pending = [f for f, d in rec["decisions"].items() if d in ("repilot", "invalidated")]
    if not pending:
        easy = [f for f, d in rec["decisions"].items() if d == "easy"]
        reg_fams = regression_stratum(easy, sp["band"]["regression_share"], sp["seed"], wave)
        kept = sorted(f for f, d in rec["decisions"].items() if d == "band")
        rec.update({"kept": kept, "regression": reg_fams,
                    "dropped": sorted(f for f, d in rec["decisions"].items()
                                      if d == "hard" or (d == "easy" and f not in reg_fams)),
                    "finalized": True, "finalized_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})
        floor = sp["floors"]["wave1_min_kept"] if wave == 1 else None
        rec["below_floor"] = floor is not None and len(kept) < floor
    save_json(band_path(wave), rec)
    d = list(rec["decisions"].values())
    emit(stage="band", wave=wave, families=len(d), kept=d.count("band"), regression=len(rec.get("regression", [])),
         dropped=len(rec.get("dropped", [])), repilot=d.count("repilot"), invalidated=d.count("invalidated"),
         status="finalized" if rec["finalized"] else "pending")
    return rec


def kept_dev(split_waves):
    """Every finalized wave's kept + regression families (the acceptance replicate's suite)."""
    out = []
    for w in split_waves:
        rec = load_json(band_path(w))
        if not rec or not rec.get("finalized"):
            raise ValueError("a wave is not finalized")
        out += rec["kept"] + rec["regression"]
    return sorted(out)


def acceptance_rate(tuples):
    vals = [v for v in tuples.values()]
    informative = [v for v in vals if not v["void"]]
    passed = sum(1 for v in informative if v["pass"])
    rate = passed / len(informative) if informative else None
    return {"tasks": len(vals), "voids": len(vals) - len(informative), "passed": passed, "rate": rate,
            "in_band": rate is not None and ACCEPT_LO <= rate <= ACCEPT_HI}


def accept(run_rel):
    path = DIR / "acceptance.json"
    if path.exists():
        raise RuntimeError("acceptance decided (write-once)")
    reg = registered(run_rel)
    if not reg or reg.get("purpose") != "acceptance" or reg.get("status") != "complete":
        raise ValueError("not a completed acceptance run")
    manifest = load_json(OUT_REPO / f"{run_rel}.manifest.json")
    m = acceptance_rate(run_tuples(OUT_REPO / run_rel, manifest))
    m.update({"run": run_rel, "decision": "ok" if m["in_band"] else "adjust-next-wave",
              "note": "never a re-filter on this replicate; it is the A/A threshold's first half",
              "decided_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})
    save_json(path, m)
    emit(stage="accept", tasks=m["tasks"], voids=m["voids"], rate=m["rate"], in_band=m["in_band"],
         decision=m["decision"])
    return m


@gateway("band")
def main():
    args = sys.argv[1:]
    gate_or_exit("band")
    run_rel = args[args.index("--run") + 1]
    if args[:1] == ["band"]:
        band(int(args[args.index("--wave") + 1]), run_rel)
    elif args[:1] == ["accept"]:
        accept(run_rel)
    else:
        raise ValueError("usage")


if __name__ == "__main__":
    main()
