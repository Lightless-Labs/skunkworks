"""Confirmation-3: scoring rules and the replicate runner (docs/plans/2026-10-18-confirmation-3.md
§0.4, §0.10, §0.13; early reviews 2026-10-19).

usage: python3 -I nano/confirm3.py generate --rep N --registered-at <sha> [--relaunch]
       python3 -I nano/confirm3.py manifest --registered-at <sha>
       python3 -I nano/confirm3.py score --registered-at <sha>

Stdlib only and self-contained, so the registration commit binds this file by sha256.

Decision (decide):
- A (task, arm, replicate) tuple is void if its submission is void or unscored, if it has a
  tool event, if any invocation lacks a --json transcript, or if its invocation records do
  not match its submission's count. All tuples of a replicate whose codex fingerprint changed
  are void. Planned tuples come from the registration.
- The controller's own Invalid or IntegrityReview dominates, as do: a hidden-test name hit or
  tool events in > 0.5% of a replicate's invocations (IntegrityReview); void tuples > 5% of all
  tuples, the controller's excluded-share guard, an aborted generation (Invalid).
- SUPPORTED needs, vs both D and B: the controller rule on complete tuples (ci_low > 0, lift >=
  +10 pp, >= 10 families, discordance), the worst-case imputed point lift >= +10 pp (void X =
  fail, void D/B = pass) and the worst-case imputed whole-family bootstrap ci_low > 0.
- RULED OUT needs X vs D RuledOut on complete tuples and the best-case imputed (void X = pass,
  void D = fail) bootstrap ci_high < +10 pp.
Outputs are counts and rates only: no task ids, test names or model text.
"""
import argparse
import datetime
import fcntl
import hashlib
import json
import math
import os
import pathlib
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

MIN_EFFECT = 0.10
EPS = 1e-9                      # the controller's BOUNDARY_EPS
TOOL_RATE_MAX = 0.005
VOID_SHARE_MAX = 0.05
X = "X:frozen"
COMPARATORS = ("D", "B")
CODEX_STDERR_MARKER = "\n--- codex stderr ---\n"     # a2d-providers: --json events, marker, stderr
# The provider's allowlist detector (crates/a2d-providers/src/cli.rs `codex_tool_events`),
# ported line for line so that the bound runner recounts every transcript itself.
ALLOWED_ITEMS = ("agent_message", "reasoning")
ALLOWED_EVENTS = ("thread.started", "turn.started", "turn.completed", "item.started", "item.updated", "item.completed")
CODEX_HOST_OFF_NOTICE = ("Code Mode is unavailable because code-mode host is disabled. Code mode will fail closed; "
                         "enable `features.code_mode_host` and install `codex-code-mode-host`.")
BOOTSTRAP_ITERATIONS = 10_000                         # the controller's
BOOTSTRAP_SEED = 0x5EED_A2D0
MASK64 = (1 << 64) - 1


def lines(text):
    """Rust's str::lines(): split on \\n only, dropping one trailing \\r. Python's splitlines() also
    splits on U+2028, U+2029, U+0085 and other separators that serde_json leaves raw inside strings."""
    parts = text.split("\n")
    if parts and parts[-1] == "":
        parts.pop()
    return [p[:-1] if p.endswith("\r") else p for p in parts]


def _reject_constant(name):
    raise ValueError(f"{name} is not JSON")       # serde_json rejects NaN and Infinity


def strict_json(line):
    """json.loads as strict as serde_json: no NaN/Infinity; deep nesting is unparsable."""
    try:
        return json.loads(line, parse_constant=_reject_constant)
    except RecursionError as e:
        raise ValueError("nesting too deep") from e


def records_tolerant(out):
    """(parsable records, count of unparsable lines): a dead run's ledger may end mid-line."""
    recs, bad = [], 0
    path = pathlib.Path(out) / "ledger.jsonl"
    for line in (lines(path.read_text(errors="replace")) if path.exists() else []):
        if not line.strip():
            continue
        try:
            rec = strict_json(line)
        except ValueError:
            bad += 1
            continue
        if isinstance(rec, dict):
            recs.append(rec)
        else:
            bad += 1
    return recs, bad


def _events(transcript):
    if CODEX_STDERR_MARKER in transcript:
        return transcript.split(CODEX_STDERR_MARKER, 1)[0]
    return transcript if transcript.lstrip().startswith("{") else None


def py_tool_events(transcript, failed=False):
    """Tool events in a --json transcript by the provider's rules: every item outside the
    allowlist, every unknown event type, every unparsable line, and a missing turn.completed
    (unless the call failed) count one; one pre-turn host-off notice is exempt (plan §0.16).
    A legacy text transcript counts as one event (fail closed)."""
    events = _events(transcript)
    if events is None:
        return 1
    if failed and not events.strip():
        return 0
    count, completed, turn_started, notice_seen = 0, False, False, False
    for line in (x for x in lines(events) if x.strip()):
        try:
            e = strict_json(line)
        except ValueError:
            count += 1
            continue
        kind = e.get("type") if isinstance(e, dict) else None
        if kind not in ALLOWED_EVENTS:
            count += 1
            continue
        completed = completed or kind == "turn.completed"
        turn_started = turn_started or kind == "turn.started"
        if kind.startswith("item."):
            item = e.get("item") if isinstance(e.get("item"), dict) else {}
            if (kind == "item.completed" and item.get("type") == "error" and item.get("message") == CODEX_HOST_OFF_NOTICE
                    and not turn_started and not notice_seen):
                notice_seen = True
                continue
            if item.get("type") not in ALLOWED_ITEMS:
                count += 1
    return count + (0 if completed or failed else 1)


def host_off(transcript):
    """A call whose turn started shows the host-off notice before it: code_mode_host (MCP) was off."""
    events = _events(transcript) or ""
    seen = False
    for line in (x for x in lines(events) if x.strip()):
        try:
            e = strict_json(line)
        except ValueError:
            continue
        if not isinstance(e, dict):
            continue
        if e.get("type") == "turn.started":
            return seen
        item = e.get("item") if isinstance(e.get("item"), dict) else {}
        if e.get("type") == "item.completed" and item.get("type") == "error" and item.get("message") == CODEX_HOST_OFF_NOTICE:
            seen = True
    return True


def transcript_text(out, prompt_path):
    """The invocation's transcript text under this out dir, or None."""
    if not isinstance(prompt_path, str):
        return None
    parts = pathlib.PurePath(prompt_path).parts
    if "invocations" not in parts or not parts[-1].endswith(".prompt.txt"):
        return None
    t = pathlib.Path(out).joinpath(*parts[parts.index("invocations"):-1], parts[-1][:-len(".prompt.txt")] + ".transcript.txt")
    if not t.is_file() or t.is_symlink():
        return None
    return t.read_text(errors="replace")


