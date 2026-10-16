#!/usr/bin/env python3
"""Frozen cycle-2 proposer comparison, protocol v2 (Python stdlib only).

Run with python3 -I. Offline commands: quarters, stats, dossiers, brief,
analyze. Only propose and round invoke providers, through injectable callables.
stats consumes candidate_pool.py JSON via --pool (or <run>/pool.json); it
never re-scores sources or opens test files. No candidate module is imported.

Typical offline preparation (paths abbreviated):
  stats --runs <screen>=<suite> <rest>=<suite> <confirm>=<suite> --pool <pool.json> --out <stats.json>
  dossiers --runs <run>=<suite> ... --round 1 --out runs/c2-pc/round1/dossiers.json
  brief --arm informed --round 1 --stats <stats.json> --dossiers <dossiers.json> --out <PROMPT.md>
Put PROMPT.md in runs/c2-pc/round1/{informed,uninformed}/ before propose,
or put stats.json and dossiers.json in the round directory for auto-briefing.
"""
import argparse
import collections
import csv
import datetime
import hashlib
import importlib.util
import io
import itertools
import json
import math
import pathlib
import py_compile
import random
import re
import subprocess
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[3]


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ev = load_module("phaseb_eval", ROOT / "nano/eval.py")
proposer = load_module("phaseb_proposer", ROOT / "docs/experiments/analysis/c2_propose.py")
ARCHITECT = ("You are the architect of a coding harness. You change the harness so that "
             "the programmer produces working software more reliably on future, different tasks.")
RULES = proposer.RULES + ("- The coder stays gpt-6-luna at low effort; every declared lane may only name "
                          "codex/gpt-6.1-sol.\n")
ASK = """## Your task
Propose EXACTLY ONE candidate. Give:
1. a heading `### CANDIDATE 1: <slug>` (lowercase letters, digits and hyphens);
2. `Hypothesis:` one sentence;
3. `Mechanism:` why it should work;
4. `Expected:` the expected all-pass change and the main risk;
5. the complete replacement file in one ```python block.
"""
# The surrounding template is identical; only this information slot changes.
TEMPLATES = dict.fromkeys(("informed", "uninformed"), "{shared}\n{information}\n{ending}")
ARMS = ("X:inc", "X:inf", "X:uninf")
ARM_NAMES = {"informed": "inf", "uninformed": "uninf"}


def local(path):
    """Do not follow caller-controlled paths outside the repository."""
    path = pathlib.Path(path).resolve()
    if not path.is_relative_to(ROOT.resolve()):
        raise ValueError("input/output path must be inside the repository")
    if path.name == "hidden_tests.rs":
        raise ValueError("this tool never reads test source files")
    return path


def read_json(path):
    return json.loads(local(path).read_text())


