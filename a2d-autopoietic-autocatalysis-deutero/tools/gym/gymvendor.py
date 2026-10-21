"""Pure functions vendored from nano/confirm3.py for gym v3 (plan 2026-10-21, "Isolation").

The gym never imports nano/confirm3.py: it is bound by confirmation-3's registration, and an
import would put the gym's runtime on its module state (HOLDOUT, SUITE, A2D). These are the
function bodies copied verbatim; tests/test_gym.py checks parity against nano/confirm3.py's
source by AST (the file is parsed, never imported or executed) and on fixtures.

Vendored: lines, strict_json, records_tolerant, py_tool_events (the provider's allowlist
detector port), host_off, transcript_text, transcript_ok, test_names, replicate_tuples and
isolation_ok (temp-root ancestry), with the constants they use.
"""
import json
import os
import pathlib
import re
import tempfile

CODEX_STDERR_MARKER = "\n--- codex stderr ---\n"     # a2d-providers: --json events, marker, stderr
# The provider's allowlist detector (crates/a2d-providers/src/cli.rs `codex_tool_events`),
# ported line for line so that the bound runner recounts every transcript itself.
ALLOWED_ITEMS = ("agent_message", "reasoning")
ALLOWED_EVENTS = ("thread.started", "turn.started", "turn.completed", "item.started", "item.updated", "item.completed")
CODEX_HOST_OFF_NOTICE = ("Code Mode is unavailable because code-mode host is disabled. Code mode will fail closed; "
                         "enable `features.code_mode_host` and install `codex-code-mode-host`.")
AMBIENT = (".codex", "AGENTS.md", "AGENTS.override.md", ".agents", "skills")

VENDORED = ("lines", "_reject_constant", "strict_json", "records_tolerant", "_events", "py_tool_events", "host_off",
            "transcript_text", "transcript_ok", "test_names", "replicate_tuples", "isolation_ok")
VENDORED_CONSTANTS = ("CODEX_STDERR_MARKER", "ALLOWED_ITEMS", "ALLOWED_EVENTS", "CODEX_HOST_OFF_NOTICE", "AMBIENT")


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


def isolation_ok():
    """§0.10 launch isolation: no codex context (.codex, AGENTS.md, skills, ...) in any ancestor of
    the temp root, where the provider creates each call's cwd. The other §0.10 items run as the
    preflight (canary through the provider, CODEX_HOME under the holdout dir) and the fingerprint."""
    tmp = pathlib.Path(tempfile.gettempdir()).resolve()
    return not any(os.path.lexists(d / name) for d in [tmp, *tmp.parents] for name in AMBIENT)


def generation_complete_in(ledger):
    """Gym launch gate (a): True iff the ledger file holds a record with kind
    "generation_complete" and `aborted is False` (confirm3.complete's rule: an aborted generation
    may still be relaunched). Lines are split by lines() and parsed with strict_json; only `kind`
    and `aborted` are inspected; nothing is returned or printed but the boolean. A missing or
    unreadable ledger is False."""
    try:
        text = pathlib.Path(ledger).read_text(errors="replace")
    except OSError:
        return False
    for line in lines(text):
        if not line.strip():
            continue
        try:
            rec = strict_json(line)
        except ValueError:
            continue
        if isinstance(rec, dict) and rec.get("kind") == "generation_complete" and rec.get("aborted") is False:
            return True
    return False