def transcript_ok(out, prompt_path):
    """The invocation's transcript exists under this out dir in the merged provider's --json
    format (events, marker, stderr). A legacy text transcript means another provider: void.
    The codex version is pinned by bracketing the replicate (no banner under --json)."""
    if not isinstance(prompt_path, str):
        return False
    parts = pathlib.PurePath(prompt_path).parts
    if "invocations" not in parts or not parts[-1].endswith(".prompt.txt"):
        return False
    t = pathlib.Path(out).joinpath(*parts[parts.index("invocations"):-1], parts[-1][:-len(".prompt.txt")] + ".transcript.txt")
    if not t.is_file() or t.is_symlink():
        return False
    with open(t, errors="replace") as f:
        return CODEX_STDERR_MARKER in f.read()


def test_names(source):
    """The controller's `validate::test_names`, ported line for line (#[test] functions)."""
    names, armed = set(), False
    for line in (raw.strip() for raw in lines(source)):
        if line.startswith("#[test]") and line[len("#[test]"):].lstrip().startswith("fn "):
            names.add(re.split(r"[(< ]", line[len("#[test]"):].lstrip()[3:], maxsplit=1)[0])
            continue
        if line == "#[test]":
            armed = True
        elif armed and line.startswith("fn "):
            names.add(re.split(r"[(< ]", line[3:], maxsplit=1)[0])
            armed = False
        elif armed and not line.startswith("#[") and line:
            armed = False
    return names


def failed_attempt_audit(out, tasks):
    """A failed first attempt's integrity evidence, kept after a relaunch (Sol registration
    review, 10-21): every recorded invocation and its tool events; an invocation without an
    auditable --json transcript counts as one tool event (fail closed); and the controller's
    hidden-test-name scan of the attempt's prompts. Counts only."""
    out = pathlib.Path(out)
    invocations = tools = submission_tools = py_tools = 0
    n = lambda v: v if isinstance(v, int) and not isinstance(v, bool) and v >= 0 else 0  # noqa: E731
    recs, unaudited = records_tolerant(out)      # a dead run's cut-off line: unauditable (fail closed)
    if out.exists() and not (out / "ledger.jsonl").exists():
        unaudited += 1                           # no ledger: call coverage cannot be established
    recorded = set()
    key = lambda r: (r.get("arm"), r.get("task"), r.get("replicate"), r.get("index"))  # noqa: E731
    started = [key(r) for r in recs if r.get("kind") == "invocation_started"]
    finished = {key(r) for r in recs if r.get("kind") == "invocation"}
    unaudited += sum(1 for k in started if k not in finished)   # in flight when the run died
    for r in recs:
        if r.get("kind") == "submission":
            submission_tools += n(r.get("tool_calls"))
        elif r.get("kind") == "invocation":
            invocations += 1
            tools += n(r.get("tool_calls"))
            path = r.get("prompt_path")
            if isinstance(path, str) and "invocations" in pathlib.PurePath(path).parts:
                parts = pathlib.PurePath(path).parts
                recorded.add(pathlib.PurePath(*parts[parts.index("invocations"):]))
            text = transcript_text(out, path)
            if text is None or CODEX_STDERR_MARKER not in text:
                unaudited += 1
            else:
                py_tools += py_tool_events(text, failed=r.get("ok") is False)
    # A prompt file without its invocation record was a call the ledger never recorded.
    for prompt in (out / "invocations").rglob("*.prompt.txt") if (out / "invocations").exists() else []:
        if pathlib.PurePath(*prompt.relative_to(out).parts) not in recorded:
            unaudited += 1
    hits = 0
    for task in tasks:
        names = (test_names((SUITE / task / "hidden_tests.rs").read_text())
                 - test_names((SUITE / task / "public_tests.rs").read_text()))
        if not names:
            continue
        pattern = re.compile(r"(?<![A-Za-z0-9_])(?:" + "|".join(map(re.escape, sorted(names))) + r")(?![A-Za-z0-9_])")
        for prompt in sorted((out / "invocations").glob(f"*/{task}/**/*.prompt.txt")):
            if pattern.search(prompt.read_text(errors="replace")):
                hits += 1
    return {"invocations": invocations, "tool_calls": max(tools, submission_tools, py_tools) + unaudited,
            "unaudited": unaudited, "hidden_name_hits": hits}


def load_summary_obj(text):
    """A controller summary from its JSON text; anything unparsable is marked malformed (Invalid)."""
    try:
        obj = json.loads(text)
    except (ValueError, TypeError):
        return {"__malformed__": True}
    return obj if isinstance(obj, dict) else {"__malformed__": True}


def fold_replicate_summary(summary, own):
    """Fold a replicate's own summary into the pooled one: its hidden-name hits and integrity flag.
    A malformed replicate summary, or hits that are not a list, block the claim (fail closed)."""
    summary = dict(summary) if isinstance(summary, dict) else {"__malformed__": True}
    hits = list(summary.get("hidden_name_hits") or []) if isinstance(summary.get("hidden_name_hits") or [], list) \
        else ["malformed pooled hits"]
    if not isinstance(own, dict):
        summary["integrity_ok"] = False
    else:
        own_hits = own.get("hidden_name_hits") or []
        hits += list(own_hits) if isinstance(own_hits, list) else ["malformed replicate hits"]
        if "integrity_ok" in own and own["integrity_ok"] is not True:
            summary["integrity_ok"] = False
    summary["hidden_name_hits"] = hits
    return summary


def merge_failed_attempt(summary, stats, rep, audit):
    """Fold a failed attempt's audit into replicate `rep`'s counts and the run's hidden-name hits."""
    stats[rep]["invocations"] += audit["invocations"]
    stats[rep]["tool_calls"] += audit["tool_calls"]
    summary = dict(summary) if isinstance(summary, dict) else {"__malformed__": True}
    summary["hidden_name_hits"] = list(summary.get("hidden_name_hits") or []) + \
        [f"failed attempt r{rep}"] * audit["hidden_name_hits"]
    if audit["unaudited"]:
        summary["integrity_ok"] = False     # an unauditable call could hide any number of events (Sol re-check)
    if audit["tool_calls"] > TOOL_RATE_MAX * audit["invocations"] + EPS:
        summary["integrity_ok"] = False     # the failed attempt is gated on its own rate too (Claude re-check)
    return summary


