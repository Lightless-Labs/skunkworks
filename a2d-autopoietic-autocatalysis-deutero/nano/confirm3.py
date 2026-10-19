"""Confirmation-3 scoring rules (docs/plans/2026-10-18-confirmation-3.md §0.4, §0.10, §0.13).

Stdlib only and self-contained, so the registration can bind this file by sha256.
The claim is the controller's, re-applied to its own pooled comparison numbers (the
whole-family bootstrap on complete (task, replicate) tuples). Then:
- a (task, arm, replicate) tuple is void if its submission is void or unscored, if it
  has a tool event, or if any invocation lacks a transcript or ran another codex version;
- SUPPORTED also needs the worst-case imputed point lift >= +10 pp vs D and B
  (void X = fail, void D/B = pass), over every planned (task, replicate);
- RULED OUT also needs the best-case imputed point lift vs D < +10 pp
  (void X = pass, void D = fail);
- run level: a hidden-test name hit or a replicate with tool events in > 0.5% of its
  invocations -> IntegrityReview; void tuples > 5% of all tuples, or the controller's
  validity guard -> Invalid. A single tool event voids its tuple only (§0.4), so the
  controller's own any-tool-call IntegrityReview is not used.
Outputs are counts and rates only: no task ids, test names or model text.
"""
import json
import pathlib
import re

MIN_EFFECT = 0.10
EPS = 1e-9                      # the controller's BOUNDARY_EPS
TOOL_RATE_MAX = 0.005
VOID_SHARE_MAX = 0.05
X = "X:frozen"
COMPARATORS = ("D", "B")
BANNER = re.compile(r"^OpenAI Codex v(\S+)")


def records(out):
    return [json.loads(line) for line in (pathlib.Path(out) / "ledger.jsonl").read_text().splitlines() if line.strip()]


def transcript_ok(out, prompt_path, pinned):
    """The invocation's transcript exists under this out dir and was written by the pinned codex."""
    parts = pathlib.PurePath(prompt_path).parts
    if "invocations" not in parts or not parts[-1].endswith(".prompt.txt"):
        return False
    t = pathlib.Path(out).joinpath(*parts[parts.index("invocations"):-1], parts[-1][:-len(".prompt.txt")] + ".transcript.txt")
    if not t.is_file():
        return False
    with open(t, errors="replace") as f:
        # Log noise can precede the banner (as in confirmation-2's web audit).
        for line in (next(f, "") for _ in range(20)):
            m = BANNER.match(line)
            if m:
                return m.group(1) == pinned
    return False


def replicate_tuples(out, rep, pinned):
    """({(rep, arm, task): {"pass": bool | None, "void": bool}} for every planned (arm, task),
    {"invocations": n, "tool_calls": n}) for one replicate's out dir."""
    recs = records(out)
    reg = next(r for r in recs if r.get("kind") == "registration")
    tuples = {(rep, arm, task): {"pass": None, "void": False} for arm in reg["arms"] for task in reg["tasks"]}
    stats = {"invocations": 0, "tool_calls": 0}
    for r in recs:
        key = (rep, r.get("arm"), r.get("task"))
        kind = r.get("kind")
        if kind == "submission":
            stats["invocations"] += r.get("invocations") or 0
            stats["tool_calls"] += r.get("tool_calls") or 0
        if key not in tuples:
            continue
        if kind == "submission" and (r.get("void_reason") or (r.get("tool_calls") or 0) > 0):
            tuples[key]["void"] = True
        elif kind == "score":
            tuples[key]["pass"] = bool(r["result"]["all_passed"])
        elif kind == "invocation" and not transcript_ok(out, r.get("prompt_path", ""), pinned):
            tuples[key]["void"] = True
    for v in tuples.values():
        if v["pass"] is None:
            v["void"] = True
    return tuples, stats


def imputed_lift(tuples, comp, case):
    """Point all-pass lift of X over comp on every planned (replicate, task), voids imputed
    worst-case (X fail, comp pass) or best-case (X pass, comp fail)."""
    keys = sorted({(rep, task) for rep, _, task in tuples})
    worst = case == "worst"
    xs = ys = 0
    for rep, task in keys:
        x, y = tuples[(rep, X, task)], tuples[(rep, comp, task)]
        xs += (not worst) if x["void"] else bool(x["pass"])
        ys += worst if y["void"] else bool(y["pass"])
    return (xs - ys) / len(keys) if keys else 0.0


def classify(c, min_families):
    """crates/a2d-lift/src/stats.rs classify_guarded, on a pooled comparison record."""
    if c["families"] < min_families or (c["discordant_treatment_only"] == 0 and c["discordant_comparator_only"] == 0):
        return "Inconclusive"
    lo, hi = c["ci95"]
    if lo > EPS and c["lift"] >= MIN_EFFECT - EPS:
        return "Supported"
    if hi < MIN_EFFECT - EPS:
        return "RuledOut"
    return "Inconclusive"


def decide(summary, tuples, rep_stats):
    """The confirmation-3 outcome from the pooled controller summary and the tuples."""
    classes = {}
    for comp in COMPARATORS:
        c = next((c for c in summary["comparisons"] if c["treatment"] == X and c["comparator"] == comp), None)
        classes[comp] = classify(c, summary["min_families"]) if c else "Invalid"
    worst = {comp: round(imputed_lift(tuples, comp, "worst"), 4) for comp in COMPARATORS}
    best_d = round(imputed_lift(tuples, "D", "best"), 4)
    void_share = sum(v["void"] for v in tuples.values()) / len(tuples) if tuples else 1.0
    tool_rates = {rep: (s["tool_calls"] / s["invocations"] if s["invocations"] else 1.0) for rep, s in rep_stats.items()}
    integrity = bool(summary.get("hidden_name_hits")) or any(r > TOOL_RATE_MAX + EPS for r in tool_rates.values())
    invalid = (void_share > VOID_SHARE_MAX + EPS or summary["excluded_share"] > summary["max_excluded_share"] + 1e-12
               or summary.get("aborted") or "Invalid" in classes.values())
    if integrity:
        overall = "IntegrityReview"
    elif invalid:
        overall = "Invalid"
    elif all(c == "Supported" for c in classes.values()):
        overall = "SUPPORTED" if min(worst.values()) >= MIN_EFFECT - EPS else "INCONCLUSIVE (worst-case void imputation)"
    elif classes["D"] == "RuledOut":
        overall = "RULED OUT" if best_d < MIN_EFFECT - EPS else "INCONCLUSIVE (best-case void imputation)"
    else:
        overall = "INCONCLUSIVE"
    return {"overall": overall, "classes": classes, "worst_case_void_lifts": worst, "best_case_void_lift_vs_D": best_d,
            "void_tuples": sum(v["void"] for v in tuples.values()), "tuples": len(tuples),
            "void_share": round(void_share, 4), "tool_event_rates": {str(k): round(v, 5) for k, v in tool_rates.items()},
            "hidden_name_hits": len(summary.get("hidden_name_hits") or []), "controller_claim": summary["claim"]}
