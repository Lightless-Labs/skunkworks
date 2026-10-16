"""Cycle 2 proposer: ask an external model (no tools, everything inline) for
candidate nanoharness changes against the current incumbent.

usage: python3 -I c2_propose.py --round <name> --stats <aggregate.json> [--model gpt-6.1-sol] [--effort high]

The proposer sees only: the incumbent and v0 harness files, the protocol,
the isolation guard, nano/seeds.md, nano/results.tsv, the cycle-2 LOG
section, and aggregate failure statistics. Never hidden tests, per-task
hidden outcomes, or holdout data. The call runs in an empty temporary
directory with Codex's shell, exec, web and app tools disabled, so the
prompt is its only input. Candidates are written to
nano/candidates/<round>-<n>-<slug>.py with the reply kept beside them.
"""
import argparse, json, pathlib, re, subprocess, sys, tempfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
NANO = ROOT / "nano"
DISABLED = ["shell_tool", "unified_exec", "js_repl", "web_search_request", "standalone_web_search",
            "browser_use", "computer_use", "apps"]

PROTOCOL = """\
## The harness protocol (controller side is fixed)
- One Python 3 process per task, run with `python3 -I` in an empty directory. JSON lines over stdin/stdout.
- First line in: {"task": {"id", "family", "task_prompt", "coder_system", "k", "max_feedback_chars"}}.
  task_prompt = the full task as the coder sees it (numbered requirements + the visible tests). k = 5.
- Requests (one JSON object per line on stdout; one reply line on stdin):
  - {"op": "model", "system": str, "prompt": str} -> {"ok": bool, "text": str, "error"?}. Default lane = the coder,
    codex/gpt-6-luna at low effort ("Luna"). Add "lane": "<name>" to call a lane declared in a header comment
    `# lanes: <name>=codex/<model>:<effort>`; the stronger model available is codex/gpt-6.1-sol ("Sol").
    EVERY model call, on any lane, counts against the same per-task cap k = 5. Calls beyond k fail.
    Model calls have no tools and no memory between calls.
  - {"op": "extract", "text": str} -> {"code": str | null}   (the rust code block of a reply)
  - {"op": "public_test", "code": str} -> {"compiled": bool, "passed": int, "expected": int, "all_passed": bool,
    "output": str}   (visible tests only; free, not metered)
  - {"op": "done", "codes": [str, ...]} -> ends the task. The controller public-tests each code and submits the
    one with the best public result (compiled, then tests passed); ties go to the LATER code in the list.
- Score: hidden acceptance tests (never visible to the harness) on the submitted file; primary metric = all-pass
  (every hidden test passes), secondary = fraction of hidden tests passed.
- Isolation guard (static, enforced before any run): imports only from {json, re, sys, random, math,
  itertools, collections, functools, hashlib, difflib, textwrap, string, heapq, dataclasses, typing};
  no calls to open, exec, eval, compile, __import__, globals, locals, vars, getattr, setattr, delattr,
  input, breakpoint; no dunder attribute access except __name__. Diagnostics go to stderr as lines
  starting with "DIAG ".
"""

RULES = """\
## Rules for proposals
- Outcome: raise the all-pass rate over the INCUMBENT at the same cap (k = 5 calls per task, any lane).
- The coder stays Luna: every submitted file must be written by a Luna call. Sol may read, judge, review,
  critique, analyse the spec or give feedback, but must not write solution code that gets submitted.
- One idea per candidate (a single-variable change against the incumbent), so the experiment attributes
  its effect. A coupled change is allowed only if the parts cannot work separately; say why.
- Each candidate is a complete replacement for the incumbent file that passes the guard.
- The keep rule is strict (screen on half the dev set, full set, then a fresh replicate; the full-stage
  all-pass gain must clear about +5 pp and 1.645 standard errors). Prefer ideas with a plausible large effect.
"""

ASK = """\
## Your task
Propose THREE candidates, ranked by expected all-pass gain over the incumbent. For each, give:
1. a heading line `### CANDIDATE <n>: <slug>` (slug: lowercase letters, digits and hyphens);
2. `Hypothesis:` one sentence;
3. `Mechanism:` why it should work, tied to the evidence above;
4. `Expected:` the all-pass change you expect and the main risk;
5. the complete Python file in one ```python block.
"""