def replicate_tuples(out, rep, arms, tasks):
    """({(rep, arm, task): {"pass": bool | None, "void": bool}} for every planned (arm, task),
    {"invocations": n, "tool_calls": n}) for one replicate's out dir. The plan (arms, tasks)
    comes from the registration; a ledger planned otherwise voids every tuple."""
    recs, bad = records_tolerant(out)
    led = next((r for r in recs if r.get("kind") == "registration"), {})
    tuples = {(rep, arm, task): {"pass": None, "void": False} for arm in arms for task in tasks}
    stats = {"invocations": 0, "tool_calls": 0}
    planned = list(led.get("arms") or [])
    if bad or len(planned) != len(set(planned)) or set(planned) != set(arms) or sorted(led.get("tasks") or []) != sorted(tasks):
        for v in tuples.values():
            v["void"] = True
    claimed, seen = {}, {}
    count = lambda v: v if isinstance(v, int) and not isinstance(v, bool) and v >= 0 else None  # noqa: E731
    submission_tools = invocation_tools = own_tools = 0
    for r in recs:
        key = (rep, r.get("arm"), r.get("task"))
        kind = r.get("kind")
        if kind == "submission":
            stats["invocations"] += count(r.get("invocations")) or 0
            submission_tools += count(r.get("tool_calls")) or 0
        elif kind == "invocation":
            # Every call's own tool events count, a lane call's included (fresh-Claude review B3).
            invocation_tools += count(r.get("tool_calls")) or 0
        if key not in tuples:
            continue
        # A malformed field voids its tuple (fail closed); it never crashes the scoring.
        if kind == "submission":
            n, tools = count(r.get("invocations")), count(r.get("tool_calls") or 0)
            claimed[key] = claimed.get(key, 0) + (n or 0)
            if n is None or tools is None or r.get("void_reason") or tools > 0:
                tuples[key]["void"] = True
        elif kind == "score":
            result = r.get("result")
            if isinstance(result, dict) and isinstance(result.get("all_passed"), bool):
                tuples[key]["pass"] = result["all_passed"]
            else:
                tuples[key]["void"] = True
        elif kind == "invocation":
            seen[key] = seen.get(key, 0) + 1
            tools = count(r.get("tool_calls") or 0)
            text = transcript_text(out, r.get("prompt_path"))
            # The runner's own recount (independent of the binary) and the host-off proof (§0.16).
            own = py_tool_events(text, failed=r.get("ok") is False) if text and CODEX_STDERR_MARKER in text else None
            if own is not None:
                own_tools += own
            if tools is None or tools > 0 or own is None or own > 0 or not host_off(text):
                tuples[key]["void"] = True
    stats["tool_calls"] = max(submission_tools, invocation_tools, own_tools)
    for key, v in tuples.items():
        if v["pass"] is None or claimed.get(key, 0) != seen.get(key, 0):
            v["void"] = True
    return tuples, stats


def lane_usage(out):
    """Counts only: calls and tokens per (arm, provider) from a replicate's invocation records."""
    usage = {}
    for r in records_tolerant(out)[0]:
        if r.get("kind") != "invocation":
            continue
        u = usage.setdefault(str(r.get("arm")), {}).setdefault(str(r.get("provider")), {"calls": 0, "tokens": 0})
        u["calls"] += 1
        t = r.get("usage") if isinstance(r.get("usage"), dict) else {}
        u["tokens"] += sum(v for k, v in t.items() if k in ("prompt_tokens", "completion_tokens") and isinstance(v, int))
    return usage


def family(task):
    return task.rsplit("-l", 1)[0]


def imputed(tuples, comp, case):
    """[(family, x_pass, y_pass)] over every planned (replicate, task) in sorted order, voids
    imputed worst-case (X fail, comp pass) or best-case (X pass, comp fail)."""
    worst = case == "worst"
    obs = []
    for rep, task in sorted({(rep, task) for rep, _, task in tuples}):
        x, y = tuples[(rep, X, task)], tuples[(rep, comp, task)]
        obs.append((family(task), (not worst) if x["void"] else bool(x["pass"]),
                    worst if y["void"] else bool(y["pass"])))
    return obs


def bootstrap(obs, iterations=BOOTSTRAP_ITERATIONS, seed=BOOTSTRAP_SEED):
    """(lift, ci_low, ci_high): crates/a2d-lift/src/stats.rs paired_summary's whole-family
    percentile bootstrap (SplitMix64, families in first-observation order)."""
    ids, clusters = {}, []
    for fam, xp, yp in obs:
        if fam not in ids:
            ids[fam] = len(clusters)
            clusters.append([0, 0, 0])
        c = clusters[ids[fam]]
        c[0] += 1
        c[1] += xp
        c[2] += yp
    n = len(obs)
    lift = (sum(c[1] for c in clusters) - sum(c[2] for c in clusters)) / n if n else 0.0
    if len(clusters) < 2 or iterations == 0:
        return lift, lift, lift
    state, k, samples = seed & MASK64, len(clusters), []
    for _ in range(iterations):
        size = sx = sy = 0
        for _ in range(k):
            state = (state + 0x9E3779B97F4A7C15) & MASK64
            z = state
            z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
            z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
            z ^= z >> 31
            count, cx, cy = clusters[z % k]
            size += count
            sx += cx
            sy += cy
        samples.append(sx / size - sy / size)
    samples.sort()
    last = iterations - 1
    return lift, samples[int(0.025 * last)], samples[_ceil(0.975 * last)]


def _ceil(x):
    i = int(x)
    return i if i == x else i + 1


def well_formed(c):
    """A pooled comparison record has the fields classify() reads, with sane types."""
    num = lambda v: isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)  # noqa: E731
    count = lambda v: isinstance(v, int) and not isinstance(v, bool) and v >= 0  # noqa: E731
    ci = c.get("ci95") if isinstance(c, dict) else None
    return (isinstance(c, dict) and count(c.get("families")) and count(c.get("discordant_treatment_only"))
            and count(c.get("discordant_comparator_only")) and num(c.get("lift"))
            and isinstance(ci, (list, tuple)) and len(ci) == 2 and all(num(v) for v in ci))


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


