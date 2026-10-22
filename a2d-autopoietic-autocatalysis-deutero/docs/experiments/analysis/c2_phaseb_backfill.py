#!/usr/bin/env python3
"""Phase B, bookkeeping only: restore the uninformed rows that nano/results.tsv dropped.

c2_phaseb.py `round` logs one row per arm, both with id PC-<r> and stage "round".
eval.log_result de-duplicates on (id, stage, decision) without the proposer, so when
both arms reached the same decision (all four rounds: screen-fail) the uninformed row
was silently dropped (found 2026-10-22). eval.py is frozen until confirmation-3's
decision, so this script rebuilds the missing rows with the functions `round` used,
from each round's registered scores.json, and appends them directly.

Check: the informed rows are rebuilt the same way and must equal the logged ones
field for field; nothing is written otherwise. Rows keep the registered id scheme
(PC-<r>, proposer pc-uninformed) and the experiment's date; spend fields are per
round, as `round` logs them, so they repeat the informed row's. The informed brief
never showed pc-uninformed rows (c2_phaseb.information), so what either arm saw is
unchanged, and the registered endpoints come from scores.json, not from this file.
Idempotent: a round whose uninformed row is present is skipped.
"""
import csv
import importlib.util
import io
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("phaseb", ROOT / "docs/experiments/analysis/c2_phaseb.py")
pb = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pb)
ev = pb.ev
RESULTS = ROOT / "nano/results.tsv"
SPEND = ("invocations", "calls_by_lane", "tokens")
NOTE = "backfilled 2026-10-22 (c2_phaseb_backfill.py): eval.log_result's (id, stage, decision) key dropped this row"


def rebuild(r, arm, informed_row, threshold):
    base = pb.round_dir(r)
    scores = pb.read_json(base / "scores.json")
    name = pb.ARM_NAMES[arm]
    decided = scores["decisions"][name]
    if not decided["valid"]:
        raise SystemExit(f"round {r} {arm}: invalid proposals are scored by incumbent substitution; not rebuilt here")
    table = {(s["arm"], s["task"]): (s["fraction"], s["all_passed"], s["family"]) for s in scores["scores"]}
    rows = pb.paired_table(table, "X:" + name, "X:inc")
    net = sum(row[2] for row in rows)
    if ("screen-pass" if net >= 2 else "screen-fail") != decided["decision"] or net != decided["net_allpass"]:
        raise SystemExit(f"round {r} {arm}: rebuilt decision differs from scores.json")
    return {"id": f"PC-{r}", "date": informed_row["date"], "proposer": "pc-" + arm, "stage": "round",
            "hypothesis": pb.read_json(base / arm / "candidate.json").get("hypothesis", ""),
            "delta_frac": ev.mean(rows, 1), "se": ev.cluster_se(rows, 1),
            "delta_allpass": ev.mean(rows, 2), "se_allpass": ev.cluster_se(rows, 2),
            "threshold": threshold, "n_tasks": len(rows), "n_families": len({row[0] for row in rows}),
            "decision": decided["decision"], **{k: informed_row[k] for k in SPEND},
            "candidate_sha": ev.sha(base / arm / "candidate.py"), "note": ""}


def line(row):
    return "\t".join(str(row.get(k, "")).replace("\t", " ") for k in ev.FIELDS) + "\n"


def main():
    text = RESULTS.read_text()
    logged = list(csv.DictReader(io.StringIO(text), delimiter="\t"))
    threshold = pb.read_json(ROOT / "nano/aa.json")["threshold_allpass"]
    rounds = [r for r in range(1, 7) if (pb.round_dir(r) / "scores.json").exists()]
    new, checked = [], []
    for r in rounds:
        def find(proposer):
            hits = [row for row in logged if row["id"] == f"PC-{r}" and row["stage"] == "round"
                    and row["proposer"] == proposer]
            if len(hits) > 1:
                raise SystemExit(f"round {r}: {len(hits)} {proposer} rows")
            return hits[0] if hits else None
        informed = find("pc-informed")
        if informed is None:
            raise SystemExit(f"round {r}: no informed row to check the reconstruction against")
        again = rebuild(r, "informed", informed, threshold)
        diff = [k for k in ev.FIELDS if k != "note" and str(again[k]).replace("\t", " ") != informed[k]]
        if diff:
            raise SystemExit(f"round {r}: rebuilt informed row differs from the logged one in {diff}")
        checked.append(r)
        if find("pc-uninformed") is None:
            new.append({**rebuild(r, "uninformed", informed, threshold), "note": NOTE})
    if new:
        if not text.endswith("\n"):
            raise SystemExit("results.tsv does not end with a newline")
        with open(RESULTS, "a") as f:
            f.writelines(line(row) for row in new)
    print({"informed_rows_reproduced": checked,
           "appended": [(row["id"], row["proposer"], row["decision"], round(row["delta_allpass"], 4)) for row in new]})
    return 0


if __name__ == "__main__":
    sys.exit(main())