def section(title, body):
    return f"## {title}\n\n{body.strip()}\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--round", required=True)
    ap.add_argument("--stats", required=True)
    ap.add_argument("--model", default="gpt-6.1-sol")
    ap.add_argument("--effort", default="high")
    ap.add_argument("--extract-only", action="store_true", help="re-parse an existing reply.md")
    args = ap.parse_args()
    if args.extract_only:
        out_dir = ROOT / "runs" / f"c2-propose-{args.round}"
        extract(args.round, out_dir, (out_dir / "reply.md").read_text())
        return
    log = (ROOT / "docs/experiments/LOG.md").read_text()
    cycle2 = log[log.index("## Cycle 2"):]
    # Holdout metadata (family names, paths, hashes, look rules and outcomes)
    # never reaches a proposer: holdout and look entries are dropped whole,
    # and any other line that mentions a holdout is dropped.
    entries = re.split(r"(?m)^(?=### )", cycle2)
    entries = [e for e in entries if not re.match(r"### (Holdouts|C2-LOOK)", e)]
    cycle2 = "".join("".join(l for l in e.splitlines(keepends=True)
                             if not re.search(r"holdout|look \d|C2-LOOK", l, re.I)) for e in entries)
    stats = json.loads(pathlib.Path(args.stats).read_text())
    prompt = "\n".join([
        "You are one of several proposers in an autoresearch loop on a minimal coding harness (A²D cycle 2).",
        "Everything you need is in this message; you have no tools and no files.\n",
        PROTOCOL,
        section("The incumbent (nano/harness.py, E002: 4 Luna samples + Sol judge)",
                "```python\n" + (NANO / "harness.py").read_text() + "```"),
        section("The baseline D (Luna's plain repair loop, as harness v0)",
                "```python\n" + (NANO / "history/before-E002b.py").read_text() + "```"),
        section("Aggregate evidence about the incumbent (dev set; no per-task data)",
                "```json\n" + json.dumps(stats, indent=1) + "\n```"),
        section("Seed list (human insight and prior evidence)", (NANO / "seeds.md").read_text()),
        section("Results log (nano/results.tsv)", "```\n" + (NANO / "results.tsv").read_text() + "```"),
        section("Experiment log, cycle 2", cycle2),
        RULES,
        ASK,
    ])
    out_dir = ROOT / "runs" / f"c2-propose-{args.round}"
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "PROMPT.md").write_text(prompt)
    with tempfile.TemporaryDirectory() as empty:
        cmd = ["codex", "exec", prompt, "-m", args.model, "-c", f'model_reasoning_effort="{args.effort}"',
               "-c", 'approval_policy="never"', "--sandbox", "read-only", "--skip-git-repo-check", "--ephemeral",
               "--ignore-user-config", "--ignore-rules", "-o", str(pathlib.Path(empty) / "reply.md")]
        for f in DISABLED:
            cmd += ["--disable", f]
        r = subprocess.run(cmd, cwd=empty, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=3600)
        (out_dir / "codex.log").write_text(r.stdout[-20000:] + "\n--- stderr ---\n" + r.stderr[-20000:])
        reply_path = pathlib.Path(empty) / "reply.md"
        if r.returncode or not reply_path.exists():
            sys.exit(f"proposer failed ({r.returncode}); see {out_dir}/codex.log")
        reply = reply_path.read_text()
    (out_dir / "reply.md").write_text(reply)
    extract(args.round, out_dir, reply)


def extract(round_name, out_dir, reply):
    parts = re.split(r"^### CANDIDATE (\d+):\s*([a-z0-9-]+)\s*$", reply, flags=re.M)
    made = []
    for i in range(1, len(parts) - 2, 3):
        n, slug, body = parts[i], parts[i + 1], parts[i + 2]
        # The closing fence is a line of its own; fences inside string literals are not.
        code = re.search(r"^```python[ \t]*\n(.*?)^```[ \t]*$", body, re.S | re.M)
        hyp = re.search(r"Hypothesis:\s*(.+)", body)
        if not code:
            continue
        path = NANO / "candidates" / f"{round_name}-{n}-{slug}.py"
        path.write_text(code.group(1))
        made.append({"n": int(n), "slug": slug, "path": str(path.relative_to(ROOT)),
                     "hypothesis": hyp.group(1).strip() if hyp else ""})
    (out_dir / "candidates.json").write_text(json.dumps(made, indent=1) + "\n")
    print(json.dumps(made, indent=1))


if __name__ == "__main__":
    main()