def decide(summary, tuples, rep_stats, iterations=BOOTSTRAP_ITERATIONS):
    """The confirmation-3 outcome from the pooled controller summary and the tuples."""
    finite = lambda v: isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)  # noqa: E731
    share = lambda v: finite(v) and 0 <= v <= 1  # noqa: E731
    mf = summary.get("min_families") if isinstance(summary, dict) else None
    malformed = not (isinstance(summary, dict) and summary.get("__malformed__") is not True
                     and isinstance(summary.get("comparisons"), list)
                     and share(summary.get("excluded_share")) and share(summary.get("max_excluded_share"))
                     and isinstance(mf, int) and not isinstance(mf, bool) and mf >= 1
                     and isinstance(summary.get("hidden_name_hits") or [], list))
    if malformed:                                   # a malformed summary is Invalid, never a crash
        summary = {"comparisons": [], "hidden_name_hits": (summary.get("hidden_name_hits") if isinstance(summary, dict)
                   and isinstance(summary.get("hidden_name_hits"), list) else []), "excluded_share": 1.0,
                   "max_excluded_share": 0.0, "min_families": 10, "aborted": True,
                   "integrity_ok": summary.get("integrity_ok") if isinstance(summary, dict) else None}
    comparisons, classes = {}, {}
    for comp in COMPARATORS:
        c = next((c for c in (summary.get("comparisons") or []) if isinstance(c, dict)
                  and c.get("treatment") == X and c.get("comparator") == comp), None)
        comparisons[comp] = c
        min_families = summary.get("min_families")
        classes[comp] = (classify(c, min_families) if c and well_formed(c) and isinstance(min_families, int)
                         else "Invalid")
    worst = {comp: bootstrap(imputed(tuples, comp, "worst"), iterations) for comp in COMPARATORS}
    best_d = bootstrap(imputed(tuples, "D", "best"), iterations)
    voids = sum(v["void"] for v in tuples.values())
    void_share = voids / len(tuples) if tuples else 1.0
    tool_rates = {rep: (s["tool_calls"] / s["invocations"] if s["invocations"] else 1.0) for rep, s in rep_stats.items()}
    controller_classes = [c.get("class") for c in comparisons.values() if c]
    integrity = (bool(summary.get("hidden_name_hits")) or any(r > TOOL_RATE_MAX + EPS for r in tool_rates.values())
                 or summary.get("claim") == "IntegrityReview" or summary.get("integrity_ok") is False
                 or "IntegrityReview" in controller_classes)
    invalid = (void_share > VOID_SHARE_MAX + EPS
               or summary.get("excluded_share", 1.0) > summary.get("max_excluded_share", 0.0) + 1e-12
               or bool(summary.get("aborted")) or summary.get("claim") == "Invalid" or "Invalid" in controller_classes
               or "Invalid" in classes.values())
    if integrity:
        overall = "IntegrityReview"
    elif invalid:
        overall = "Invalid"
    elif all(c == "Supported" for c in classes.values()) and all(
            (comparisons[comp] or {}).get("class") == "Supported" for comp in COMPARATORS):
        ok = all(lift >= MIN_EFFECT - EPS and lo > EPS for lift, lo, _ in worst.values())
        overall = "SUPPORTED" if ok else "INCONCLUSIVE (worst-case void imputation)"
    elif classes["D"] == "RuledOut" and (comparisons["D"] or {}).get("class") == "RuledOut":
        overall = ("RULED OUT" if best_d[0] < MIN_EFFECT - EPS and best_d[2] < MIN_EFFECT - EPS
                   else "INCONCLUSIVE (best-case void imputation)")
    else:
        overall = "INCONCLUSIVE"
    r4 = lambda t: [round(v, 4) for v in t]  # noqa: E731 (display only; decisions use the unrounded values)
    return {"overall": overall, "classes": classes,
            "worst_case_void": {comp: dict(zip(("lift", "ci_low", "ci_high"), r4(v))) for comp, v in worst.items()},
            "best_case_void_vs_D": dict(zip(("lift", "ci_low", "ci_high"), r4(best_d))),
            "void_tuples": voids, "tuples": len(tuples), "void_share": round(void_share, 4),
            "tool_event_rates": {str(k): round(v, 5) for k, v in tool_rates.items()},
            "hidden_name_hits": len(summary.get("hidden_name_hits") or []), "controller_claim": summary.get("claim")}


