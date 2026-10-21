#!/usr/bin/env python3
"""Phase B, descriptive only: join the blinded idea-category labels with each round's registered decisions.

Pre-specified 2026-10-21 ~21:00 UTC, before round 4 reported (LOG "C2-PB label join"). Inputs:
runs/c2-pc/idea-labels.json (labels assigned blind, before any round-4 outcome) and
runs/c2-pc/round<r>/scores.json (the decisions c2_phaseb.py `round` registered) and
runs/c2-pc/round<r>/<arm>/candidate.py. Output: runs/c2-pc/labels-join.json with
- per proposal: round, arm, blinded id, category, candidate sha256, decision, delta_allpass,
  delta_frac, valid (None for every outcome field while a round has no scores.json);
- per arm: the categories in round order and how many are distinct;
- the categories both arms proposed, and cross-arm repeats: a proposal whose category (or exact
  candidate bytes) an earlier proposal of the other arm already had;
- per category: proposals and mean delta_allpass over the scored, valid ones.
Eight proposals: no test, no interval, no claim. The registered endpoints are c2_phaseb.py `analyze`.
"""
import hashlib
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
PC = ROOT / "runs" / "c2-pc"
ARMS = {"informed": "inf", "uninformed": "uninf"}


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def join(labels, rounds_dir):
    proposals = []
    for key, lab in sorted(labels["by_round_arm"].items(), key=lambda kv: (int(kv[0][1:].split("-")[0]), kv[0])):
        r, arm = key[1:].split("-", 1)
        r = int(r)
        rdir = rounds_dir / f"round{r}"
        scores = rdir / "scores.json"
        decision = json.loads(scores.read_text())["decisions"][ARMS[arm]] if scores.is_file() else {}
        proposals.append({"round": r, "arm": arm, "id": lab["id"], "category": lab["category"],
                          "candidate_sha256": sha256(rdir / arm / "candidate.py"),
                          **{k: decision.get(k) for k in ("decision", "delta_allpass", "delta_frac", "valid")}})
    proposals.sort(key=lambda p: (p["round"], p["arm"]))
    arms = {}
    for arm in ARMS:
        cats = [p["category"] for p in proposals if p["arm"] == arm]
        arms[arm] = {"categories": cats, "distinct": len(set(cats))}
    repeats = []
    for p in proposals:
        for q in proposals:
            if q["arm"] != p["arm"] and q["round"] < p["round"]:
                same_bytes = p["candidate_sha256"] is not None and p["candidate_sha256"] == q["candidate_sha256"]
                if same_bytes or q["category"] == p["category"]:
                    repeats.append({"proposal": f"r{p['round']}-{p['arm']}", "repeats": f"r{q['round']}-{q['arm']}",
                                    "category": p["category"], "identical_candidate": same_bytes})
    categories = {}
    for cat in sorted({p["category"] for p in proposals}):
        mine = [p for p in proposals if p["category"] == cat]
        scored = [p["delta_allpass"] for p in mine if p["valid"] and p["delta_allpass"] is not None]
        categories[cat] = {"proposals": [f"r{p['round']}-{p['arm']}" for p in mine],
                           "mean_delta_allpass": sum(scored) / len(scored) if scored else None}
    return {"method": "descriptive join of blinded labels and registered round decisions; no inference (8 proposals)",
            "labels_packet_sha256": labels.get("packet_sha256"),
            "rounds_scored": sorted({p["round"] for p in proposals if p["decision"] is not None}),
            "proposals": proposals, "arms": arms,
            "shared_categories": sorted(set(arms["informed"]["categories"]) & set(arms["uninformed"]["categories"])),
            "cross_arm_repeats": repeats, "categories": categories}


def main():
    data = join(json.loads((PC / "idea-labels.json").read_text()), PC)
    out = PC / "labels-join.json"
    out.write_text(json.dumps(data, indent=2) + "\n")
    print(json.dumps({"rounds_scored": data["rounds_scored"], "arms": data["arms"],
                      "shared_categories": data["shared_categories"],
                      "cross_arm_repeats": data["cross_arm_repeats"]}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