def write_json(path, value):
    path = local(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def immutable_json(path, value):
    path = local(path)
    if path.exists() and read_json(path) != value:
        raise ValueError(f"immutable artifact changed: {path.name}")
    write_json(path, value)


def phase_dir():
    return ROOT / "runs/c2-pc"


def round_dir(r):
    if r not in range(1, 7):
        raise ValueError("round must be in 1..6")
    return phase_dir() / f"round{r}"


def quarter(r, suite=None):
    round_dir(r)
    fams = sorted(ev.families(local(suite or ev.DEV)))
    random.Random(f"A2D-C2-PC-{r}").shuffle(fams)
    size, extra = divmod(len(fams), 4)
    groups, start = [], 0
    for i in range(4):
        end = start + size + (i < extra)
        groups.append(sorted(fams[start:end]))
        start = end
    index = (r - 1) % 4
    return {"round": r, "seed": f"A2D-C2-PC-{r}", "quarters": groups,
            "evaluation_index": index, "evaluation_families": groups[index]}


def cmd_quarters(args):
    data = quarter(args.round)
    immutable_json(round_dir(args.round) / "quarter.json", data)
    print(json.dumps(data, indent=2))


def run_pairs(values):
    pairs = []
    for value in values:
        run, suite = value.split("=", 1)
        pairs.append((local(run), local(suite)))
    return pairs


def records(run):
    return [json.loads(line) for line in local(run / "ledger.jsonl").read_text().splitlines() if line.strip()]


def task_families(suite):
    return {task: family for family, tasks in ev.families(local(suite)).items() for task in tasks}


def arm_dir(arm):
    return arm.replace(":", "-")


def tuple_dir(run, arm, task):
    return local(run / "invocations" / arm_dir(arm) / task / "r0")


def submission_path(run, arm, task):
    return local(run / "submissions" / arm_dir(arm) / task / "r0.rs")


def score_table(run, suite, arms):
    """Like eval.scores, but retain operational failures in every denominator.

    The controller scores absent code as failure (score_hidden(None)). Its
    summary can exclude voids, so use ledger scores, never summary arm rates.
    A completely absent score is explicitly zero-filled and reported.
    """
    families = task_families(suite)
    table, seen, missing, voids = {}, set(), [], []
    rows = records(run)
    for rec in rows:
        if rec.get("kind") != "score" or rec.get("arm") not in arms:
            continue
        if rec.get("replicate", 0) != 0:
            raise ValueError("phase B expects one replicate per run")
        key = rec["arm"], rec["task"]
        if key in seen or key[1] not in families:
            raise ValueError("duplicate or unexpected score")
        seen.add(key)
        result = rec["result"]
        expected = result.get("expected") or 0
        frac = result.get("passed", 0) / expected if expected and result.get("compiled") else 0.0
        table[key] = (frac, float(bool(result.get("all_passed"))), families[key[1]])
    for arm in arms:
        for task, family in families.items():
            if (arm, task) not in table:
                table[arm, task] = (0.0, 0.0, family)
                missing.append({"arm": arm, "task": task})
    for rec in rows:
        if rec.get("kind") == "submission" and rec.get("arm") in arms and rec.get("void_reason"):
            key = rec["arm"], rec["task"]
            if key in table:
                table[key] = (0.0, 0.0, families[key[1]])
                voids.append({"arm": key[0], "task": key[1]})
    return table, {"missing_scores": missing, "void_tuples": voids}


def branch_of(run, arm, task):
    branch = "unknown"
    for path in sorted(tuple_dir(run, arm, task).glob("*harness.stderr.txt")):
        for line in local(path).read_text().splitlines():
            if line.startswith("DIAG "):
                try:
                    branch = json.loads(line[5:]).get("branch", branch)
                except (ValueError, AttributeError):
                    continue
    return branch if branch in {"judge", "one-green", "no-green-repair", "unknown"} else "other"


def stats(pairs, arm, pool):
    """Pool schema is candidate_pool.py's list of per-coder-response records."""
    by_tuple = collections.defaultdict(dict)
    for sample in pool:
        if sample["arm"] not in (arm, arm_dir(arm)):
            continue
        key = sample["run"], sample["task"]
        if sample["inv"] in by_tuple[key]:
            raise ValueError("duplicate sample in pool")
        by_tuple[key][sample["inv"]] = sample
    branches, buckets, failures = collections.Counter(), collections.Counter(), collections.Counter()
    sample_n = sample_ok = passed = n = 0
    curves = {str(k): {"n": 0, "public_green": 0.0, "hidden_pass": 0.0,
                       "public_green_and_hidden_pass": 0.0} for k in range(1, 6)}
    for run, suite in pairs:
        table, _ = score_table(run, suite, [arm])
        scored = {r["task"]: r["result"] for r in records(run)
                  if r.get("kind") == "score" and r.get("arm") == arm}
        for task in sorted(task_families(suite)):
            n += 1
            ok = bool(table[arm, task][1])
            passed += ok
            branch = branch_of(run, arm, task)
            branches[branch] += 1
            samples = by_tuple.get((str(run), task), by_tuple.get((run.name, task), {}))
            responses = list(tuple_dir(run, arm, task).glob("inv*-lift-coder.response.txt"))
            if any(p.name.split("-")[0] not in samples for p in responses):
                raise ValueError("pool is missing default-lane coder samples; re-score the complete run")
            samples = [samples[k] for k in sorted(samples)]
            greens = [s for s in samples if s.get("public", {}).get("ok")]
            green_hidden = any(s.get("hidden", {}).get("ok") for s in greens)
            if not ok:
                if branch == "judge":
                    buckets["judge missed a hidden-passing green" if green_hidden else
                            "judge-branch with no hidden-passing green"] += 1
                elif branch == "one-green":
                    buckets["one-green fails hidden"] += 1
                else:
                    buckets["no green"] += 1
                result = scored.get(task)
                kind = ("missing" if result is None else "timeout" if result.get("timed_out") else
                        "compile" if not result.get("compiled") else "partial")
                failures[kind] += 1
            sample_n += len(samples)
            sample_ok += sum(bool(s.get("hidden", {}).get("ok")) for s in samples)
            for k in range(1, min(5, len(samples)) + 1):
                entry = curves[str(k)]
                subsets = list(itertools.combinations(samples, k))
                entry["n"] += 1
                for metric, predicate in (
                    ("public_green", lambda s: s.get("public", {}).get("ok", False)),
                    ("hidden_pass", lambda s: s.get("hidden", {}).get("ok", False)),
                    ("public_green_and_hidden_pass", lambda s: s.get("public", {}).get("ok", False)
                     and s.get("hidden", {}).get("ok", False)),
                ):
                    entry[metric] += sum(any(predicate(s) for s in subset) for subset in subsets) / len(subsets)
    for entry in curves.values():
        for key in ("public_green", "hidden_pass", "public_green_and_hidden_pass"):
            entry[key] = entry[key] / entry["n"] if entry["n"] else None
    return {"n_task_runs": n, "all_pass_rate": passed / n if n else None,
            "branch_counts": dict(branches), "failure_buckets": dict(buckets),
            "failure_kinds": dict(failures), "any_pass_at_k": curves,
            "n_samples": sample_n, "per_sample_hidden_allpass_rate": sample_ok / sample_n if sample_n else None}


def cmd_stats(args):
    pairs = run_pairs(args.runs)
    paths = args.pool or [run / "pool.json" for run, _ in pairs]
    if not all(local(p).exists() for p in paths):
        raise ValueError("stats needs candidate_pool.py output: supply --pool <pooled.json>")
    pool = [sample for path in paths for sample in read_json(path)]
    data = stats(pairs, args.arm, pool)
    write_json(args.out, data)
    print(json.dumps(data, indent=2))


# What the coder saw, independent of the incumbent's own prompt additions
# (sampling angles, reviewer notes): arm D's first prompt in the v0 A/A run is
# exactly the controller's task prompt (parity-tested), for every dev task.
TASK_PROMPTS = "runs/c2-aa-200daaa763c4/run/invocations/D"


def task_prompt_text(task):
    prompt = local(ROOT / TASK_PROMPTS / task / "r0/inv00-lift-coder.prompt.txt").read_text()
    if "PROMPT:\n" not in prompt:
        raise ValueError("first coder prompt has no PROMPT section")
    return prompt.split("PROMPT:\n", 1)[1]


def dossiers(pairs, arm, r, maximum=8):
    if maximum < 0:
        raise ValueError("maximum must be nonnegative")
    excluded = set(quarter(r)["evaluation_families"])
    selected = {}
    for run, suite in pairs:
        families = task_families(suite)
        rows = records(run)
        public = {rec["task"]: rec.get("public", {}).get("all_passed", False) for rec in rows
                  if rec.get("kind") == "submission" and rec.get("arm") == arm and rec.get("replicate", 0) == 0}
        for rec in rows:
            task = rec.get("task")
            if (rec.get("kind") != "score" or rec.get("arm") != arm or rec.get("replicate", 0) != 0
                    or rec["result"].get("all_passed") or not public.get(task)
                    or task not in families or families[task] in excluded or task in selected):
                continue
            selected[task] = {"requirements_and_public_tests": task_prompt_text(task),
                              "submitted_file": submission_path(run, arm, task).read_text(),
                              "label": "fails at least one hidden test"}
    order = sorted(selected, key=lambda t: hashlib.sha256(f"A2D-C2-PC-dossier-{r}-{t}".encode()).hexdigest())
    return [selected[t] for t in order[:min(maximum, 8)]]


def cmd_dossiers(args):
    if args.max < 0:
        raise ValueError("--max must be nonnegative")
    data = dossiers(run_pairs(args.runs), args.arm, args.round, args.max)
    write_json(args.out, data)
    print(json.dumps({"dossiers": len(data)}))


def frozen_incumbent():
    snap = phase_dir() / "incumbent.py"
    return snap if snap.exists() else ROOT / "nano/harness.py"


def frozen_v0():
    snap = phase_dir() / "v0.py"
    return snap if snap.exists() else ROOT / "nano/history/before-E002b.py"


def section(title, body):
    return f"## {title}\n\n{body.strip()}\n"


def information(arm, r, stats_data=None, dossier_data=None):
    if arm == "informed":
        if stats_data is None or dossier_data is None:
            raise ValueError("informed brief requires --stats and --dossiers")
        rows = list(csv.DictReader(io.StringIO(local(ROOT / "nano/results.tsv").read_text()), delimiter="\t"))
        def allowed(row):
            if row.get("proposer") == "pc-uninformed":
                return False
            if row.get("stage") != "round":
                return True
            number = re.fullmatch(r"PC-(\d+)", row.get("id", ""))
            return row.get("proposer") == "pc-informed" and (number is None or int(number[1]) < r)
        rows = [row for row in rows if allowed(row)]
        buf = io.StringIO()
        writer = csv.DictWriter(buf, fieldnames=ev.FIELDS, delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
        return "\n".join((section("Results", buf.getvalue()),
                          section("Failure aggregates", json.dumps(stats_data, indent=2)),
                          section("Failure dossiers", json.dumps(dossier_data, indent=2))))
    titles = []
    for previous in range(1, r):
        path = round_dir(previous) / "uninformed/candidate.json"
        if path.exists():
            data = read_json(path)
            titles.append(f"{data.get('slug', '')}\nHypothesis: {data.get('hypothesis', '')}")
    return section("Your earlier proposal titles", "\n\n".join(titles) or "None.")


def brief(arm, r, stats_data=None, dossier_data=None):
    round_dir(r)
    shared = "\n".join((ARCHITECT, proposer.PROTOCOL,
                         section("The incumbent", "```python\n" + local(frozen_incumbent()).read_text() + "```"),
                         section("The baseline v0", "```python\n" + local(frozen_v0()).read_text() + "```")))
    prompt = TEMPLATES[arm].format(shared=shared, information=information(arm, r, stats_data, dossier_data),
                                   ending=RULES + "\n" + ASK)
    # Fail closed rather than silently redact evidence or leak forbidden lines.
    assert not re.search(r"holdout|seeds\.md", prompt, re.I), "forbidden context in proposer brief"
    return prompt


def cmd_brief(args):
    prompt = brief(args.arm, args.round, read_json(args.stats) if args.stats else None,
                   read_json(args.dossiers) if args.dossiers else None)
    path = local(args.out)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(prompt)


def audit_events(lines):
    """Audit every streamed item, including item.started/item.updated events.

    Lifecycle envelopes are allowed; their item payloads must be messages or
    reasoning. Unknown envelopes, malformed JSON and missing items fail closed.
    """
    offending, usage, errors, count = set(), [], [], 0
    for line in lines.splitlines() if isinstance(lines, str) else lines:
        if not line.strip():
            continue
        count += 1
        try:
            event = json.loads(line)
            kind = event.get("type", "")
            if kind.startswith("item."):
                item = event.get("item") or {}
                itype = item.get("type", "missing-item-type")
                allowed = itype in {"agent_message", "assistant_message", "reasoning"}
                if itype == "message":
                    allowed = item.get("role") == "assistant"
                if not allowed:
                    offending.add(itype)
            elif kind in {"thread.started", "turn.started", "turn.completed"}:
                pass
            elif kind in {"turn.failed", "error"}:
                errors.append(event.get("error", event.get("message", kind)))
            else:
                offending.add(kind or "missing-event-type")
            if event.get("usage") is not None:
                usage.append(event["usage"])
        except (ValueError, TypeError, AttributeError):
            offending.add("malformed-event")
    if not count:
        offending.add("empty-event-stream")
    return {"void": bool(offending), "offending_types": sorted(offending), "usage": usage, "errors": errors}


def call_model(prompt):
    """Only model-call boundary; tests inject an offline replacement."""
    # An empty directory OUTSIDE the repository: the proposer must not start
    # where project instructions, runs or task suites are reachable.
    with tempfile.TemporaryDirectory(prefix="pc-call-") as empty:
        reply_path = pathlib.Path(empty) / "last.md"
        cmd = ["codex", "exec", prompt, "-m", "gpt-6.1-sol", "-c", 'model_reasoning_effort="high"',
               "-c", 'approval_policy="never"', "--sandbox", "read-only", "--skip-git-repo-check",
               "--ephemeral", "--ignore-user-config", "--ignore-rules", "--json", "-o", str(reply_path)]
        for feature in proposer.DISABLED:
            cmd += ["--disable", feature]
        try:
            result = subprocess.run(cmd, cwd=empty, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=3600)
        except subprocess.TimeoutExpired as error:
            events = error.stdout or ""
            if isinstance(events, bytes):
                events = events.decode("utf-8", errors="replace")
            return {"returncode": 124, "events": events, "reply": "", "stderr": "proposer API call timed out"}
        return {"returncode": result.returncode, "events": result.stdout,
                "reply": reply_path.read_text() if reply_path.exists() else "",
                "stderr": result.stderr}


def parse_candidate(reply):
    headings = re.findall(r"^### CANDIDATE (\d+):\s*([a-z0-9-]+)[ \t]*$", reply, re.M)
    blocks = re.findall(r"^```python[ \t]*\n(.*?)^```[ \t]*$", reply, re.S | re.M)
    if len(headings) != 1 or headings[0][0] != "1" or len(blocks) != 1:
        raise ValueError("expected exactly CANDIDATE 1 and one Python block")
    data = {"slug": headings[0][1]}
    for label in ("Hypothesis", "Mechanism", "Expected"):
        match = re.search(rf"^{label}:[ \t]*(.+)$", reply, re.M)
        if not match:
            raise ValueError(f"missing {label}:")
        data[label.lower()] = match.group(1).strip()
    return data, blocks[0]


def validate_candidate(path):
    try:
        ev.guard(local(path))
        # Check each declaration, not the merged dict (a duplicate name could
        # otherwise hide a prohibited earlier declaration).
        for line in local(path).read_text().splitlines():
            if re.match(r"^# lanes:", line):
                entries = line.partition(":")[2].split()
                if not entries:
                    raise ValueError("empty lanes declaration")
                for entry in entries:
                    name, lane = entry.split("=", 1)
                    if not name or lane.rsplit(":", 1)[0] != "codex/gpt-6.1-sol":
                        raise ValueError("lanes may only name codex/gpt-6.1-sol")
        with tempfile.TemporaryDirectory(dir=local(path).parent) as tmp:
            py_compile.compile(str(path), cfile=str(pathlib.Path(tmp) / "candidate.pyc"), doraise=True)
    except (SystemExit, Exception) as error:
        return False, str(error)
    return True, ""


def freeze():
    base = phase_dir()
    base.mkdir(parents=True, exist_ok=True)
    for name, source in (("incumbent.py", ROOT / "nano/harness.py"),
                         ("v0.py", ROOT / "nano/history/before-E002b.py")):
        snap = local(base / name)
        if not snap.exists():
            snap.write_bytes(local(source).read_bytes())
    immutable_json(base / "frozen.json", {"incumbent_sha": ev.sha(base / "incumbent.py"),
                                          "v0_sha": ev.sha(base / "v0.py")})
    return base / "incumbent.py"


def smoke_candidate(path, r, out, jobs=1):
    """One dev-task completion check. No hidden results enter a re-ask."""
    task = min(task for family, tasks in ev.families(ev.DEV).items()
               if family not in quarter(r)["evaluation_families"] for task in tasks)
    suite = out / "smoke-suite"
    suite.mkdir(exist_ok=True)
    if not (suite / task).exists():
        (suite / task).symlink_to(local(ev.DEV / task), target_is_directory=True)
    if not ev.within_cap("proposal-smoke", 5, ev.DAILY_CAP):
        raise RuntimeError("smoke deferred by daily budget; proposal is not yet valid")
    run = out / "smoke"
    ev.lift(f"C2-PC-{r}-SMOKE", suite, ["X:cand"], {"cand": path}, ev.lanes_of(path), run, r, jobs)
    submissions = [rec for rec in records(run) if rec.get("kind") == "submission"]
    if len(submissions) != 1 or submissions[0].get("void_reason") or not submission_path(run, "X:cand", task).exists():
        raise ValueError("1-task smoke did not complete with a submission")
    bad = authorship(run, "X:cand", [task])
    if bad:
        raise ValueError("smoke submission was not written by the default-lane coder")


def propose(arm, r, model_call=None, smoke=None):
    model_call = model_call or call_model
    smoke = smoke or smoke_candidate
    freeze()
    out = round_dir(r) / arm
    out.mkdir(parents=True, exist_ok=True)
    if (out / "candidate.json").exists():
        return read_json(out / "candidate.json")
    prompt_path = out / "PROMPT.md"
    if not prompt_path.exists():
        prompt_path.write_text(brief(arm, r, read_json(round_dir(r) / "stats.json") if arm == "informed" else None,
                                     read_json(round_dir(r) / "dossiers.json") if arm == "informed" else None))
    prompt = prompt_path.read_text()
    assert not re.search(r"holdout|seeds\.md", prompt, re.I), "forbidden context in proposer brief"
    attempts, data = [], {}
    valid, reason = False, "not attempted"
    candidate = out / "candidate.py"
    candidate.write_text("")
    for attempt in range(2):
        actual_prompt = prompt if not attempt else prompt + "\n\nPrevious attempt was invalid. Correct this error:\n" + reason
        attempt_dir = out / f"attempt{attempt + 1}"
        attempt_dir.mkdir(exist_ok=True)
        (attempt_dir / "PROMPT.md").write_text(actual_prompt)
        summary = {"void": False, "offending_types": [], "usage": []}
        reply = ""
        try:
            response = model_call(actual_prompt)
            reply = response.get("reply", "")
            summary = audit_events(response.get("events", ""))
            if response.get("returncode") or summary["errors"]:
                raise ValueError("API failure: " + str(summary.get("errors") or response.get("stderr", "")))
            if summary["void"]:
                raise ValueError("void tool event(s): " + ", ".join(summary["offending_types"]))
            data, code = parse_candidate(reply)
            candidate.write_text(code)
            valid, reason = validate_candidate(candidate)
            if not valid:
                raise ValueError(reason)
            smoke(candidate, r, attempt_dir)
            valid, reason = True, ""
        except (Exception, SystemExit) as error:
            valid, reason = False, str(error)
        summary.update({"attempt": attempt + 1, "valid": valid, "reason": reason})
        attempts.append(summary)
        (attempt_dir / "reply.md").write_text(reply)
        (out / "reply.md").write_text(reply)
        if valid:
            break
    write_json(out / "events-summary.json", {"attempts": attempts})
    data.update({"valid": valid, "preflight_valid": valid, "reason": reason, "arm": arm,
                 "round": r, "candidate_sha": ev.sha(candidate)})
    write_json(out / "candidate.json", data)
    return data


def cmd_propose(args):
    print(json.dumps(propose(args.arm, args.round), indent=2))


def authorship(run, arm, tasks):
    violations = []
    for task in sorted(tasks):
        submitted = submission_path(run, arm, task)
        # A crash with no submission remains a task failure, not an invalid
        # proposal. There is no submitted file whose authorship to audit.
        if not submitted.exists():
            continue
        code = submitted.read_text().strip()
        matches = False
        for reply in sorted(tuple_dir(run, arm, task).glob("inv*-lift-coder.response.txt")):
            blocks = re.findall(r"```rust[ \t]*\r?\n(.*?)```", local(reply).read_text(), re.S)
            if code and any(block.strip() == code for block in blocks):
                matches = True
                break
        if not matches:
            violations.append(task)
    return violations


def paired_table(table, x, y):
    return [(fam, fx - table[y, task][0], ax - table[y, task][1])
            for (arm, task), (fx, ax, fam) in sorted(table.items()) if arm == x]


# Controller lane names are global. This transport adapter changes only the
# lane field of complete outgoing model-request lines; source strings, model
# effort, default-lane requests and the harness's own lane comparisons stay
# unchanged. It is needed only when two arms declare conflicting bindings.
LANE_ADAPTER = '''import json as phaseb_json
import sys as phaseb_sys
class PhaseBLaneWriter:
    def __init__(self, stream):
        self.stream = stream
        self.pending = ""
    def write(self, text):
        self.pending += text
        while "\\n" in self.pending:
            line, self.pending = self.pending.split("\\n", 1)
            try:
                message = phaseb_json.loads(line)
                if message.get("op") == "model" and message.get("lane") in PHASEB_LANES:
                    message["lane"] = PHASEB_LANES[message["lane"]]
                    line = phaseb_json.dumps(message)
            except (ValueError, TypeError, AttributeError):
                pass
            self.stream.write(line + "\\n")
        return len(text)
    def flush(self):
        self.stream.flush()
phaseb_sys.stdout = PhaseBLaneWriter(phaseb_sys.stdout)
'''


def bind_lanes(externals, base):
    bindings = {name: ev.lanes_of(path) for name, path in externals.items()}
    combined, conflict = {}, False
    for lanes in bindings.values():
        for name, spec in lanes.items():
            conflict |= name in combined and combined[name] != spec
            combined[name] = spec
    if not conflict:
        return externals, combined
    adapted, combined = {}, {}
    for arm, path in externals.items():
        mapping = {name: f"pc-{arm}-{name}" for name in bindings[arm]}
        if not mapping:
            adapted[arm] = path
            continue
        header = "# lanes: " + " ".join(f"{mapping[name]}={spec}" for name, spec in bindings[arm].items()) + "\n"
        source = "\n".join(line for line in local(path).read_text().splitlines() if not re.match(r"^# lanes:", line)) + "\n"
        code = header + "PHASEB_LANES = " + repr(mapping) + "\n" + LANE_ADAPTER + "\n" + source
        target = local(base / f"external-{arm}.py")
        if target.exists() and target.read_text() != code:
            raise ValueError("registered lane adapter changed")
        target.write_text(code)
        ev.guard(target)
        adapted[arm] = target
        combined.update({mapping[name]: spec for name, spec in bindings[arm].items()})
    return adapted, combined


def run_round(base, suite, externals, lanes, seed, jobs, lift, cap):
    """An infrastructure failure gets one new-seed retry, same proposals.

    Persist failed attempts so repeated CLI invocations cannot give a round
    unlimited retries. Per-task failures do not trigger this retry.
    """
    status_path = base / "run-attempts.json"
    status = read_json(status_path) if status_path.exists() else {"failures": []}
    for attempt, name in enumerate(("run", "run-retry")):
        out = base / name
        if (out / "summary.json").exists():
            summary = read_json(out / "summary.json")
            if not summary.get("aborted", False) and summary.get("integrity_ok", True):
                return out
        if any(failure["attempt"] == attempt for failure in status["failures"]):
            continue
        if out.exists():
            reason = "interrupted, aborted or integrity-invalid run"
        else:
            if not cap(base.name, 15 * len(task_families(suite)), ev.DAILY_CAP):
                return None
            registration = base / f"attempt{attempt + 1}-registration"
            registration.mkdir(exist_ok=True)
            ev.manifest(registration, {"round_manifest_sha": ev.sha(base / "manifest.json"),
                                       "attempt": attempt, "seed": seed + attempt})
            try:
                lift(f"C2-PC-{base.name[5:]}", suite, list(ARMS), externals, lanes, out, seed + attempt, jobs)
                summary = read_json(out / "summary.json")
                if not summary.get("aborted", False) and summary.get("integrity_ok", True):
                    return out
                reason = "aborted or integrity-invalid run"
            except (Exception, SystemExit) as error:
                reason = str(error)
        status["failures"].append({"attempt": attempt, "seed": seed + attempt, "reason": reason})
        write_json(status_path, status)
    raise ValueError("round failed twice; proposals retained, no completed-round scores written")


def cmd_round(args, lift=None, cap=None):
    lift, cap = lift or ev.lift, cap or ev.within_cap
    r, base = args.round, round_dir(args.round)
    snap = freeze()
    base.mkdir(parents=True, exist_ok=True)
    q = quarter(r)
    immutable_json(base / "quarter.json", q)
    suite = ev.make_suite(base / "suite", q["evaluation_families"])
    expected = {t: f for f, tasks in ev.families(ev.DEV).items() if f in q["evaluation_families"] for t in tasks}
    if task_families(suite) != expected:
        raise ValueError("round suite does not match the registered evaluation quarter")
    proposals, externals, source_hashes = {}, {"inc": snap}, {}
    for arm, name in ARM_NAMES.items():
        path = base / arm / "candidate.py"
        data = read_json(base / arm / "candidate.json")
        if ev.sha(path) != data["candidate_sha"]:
            raise ValueError("proposal changed after validation")
        valid, reason = validate_candidate(path)
        preflight_valid = data.get("preflight_valid", data["valid"])
        if preflight_valid and not valid:
            raise ValueError("previously valid proposal failed guard: " + reason)
        proposals[name] = data
        source_hashes[name] = ev.sha(path)
        externals[name] = path if preflight_valid else snap
    externals, lanes = bind_lanes(externals, base)
    seed = int(hashlib.sha256(f"A2D-C2-PC-run-{r}".encode()).hexdigest()[:6], 16)
    manifest = {"protocol": "c2-pc-v2", "round": r, "seed": seed, "arms": list(ARMS),
                "controller": ev.git_head(), "controller_sha": ev.sha(ev.A2D),
                "tool_sha": ev.sha(pathlib.Path(__file__)), "eval_sha": ev.sha(ROOT / "nano/eval.py"),
                "proposer_sha": ev.sha(pathlib.Path(proposer.__file__)),
                "v0_sha": ev.sha(frozen_v0()), "aa_sha": ev.sha(ROOT / "nano/aa.json"),
                "suite_hash": ev.suite_hash(suite), "incumbent_sha": ev.sha(snap),
                "candidate_hashes": source_hashes, "lanes": lanes,
                "external_hashes": {name: ev.sha(path) for name, path in externals.items()},
                "replaced_by_incumbent": [name for name, data in proposals.items()
                                          if not data.get("preflight_valid", data["valid"])]}
    for arm in ARM_NAMES:
        manifest[arm + "_prompt_sha"] = ev.sha(base / arm / "PROMPT.md")
    ev.manifest(base, manifest)
    out = run_round(base, suite, externals, lanes, seed, args.jobs, lift, cap)
    if out is None:
        return
    # Completion here means the controller reached scoring, not that every
    # tuple succeeded. Incomplete score records are deliberately zero-filled.
    table, issues = score_table(out, suite, ARMS)
    issues["authorship"] = {arm: authorship(out, arm, task_families(suite)) for arm in ARMS}
    if issues["authorship"]["X:inc"]:
        write_json(base / "score-issues.json", issues)
        raise ValueError("incumbent failed authorship check; round cannot be analyzed")
    decisions, result_rows = {}, []
    threshold = read_json(ROOT / "nano/aa.json")["threshold_allpass"]
    calls, calls_by_lane, tokens = ev.spend(out)
    for arm, name in ARM_NAMES.items():
        data = proposals[name]
        violations = issues["authorship"]["X:" + name]
        valid = data["valid"] and not violations
        if violations:
            data.setdefault("preflight_valid", data["valid"])
            data.update({"valid": False, "reason": "post-run authorship violation", "authorship_tasks": violations})
            write_json(base / arm / "candidate.json", data)
        # Invalid proposals have exactly zero delta, independent of the random
        # draws of the incumbent replacement arm. They stay in every denominator.
        if not valid:
            for task in task_families(suite):
                table["X:" + name, task] = table["X:inc", task]
        rows = paired_table(table, "X:" + name, "X:inc")
        net = sum(row[2] for row in rows)
        decision = "invalid" if not valid else "screen-pass" if net >= 2 else "screen-fail"
        decisions[name] = {"valid": valid, "decision": decision, "net_allpass": net,
                           "delta_allpass": ev.mean(rows, 2), "delta_frac": ev.mean(rows, 1)}
        result_rows.append({"id": f"PC-{r}", "date": datetime.date.today().isoformat(),
                            "proposer": "pc-" + arm, "stage": "round", "hypothesis": data.get("hypothesis", ""),
                            "delta_frac": ev.mean(rows, 1), "se": ev.cluster_se(rows, 1),
                            "delta_allpass": ev.mean(rows, 2), "se_allpass": ev.cluster_se(rows, 2),
                            "threshold": threshold, "n_tasks": len(rows), "n_families": len({row[0] for row in rows}),
                            "decision": decision, "invocations": calls, "calls_by_lane": json.dumps(calls_by_lane, sort_keys=True),
                            "tokens": tokens, "candidate_sha": source_hashes[name],
                            "note": "incumbent substitution; delta forced to zero" if not valid else ""})
    payload = {"round": r, "decisions": decisions,
               "scores": [{"arm": a, "task": t, "family": f, "fraction": frac, "all_passed": ok}
                          for (a, t), (frac, ok, f) in sorted(table.items())]}
    immutable_json(base / "scores.json", payload)
    write_json(base / "score-issues.json", issues)
    # Re-running a cached round must not duplicate its log rows.
    existing = list(csv.DictReader(io.StringIO(local(ROOT / "nano/results.tsv").read_text()), delimiter="\t"))
    for row in result_rows:
        if not any(old["id"] == row["id"] and old["stage"] == "round" and old["proposer"] == row["proposer"] for old in existing):
            ev.log_result(row)
    print(json.dumps({"round": r, "decisions": decisions}, indent=2))


def sign_test(values):
    positive = sum(v > 1e-12 for v in values)
    negative = sum(v < -1e-12 for v in values)
    n = positive + negative
    p = min(1.0, 2 * sum(math.comb(n, k) for k in range(min(positive, negative) + 1)) / 2 ** n) if n else 1.0
    return {"positive": positive, "negative": negative, "ties": len(values) - n, "p_two_sided": p}


def precision(rows, index):
    if not rows:
        return {"difference": None, "ci90": None, "round_differences": [], "sign_test": sign_test([])}
    by_cluster, by_round = collections.defaultdict(list), collections.defaultdict(list)
    for r, family, frac, allpass in rows:
        value = (frac, allpass)[index]
        by_cluster[r, family].append(value)
        by_round[r].append(value)
    rng = random.Random(17)
    clusters = sorted(by_cluster)
    boot = []
    for _ in range(4000):
        vals = [value for _ in clusters for value in by_cluster[rng.choice(clusters)]]
        boot.append(sum(vals) / len(vals))
    boot.sort()
    differences = [sum(vals) / len(vals) for _, vals in sorted(by_round.items())]
    return {"difference": sum(row[2 + index] for row in rows) / len(rows),
            "ci90": [boot[int(.05 * 3999)], boot[int(.95 * 3999)]],
            "round_differences": differences, "sign_test": sign_test(differences)}


def analyze(rounds, threshold):
    rows, deltas, valid_counts, passes = [], {"inf": [], "uninf": []}, collections.Counter(), collections.Counter()
    for result in sorted(rounds, key=lambda d: d["round"]):
        table = {(rec["arm"], rec["task"]): (rec["fraction"], rec["all_passed"], rec["family"])
                 for rec in result["scores"]}
        tasks = sorted(t for a, t in table if a == "X:inc")
        if len(table) != len(tasks) * 3 or any((a, t) not in table for a in ARMS for t in tasks):
            raise ValueError("incomplete round score table")
        for name in deltas:
            valid = result["decisions"][name]["valid"]
            if not valid:
                for task in tasks:
                    table["X:" + name, task] = table["X:inc", task]
            delta = sum(table["X:" + name, task][1] - table["X:inc", task][1] for task in tasks)
            deltas[name].append(delta / len(tasks))
            valid_counts[name] += valid
            passes[name] += valid and delta >= 2
        for task in tasks:
            fi, ai, fam = table["X:inf", task]
            fu, au, _ = table["X:uninf", task]
            rows.append((result["round"], fam, fi - fu, ai - au))
    n = len(rounds)
    return {"n_rounds": n, "n_task_runs": len(rows), "minimum_rounds_met": n >= 4,
            "allpass": precision(rows, 1), "hidden_fraction": precision(rows, 0),
            "arms": {name: {"n_proposals": n, "max_delta_allpass": max(vals) if vals else None,
                            "count_above_aa_threshold": sum(d > threshold for d in vals),
                            "screen_pass_rate": passes[name] / n if n else None,
                            "invalid_count": n - valid_counts[name]} for name, vals in deltas.items()},
            "threshold_allpass": threshold,
            "limitation": "Screen-pass rates are descriptive and underpowered; inconclusive is expected at this budget."}


def cmd_analyze(args):
    rounds = [read_json(round_dir(r) / "scores.json") for r in range(1, 7) if (round_dir(r) / "scores.json").exists()]
    data = analyze(rounds, read_json(ROOT / "nano/aa.json")["threshold_allpass"])
    write_json(phase_dir() / "analysis.json", data)
    print(json.dumps(data, indent=2))


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    commands = ap.add_subparsers(dest="command", required=True)
    for name in ("quarters", "stats", "dossiers", "brief", "propose", "round", "analyze"):
        parser = commands.add_parser(name)
        parser.set_defaults(function=globals()["cmd_" + name])
        if name in {"quarters", "dossiers", "brief", "propose", "round"}:
            parser.add_argument("--round", type=int, choices=range(1, 7), required=True)
        if name in {"stats", "dossiers"}:
            parser.add_argument("--runs", nargs="+", required=True)
            parser.add_argument("--arm", default="X:cand")
        if name in {"brief", "propose"}:
            parser.add_argument("--arm", choices=ARM_NAMES, required=True)
        if name in {"stats", "dossiers", "brief"}:
            parser.add_argument("--out", required=True)
        if name == "stats":
            parser.add_argument("--pool", nargs="+", help="candidate_pool.py pooled JSON, default: <run>/pool.json")
        if name == "dossiers":
            parser.add_argument("--max", type=int, default=8)
        if name == "brief":
            parser.add_argument("--stats")
            parser.add_argument("--dossiers")
        if name == "round":
            parser.add_argument("--jobs", type=int, default=6)
    args = ap.parse_args()
    try:
        args.function(args)
    except (ValueError, OSError, AssertionError, KeyError) as error:
        ap.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