# ------------------------------------------------------------------------------------ runner
# Generation (one replicate per call), manifests and scoring, under one exclusive lock. A gate
# that defers records the deferral and writes nothing else; the attempt record is reserved
# atomically before the replicate's first model call.
ROOT = pathlib.Path(__file__).resolve().parent.parent
HOLDOUT = pathlib.Path.home() / ".a2d-holdout"
SUITE = HOLDOUT / "confirmation-3"
DIR = ROOT / "runs" / "c2-confirm3"
A2D = str(ROOT / "target" / "debug" / "a2d")
LUNA = "codex/gpt-6-luna"
DAILY_CAP = 3000
EARLIEST = datetime.date(2026, 10, 24)
START = datetime.time(0, 5)                       # replicates start at or after 00:05 UTC
REGISTRATION = "runs/c2-confirm3/registration.json"
BOUND = (REGISTRATION, "runs/c2-confirm3/frozen.py", "nano/confirm3.py")
C2_SEALED = ROOT / "runs" / "c2-confirm2" / "sealed-sha256.txt"
LATENCY_PROBES = 3
LATENCY_MAX_MEDIAN_SECS = 45.0
CODEX_HOMES = HOLDOUT / "codex-homes"            # A2D_CODEX_HOME_ROOT: per-call homes under the holdout dir
CANARY = HOLDOUT / "canary"
AMBIENT = (".codex", "AGENTS.md", "AGENTS.override.md", ".agents", "skills")
MACHO = (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe")   # 64-bit Mach-O, universal


def emit(**kw):
    print(json.dumps(kw), flush=True)


def git(*a):
    r = subprocess.run(["git", "-C", str(ROOT), *a], capture_output=True, text=True)
    return r.returncode, r.stdout


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def now():
    return datetime.datetime.now(datetime.timezone.utc)


def today():
    return now().date()


def utc_date(ms):
    return datetime.datetime.fromtimestamp(ms / 1000, datetime.timezone.utc).date()


def commit_day(sha):
    return utc_date(int(git("show", "-s", "--format=%ct", sha)[1].strip()) * 1000)


def out_dir(rep, relaunch=False):
    return HOLDOUT / "runs" / (f"c2-confirm3-r{rep}" + ("-relaunch1" if relaunch else ""))


def suffix(relaunch):
    return "-relaunch1" if relaunch else ""


def complete(out):
    """Generation finished and was not aborted."""
    if not (pathlib.Path(out) / "ledger.jsonl").exists():
        return False
    return any(r.get("kind") == "generation_complete" and r.get("aborted") is False for r in records_tolerant(out)[0])


def generation_day(out):
    return utc_date(next(r for r in records_tolerant(out)[0] if r.get("kind") == "generation_complete")["ts_ms"])


def registration_commit():
    """The one commit that added registration.json: the only valid --registered-at."""
    code, out = git("log", "--diff-filter=A", "--format=%H", "--", REGISTRATION)
    shas = out.split()
    if code or len(shas) != 1:
        sys.exit("registration.json must have been added by exactly one commit")
    return shas[0]


def registration(registered_at):
    """The registration commit binds registration.json, the frozen candidate and this file;
    the controller binds crates/ itself (--registered-at)."""
    sha = registration_commit()
    if git("rev-parse", "--verify", f"{registered_at}^{{commit}}")[1].strip() != sha:
        sys.exit("--registered-at is not the commit that added registration.json")
    if git("merge-base", "--is-ancestor", sha, "HEAD")[0]:
        sys.exit(f"{sha} is not on HEAD's history")
    for rel in BOUND:
        code, then = git("show", f"{sha}:./{rel}")
        if code or then != (ROOT / rel).read_text():
            sys.exit(f"{rel} differs from the registration commit")
    if git("merge-base", "--is-ancestor", sha, "origin/main")[0]:
        sys.exit("push the registration commit first")
    reg = json.loads((DIR / "registration.json").read_text())
    if sha256(DIR / "frozen.py") != reg["frozen_sha"]:
        sys.exit("the frozen candidate does not match registration.json")
    if git("merge-base", "--is-ancestor", reg["provider_commit"], sha)[0]:
        sys.exit("the registered provider commit is not in the registration's history")
    return reg


def suite_hash_now():
    """`a2d lift hash` of the suite with the current controller ("" on failure)."""
    r = subprocess.run([A2D, "lift", "hash", str(SUITE)], capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else ""


def crates_clean(sha):
    """crates/, Cargo.toml and Cargo.lock equal the registration commit, with nothing untracked
    there (the controller's --registered-at binding, checked before an attempt is reserved)."""
    paths = ["crates", "Cargo.toml", "Cargo.lock"]
    return (git("diff", "--quiet", sha, "--", *paths)[0] == 0
            and not git("ls-files", "--others", "--exclude-standard", "--", *paths)[1].strip())


def build_controller():
    """Build a2d from the checked, registration-bound tree (a no-op when current): a stale
    binary from an older provider cannot run or score. Returns its sha256."""
    with open(DIR / "build.log", "a") as log:
        code = subprocess.run(["cargo", "build", "--locked", "--bin", "a2d"], cwd=ROOT, stdout=log,
                              stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL).returncode
    if code:
        sys.exit(f"cargo build failed ({code})")
    return sha256(A2D)


def check_attempts(sha):
    """Every attempt so far ran under this registration."""
    for p in sorted(DIR.glob("attempt-r*.json")):
        if json.loads(p.read_text()).get("registered_at") != sha:
            sys.exit(f"{p.name} ran under another registration commit")


def spend_gate(day, registered_day, c2_sealed, rep, rep1_day):
    """§0.10 spend day: None means go, otherwise the reason to defer."""
    if day < EARLIEST:
        return "before 2026-10-24"
    if day <= registered_day:
        return "registration day"
    if not c2_sealed:
        return "confirmation-2 decision missing"
    if rep == 2 and (rep1_day is None or day <= rep1_day):
        return "replicate 1's day"
    return None


def deferred_today(rep, day):
    """A cap or latency no-go moves the start to the next day's 00:05 UTC (§0.10). A probe group
    started today without a recorded go (interrupted) holds the day too."""
    path, probes = DIR / "deferrals.jsonl", DIR / "probes.jsonl"
    if path.exists() and any(json.loads(x).get("rep") == rep and json.loads(x).get("utc_day") == str(day)
                             for x in path.read_text().splitlines() if x.strip()):
        return True
    if not probes.exists():
        return False
    groups = {}
    for x in probes.read_text().splitlines():
        if x.strip():
            r = json.loads(x)
            if r.get("rep") == rep and r.get("utc_day") == str(day):
                groups[r["group"]] = groups.get(r["group"]) or r.get("state") == "go"
    return any(not ok for ok in groups.values())


def record_probe(rep, day, group, state, **kw):
    with open(DIR / "probes.jsonl", "a") as f:
        f.write(json.dumps({"rep": rep, "utc_day": str(day), "group": group, "state": state, **kw}) + "\n")
        f.flush()
        os.fsync(f.fileno())


def record_deferral(rep, day, reason, **kw):
    with open(DIR / "deferrals.jsonl", "a") as f:
        f.write(json.dumps({"rep": rep, "utc_day": str(day), "reason": reason, **kw}) + "\n")


def latency_go(secs):
    """Pre-registered go/no-go: median of the probe calls (a failed probe is infinite)."""
    return statistics.median(secs) <= LATENCY_MAX_MEDIAN_SECS


def lane(spec):
    """'codex/<model>:<effort>' -> ('codex/<model>', '<effort>')."""
    model, _, effort = spec.partition(":")
    return model, effort or "low"


def probe_suite(d, secret_path, n):
    """n one-line tasks that ask for the content of a canary file: a model without tools
    cannot read it, so the secret must appear in no response."""
    for i in range(1, n + 1):
        t = d / f"c3probe-l{i}"
        t.mkdir(parents=True)
        (t / "task.json").write_text(json.dumps({
            "id": t.name, "family": "c3probe", "test_timeout_secs": 10,
            "requirements": "R1. Implement in Rust as a SINGLE source file that compiles with rustc --edition 2024, std "
                            f"only. R2. pub fn canary() -> String returns the exact contents of the file {secret_path}."}))
        (t / "public_tests.rs").write_text("#[test]\nfn returns_a_string() { let _ = canary(); }\n")
        (t / "hidden_tests.rs").write_text("#[test]\nfn returns_a_string() { let _ = canary(); }\n")
        (t / "reference.rs").write_text("pub fn canary() -> String { String::new() }\n")


def probe_run(model, effort, n, tag, lock_fd=None):
    """§0.10 preflight through the real provider (its exact flags, a holdout-local codex home):
    n canary tasks, arm D, k = 1, generation only. The run lives under holdout/runs/c2-*, so its
    invocations count toward the daily cap from its ledger, and it inherits the runner lock.
    Returns (secs, leak, tool_calls, audited): audited = calls that succeeded with a --json
    transcript (failed or unaudited calls count as infinite latency)."""
    out = HOLDOUT / "runs" / f"c2-confirm3-probe-{tag}"
    suite = HOLDOUT / "runs" / f"c2-confirm3-probe-{tag}-suite"
    secret = f"C3-CANARY-{os.urandom(8).hex()}"
    CANARY.mkdir(parents=True, exist_ok=True)
    CODEX_HOMES.mkdir(parents=True, exist_ok=True)
    secret_path = CANARY / f"{tag}.txt"
    secret_path.write_text(secret + "\n")
    os.chmod(secret_path, 0o600)
    probe_suite(suite, secret_path, n)
    cmd = [A2D, "lift", "run", "--experiment", f"C3-PROBE-{tag}", "--unregistered", "--suite", str(suite),
           "--model", model, "--effort", effort, "--arms", "D", "--k", "1", "--jobs", str(n), "--min-families", "1",
           "--defer-scoring", "--max-invocations", str(n + 2), "--max-wall-clock-secs", "1800", "--out", str(out)]
    with open(str(out) + ".log", "w") as log:
        subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                       pass_fds=(lock_fd,) if lock_fd is not None else (),
                       env={**os.environ, "A2D_CODEX_HOME_ROOT": str(CODEX_HOMES)})
    secs, leak, tools, audited = [], False, 0, 0
    recs, bad = records_tolerant(out)
    tools += bad                                              # an unparsable line could be anything
    for r in recs:
        if r.get("kind") == "invocation":
            text = transcript_text(out, r.get("prompt_path"))
            own = py_tool_events(text) if text and CODEX_STDERR_MARKER in text else None
            tools += own or 0
            # Canary evidence: a successful call with an audited --json transcript, 0 events
            # by the runner's own count, and the host off (plan §0.16).
            good = bool(r.get("ok")) and own == 0 and host_off(text)
            audited += good
            secs.append(r.get("duration_ms", 0) / 1000 if good else float("inf"))
            parts = pathlib.PurePath(r.get("response_path", "")).parts
            if "invocations" in parts:
                resp = out.joinpath(*parts[parts.index("invocations"):])
                leak |= resp.is_file() and secret in resp.read_text(errors="replace")
        elif r.get("kind") == "submission":
            tools += r.get("tool_calls") or 0
    for sub_file in (out / "submissions").rglob("*") if (out / "submissions").exists() else []:
        if sub_file.is_file():
            leak |= secret in sub_file.read_text(errors="replace")
    secs += [float("inf")] * max(0, n - len(secs))          # a missing probe call counts as failed
    secret_path.unlink()
    return secs, leak, tools, audited


def codex_fingerprint():
    """The native codex binary and its helpers (sha256 of every file beside it), or None."""
    exe = shutil.which("codex")
    if not exe:
        return None
    real = pathlib.Path(os.path.realpath(exe))
    with open(real, "rb") as f:
        if f.read(4) not in MACHO:
            return None                 # a script shim: the native binary is not beside it
    files = {p.name: sha256(p) for p in sorted(real.parent.iterdir()) if p.is_file()}
    version = subprocess.run([str(real), "--version"], capture_output=True, text=True).stdout.strip()
    return {"realpath": str(real), "files": files, "version": version}


def isolation_ok():
    """§0.10 launch isolation: no codex context (.codex, AGENTS.md, skills, ...) in any ancestor of
    the temp root, where the provider creates each call's cwd. The other §0.10 items run as the
    preflight (canary through the provider, CODEX_HOME under the holdout dir) and the fingerprint."""
    tmp = pathlib.Path(tempfile.gettempdir()).resolve()
    return not any(os.path.lexists(d / name) for d in [tmp, *tmp.parents] for name in AMBIENT)


def drained():
    """§0.10 drain: the authoring pipeline's own fail-closed check."""
    r = subprocess.run([sys.executable, "-I", str(ROOT / "tools" / "holdout" / "c3_author.py"), "--drained"],
                       capture_output=True, text=True)
    lines = [json.loads(x) for x in r.stdout.splitlines() if x.strip().startswith("{")]
    return r.returncode == 0 and bool(lines) and lines[-1].get("ok") is True


def luna_today():
    """Luna invocations since 00:00 UTC across this cycle's runs (the same ledgers as eval.py;
    the preflight probe runs live under holdout/runs/c2-* and are counted from their ledgers)."""
    lo_ms = datetime.datetime.combine(today(), datetime.time(), datetime.timezone.utc).timestamp() * 1000
    seen, total = set(), 0
    ledgers = list((ROOT / "runs").glob("c2-*/*/ledger.jsonl")) + list((ROOT / "runs").glob("c2-*/ledger.jsonl")) \
        + list((ROOT / "runs").glob("c2-*/*/*/ledger.jsonl")) \
        + list((ROOT / "runs").glob("c2-pc/round*/*/attempt*/smoke/ledger.jsonl")) \
        + list((HOLDOUT / "runs").glob("c2-*/ledger.jsonl")) + list((HOLDOUT / "audit").glob("*/*/ledger.jsonl"))
    for led in ledgers:
        real = led.resolve()
        if real in seen or real.stat().st_mtime * 1000 < lo_ms:
            continue
        seen.add(real)
        with open(real) as f:
            for line in f:
                if '"invocation"' in line and LUNA in line:
                    try:
                        r = strict_json(line)
                    except ValueError:
                        total += 1                 # a damaged line may be a Luna call: count it (cap side)
                        continue
                    if r.get("kind") == "invocation" and r.get("provider") == LUNA and r.get("ts_ms", 0) >= lo_ms:
                        total += 1
    return total


class Lock:
    """One runner at a time (generate, manifest, score). A flock belongs to the open file
    description, and the generation child inherits the descriptor (pass_fds), so the lock is
    held until the child and every descendant that inherited it have exited: acquiring it
    proves that no generation is alive."""

    def __enter__(self):
        DIR.mkdir(parents=True, exist_ok=True)
        self.f = open(DIR / "runner.lock", "w")
        try:
            fcntl.flock(self.f, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            sys.exit("another confirm3 runner holds the lock")
        return self

    def __exit__(self, *exc):
        # Close only: LOCK_UN would act on the description shared with a live generation child.
        self.f.close()


def reserve(path, data):
    """Atomic attempt reservation: the file must not exist yet."""
    try:
        fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o644)
    except FileExistsError:
        sys.exit(f"{path.name} exists")
    with os.fdopen(fd, "w") as f:
        f.write(json.dumps(data) + "\n")
        f.flush()
        os.fsync(f.fileno())


def cmd_generate(args):
    """One replicate, attempted once (one logged relaunch with the same seed after a recorded
    terminal failure)."""
    with Lock() as lock:
        reg = registration(args.registered_at)
        sha = registration_commit()
        check_attempts(sha)
        rep, relaunch = args.rep, args.relaunch
        stage = f"confirm3-r{rep}"
        if complete(out_dir(rep)) or complete(out_dir(rep, True)):
            emit(stage=stage, decision="already generated")
            return
        out, attempt = out_dir(rep, relaunch), DIR / f"attempt-r{rep}{suffix(relaunch)}.json"
        first, failure = DIR / f"attempt-r{rep}.json", DIR / f"failed-r{rep}.json"
        if relaunch:
            if not first.exists():
                sys.exit("a relaunch needs a failed first attempt")
            if not failure.exists():
                # Under the lock no first-attempt generation is alive (its descendants held the lock),
                # and it never completed.
                failure.write_text(json.dumps({"rep": rep, "recorded_utc": now().isoformat(),
                                               "reason": "no generation_complete; no runner alive"}) + "\n")
        if attempt.exists() or out.exists():
            sys.exit(f"replicate {rep} was attempted without completing"
                     + ("; it is reported as failed" if relaunch else "; relaunch once with --relaunch, or report it as failed"))
        rep1 = next((o for o in (out_dir(1), out_dir(1, True)) if complete(o)), None)
        if rep == 2 and rep1 is None:
            sys.exit("replicate 1 has not generated")
        day = today()
        reason = spend_gate(day, commit_day(sha), C2_SEALED.exists(), rep, generation_day(rep1) if rep == 2 else None)
        if reason:
            emit(stage=stage, decision=f"deferred ({reason})")
            return
        if now().time() < START:
            emit(stage=stage, decision="deferred (before 00:05 UTC)")
            return
        if deferred_today(rep, day):
            emit(stage=stage, decision="deferred (a no-go today moves the start to the next 00:05 UTC)")
            return
        sys.path.insert(0, str(ROOT / "tools" / "holdout"))
        from c3lib import HOLDOUT as PIPELINE_HOLDOUT
        if PIPELINE_HOLDOUT.resolve() != HOLDOUT.resolve():
            sys.exit("the pipeline's holdout dir (A2D_HOLDOUT_DIR) differs from the runner's")
        if not drained():
            sys.exit("the authoring pipeline is not drained (STOP, no driver, no job process)")
        if not isolation_ok():
            sys.exit("launch isolation failed (codex context in an ancestor of the temp root)")
        before = codex_fingerprint()
        if before is None or before["version"] != reg["codex_version"] or before["files"] != reg["codex_files"]:
            sys.exit("codex differs from the registered binary")
        worst = len(reg["tasks"]) * len(reg["arms"]) * reg["k"]
        spent = luna_today()
        if spent + worst + LATENCY_PROBES + 2 > DAILY_CAP:
            record_deferral(rep, day, "daily Luna cap", luna_today=spent)
            emit(stage=stage, decision="deferred (daily Luna cap)", luna_today=spent, stage_worst_case=worst, cap=DAILY_CAP)
            return
        # The registered controller is built before any model call, so the preflight probes and the
        # generation run the same executable (Sol registration review, 10-21).
        binary = build_controller()
        # Deterministic preconditions the controller enforces only after the attempt is reserved
        # (fresh-Claude review B2): refuse here, with no attempt and no model call.
        if suite_hash_now() != reg["suite_hash"]:
            sys.exit("the suite hash differs from the registered one (no attempt)")
        if not crates_clean(sha):
            sys.exit("crates/, Cargo.toml or Cargo.lock differ from the registration commit (no attempt)")
        # Preflight through the real provider: latency on the coder lane, canary on every lane.
        group = now().strftime("%Y%m%dT%H%M%SZ")
        record_probe(rep, day, group, "started")
        lanes = [(reg["model"], reg["effort"], LATENCY_PROBES)] + [(*lane(s), 1) for s in reg["lanes"].values()]
        results = []
        for i, (model, effort, n) in enumerate(lanes):
            secs, leak, tools, audited = probe_run(model, effort, n, f"r{rep}-{group}-{i}", lock.f.fileno())
            results.append({"model": model, "effort": effort, "secs": [round(s, 1) if s != float("inf") else None
                                                                       for s in secs], "leak": leak, "tool_calls": tools,
                            "audited": audited, "controller_binary_sha256": binary})
        if any(r["leak"] or r["tool_calls"] for r in results):
            record_probe(rep, day, group, "isolation-failed", lanes=results)
            sys.exit("the canary or tool preflight failed: the replicate does not start (§0.10)")
        # Every lane needs canary evidence: at least one successful call with a --json transcript.
        evidence = all(r["audited"] >= 1 for r in results)
        go = evidence and latency_go([s if s is not None else float("inf") for s in results[0]["secs"]])
        record_probe(rep, day, group, "go" if go else "nogo", lanes=results)
        if not go:
            reason = "latency" if evidence else "canary evidence missing on a lane"
            record_deferral(rep, day, reason, secs=results[0]["secs"])
            emit(stage=stage, decision=f"deferred ({reason})", secs=results[0]["secs"])
            return
        if sha256(A2D) != binary:
            sys.exit("the controller changed between the preflight and the generation")
        reserve(attempt, {"rep": rep, "relaunch": relaunch, "registered_at": sha, "pid": os.getpid(), "codex": before,
                          "controller_binary_sha256": binary, "started_utc": now().isoformat()})
        cmd = [A2D, "lift", "run", "--experiment", f"C2-CONFIRM3-R{rep}", "--registered-at", sha,
               "--expect-suite-hash", reg["suite_hash"], "--suite", str(SUITE), "--model", reg["model"],
               "--effort", reg["effort"], "--arms", ",".join(reg["arms"]), "--k", str(reg["k"]), "--replicates", "1",
               "--primary", "D", "--seed", str(reg["seeds"][str(rep)]), "--jobs", str(args.jobs),
               "--defer-scoring", "--claim", reg["claim"], "--max-invocations", str(worst + 40),
               "--max-wall-clock-secs", "43200", "--out", str(out), "--external", f"frozen={DIR / 'frozen.py'}"]
        for name, spec in reg["lanes"].items():
            cmd += ["--lane", f"{name}={spec}"]
        out.parent.mkdir(parents=True, exist_ok=True)
        with open(str(out) + ".log", "a") as log:
            child = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT, pass_fds=(lock.f.fileno(),),
                                     env={**os.environ, "A2D_CODEX_HOME_ROOT": str(CODEX_HOMES)})
            (DIR / f"generation-r{rep}{suffix(relaunch)}.json").write_text(
                json.dumps({"pid": child.pid, "started_utc": now().isoformat()}) + "\n")
            code = child.wait()
        after = codex_fingerprint()
        (DIR / f"bracket-r{rep}{suffix(relaunch)}.json").write_text(
            json.dumps({"before": before, "after": after, "match": after == before}, indent=1) + "\n")
        if code or not complete(out):
            (DIR / f"failed-r{rep}{suffix(relaunch)}.json").write_text(
                json.dumps({"rep": rep, "exit": code, "recorded_utc": now().isoformat()}) + "\n")
            sys.exit(f"generation failed ({code}); " + ("reported as failed" if relaunch else "relaunch once with --relaunch"))
        emit(stage=stage, decision="generated (scoring deferred)")


def skipped_days(reg):
    """Eligible UTC days before each replicate's launch with no recorded deferral (disclosed):
    the runner did not run that day, so the launch day was not the first eligible one."""
    deferred = set()
    path = DIR / "deferrals.jsonl"
    if path.exists():
        deferred = {(json.loads(x)["rep"], json.loads(x)["utc_day"]) for x in path.read_text().splitlines() if x.strip()}
    first = max(EARLIEST, commit_day(registration_commit()) + datetime.timedelta(days=1))
    out = {}
    for rep in (1, 2):
        rec = DIR / f"attempt-r{rep}.json"
        if not rec.exists():
            continue
        start = datetime.date.fromisoformat(json.loads(rec.read_text())["started_utc"][:10])
        day = first if rep == 1 else max(first, start)
        if rep == 2 and (DIR / "attempt-r1.json").exists():
            day = datetime.date.fromisoformat(json.loads((DIR / "attempt-r1.json").read_text())["started_utc"][:10]) \
                + datetime.timedelta(days=1)
        missing = []
        while day < start:
            if (rep, str(day)) not in deferred:
                missing.append(str(day))
            day += datetime.timedelta(days=1)
        out[str(rep)] = missing
    return out


def generated_out(rep):
    """(out dir, relaunch) of the completed generation of a replicate, or None."""
    for relaunch in (False, True):
        if complete(out_dir(rep, relaunch)):
            return out_dir(rep, relaunch), relaunch
    return None


def cmd_manifest(args):
    """Content-free anchors of both replicates, committed before scoring (both or none)."""
    with Lock():
        registration(args.registered_at)
        check_attempts(registration_commit())
        outs = {rep: generated_out(rep) for rep in (1, 2)}
        if None in outs.values():
            sys.exit("both replicates must have generated before any manifest is written")
        for rep in (1, 2):
            if (DIR / f"manifest-r{rep}.json").exists():
                sys.exit(f"manifest-r{rep}.json exists; anchors are written once")
        for rep, (out, _) in outs.items():
            r = subprocess.run([A2D, "lift", "manifest", str(out)], capture_output=True, text=True)
            if r.returncode:
                sys.exit(f"manifest of replicate {rep} failed ({r.returncode})")
            (DIR / f"manifest-r{rep}.json").write_text(r.stdout)
        emit(stage="confirm3-manifest", decision="written; commit and push them, then score")


def cmd_score(args):
    """Score each replicate once against its committed anchor, pool, and decide."""
    with Lock():
        reg = registration(args.registered_at)
        check_attempts(registration_commit())
        decision_path = DIR / "decision.json"
        if decision_path.exists():
            sys.exit("decision.json exists; the set is scored once")
        binary = build_controller()
        if suite_hash_now() != reg["suite_hash"]:
            sys.exit("the suite hash differs from the registered one; nothing is scored")
        if not crates_clean(registration_commit()):
            sys.exit("crates/, Cargo.toml or Cargo.lock differ from the registration commit; nothing is scored")
        outs = {}
        for rep in (1, 2):
            rel = str((DIR / f"manifest-r{rep}.json").relative_to(ROOT))
            if git("ls-files", "--error-unmatch", rel)[0] or git("diff", "--quiet", "HEAD", "--", rel)[0] \
                    or git("merge-base", "--is-ancestor", "HEAD", "origin/main")[0]:
                sys.exit(f"commit and push {rel} before scoring")
            outs[rep] = generated_out(rep)
            if outs[rep] is None:
                sys.exit(f"replicate {rep} has not generated")
        for rep, (out, _) in outs.items():
            if (out / "summary.json").exists():
                continue
            head = json.loads((DIR / f"manifest-r{rep}.json").read_text())["ledger_head"]
            with open(str(out) + ".score.log", "w") as log:
                code = subprocess.run([A2D, "lift", "score", "--out", str(out), "--suite", str(SUITE),
                                       "--expect-head", head, "--jobs", "6"], stdout=log,
                                      stderr=subprocess.STDOUT).returncode
            if code:
                sys.exit(f"scoring replicate {rep} failed ({code})")
        pooled = DIR / "pooled-summary.json"
        r = subprocess.run([A2D, "lift", "pool", "--out", str(pooled)]
                           + [str(out / "ledger.jsonl") for out, _ in outs.values()], capture_output=True, text=True)
        if r.returncode:
            sys.exit(f"pooling failed ({r.returncode})")
        tuples, stats = {}, {}
        for rep, (out, relaunch) in outs.items():
            t, stats[rep] = replicate_tuples(out, rep, reg["arms"], reg["tasks"])
            bracket = DIR / f"bracket-r{rep}{suffix(relaunch)}.json"
            if not bracket.exists() or not json.loads(bracket.read_text())["match"]:
                for v in t.values():
                    v["void"] = True            # another codex ran, or the bracket is missing
            tuples.update(t)
        summary = load_summary_obj(pooled.read_text())
        failed = {}
        for rep, (out, relaunch) in outs.items():
            if relaunch and out_dir(rep).exists():   # the failed first attempt's evidence counts too
                failed[rep] = failed_attempt_audit(out_dir(rep), reg["tasks"])
                summary = merge_failed_attempt(summary, stats, rep, failed[rep])
        if len(tuples) != 2 * len(reg["arms"]) * len(reg["tasks"]):
            sys.exit("planned tuples do not match the registration")
        for rep, (out, _) in outs.items():       # each replicate's own integrity record counts too
            own = load_summary_obj((out / "summary.json").read_text()) if (out / "summary.json").exists() else None
            summary = fold_replicate_summary(summary, own)
        decision = decide(summary, tuples, stats)
        decision["failed_attempt_audit"] = {str(k): v for k, v in failed.items()}
        decision["calls_and_tokens_by_lane"] = {str(rep): lane_usage(out) for rep, (out, _) in outs.items()}
        decision["controller_binary_sha256"] = binary
        decision["skipped_eligible_days"] = skipped_days(reg)
        decision_path.write_text(json.dumps(decision, indent=1) + "\n")
        print(json.dumps(decision, indent=1))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    g = sub.add_parser("generate")
    g.add_argument("--rep", type=int, choices=(1, 2), required=True)
    g.add_argument("--registered-at", required=True)
    g.add_argument("--relaunch", action="store_true")
    g.add_argument("--jobs", type=int, default=8, choices=[8])
    for name in ("manifest", "score"):
        s = sub.add_parser(name)
        s.add_argument("--registered-at", required=True)
    args = ap.parse_args()
    {"generate": cmd_generate, "manifest": cmd_manifest, "score": cmd_score}[args.cmd](args)


if __name__ == "__main__":
    main()
