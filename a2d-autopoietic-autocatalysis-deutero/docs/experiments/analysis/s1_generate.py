"""S1: blind spec-test generation (development lane). The test-writer sees
only the requirements and public tests (API shape), never an implementation.
Same Codex no-tools lane as the controller. One call per task."""
import concurrent.futures, json, pathlib, re, subprocess, sys, tempfile, time
suite = pathlib.Path(sys.argv[1]); out = pathlib.Path(sys.argv[2])
DISABLED = ["shell_tool", "unified_exec", "js_repl", "web_search_request", "standalone_web_search", "browser_use", "computer_use", "apps"]
SYSTEM = ("You write Rust unit tests from a written specification. You never see an implementation. "
          "Respond with Rust test functions in a single ```rust fenced block and nothing else.")
INSTRUCTIONS = """Write unit tests for the specification above.

Rules:
- Write `#[test]` functions only (no module wrapper, no `use` lines needed): they will be placed in a module with `use super::*;` next to an implementation of the specified public API.
- Use ONLY the public API exactly as specified (names, signatures, error variants). The public tests show its shape.
- For EVERY numbered requirement clause (R1, R2, ...) write at least one test of the exact behaviour the clause determines: boundaries, error variants and their precedence, ordering and tie-breaks, formats, interactions between clauses. Name tests `spec_r<number>_<short_name>`.
- Never test behaviour the specification does not determine. Prefer exact expected values derived carefully from the text.
- Do not repeat the public tests."""

def call(task_dir):
    meta = json.loads((task_dir / "task.json").read_text())
    public = (task_dir / "public_tests.rs").read_text()
    prompt = f"{meta['requirements']}\n\n## Public tests (API shape)\n\n```rust\n{public}\n```\n\n{INSTRUCTIONS}"
    args = ["codex", "exec", f"{SYSTEM}\n\n{prompt}", "-m", "gpt-6-luna", "-c", 'model_reasoning_effort="low"',
            "--sandbox", "read-only", "--skip-git-repo-check", "--ephemeral", "--ignore-user-config", "--ignore-rules"]
    for f in DISABLED: args += ["--disable", f]
    start = time.time()
    with tempfile.TemporaryDirectory() as cwd:
        try:
            r = subprocess.run(args, cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=300)
            stdout, stderr, ok = r.stdout, r.stderr, r.returncode == 0
        except subprocess.TimeoutExpired:
            stdout, stderr, ok = "", "TIMEOUT", False
    tok = re.findall(r"tokens used\s*\n\s*([\d,]+)", stderr)
    m = re.search(r"```rust(.*?)```", stdout, re.S)
    (out / f"{task_dir.name}.response.txt").write_text(stdout)
    if m: (out / f"{task_dir.name}.spec_tests.rs").write_text(m.group(1).strip() + "\n")
    return {"task": task_dir.name, "ok": ok, "has_tests": bool(m), "tests": (m.group(1).count("#[test]") if m else 0),
            "tokens": int(tok[-1].replace(",", "")) if tok else None, "secs": round(time.time() - start, 1),
            "tool_calls": sum(1 for l in stderr.splitlines() if l.strip() == "exec")}

tasks = sorted(d for d in suite.iterdir() if (d / "task.json").exists())
with concurrent.futures.ThreadPoolExecutor(4) as ex:
    records = list(ex.map(call, tasks))
(out / "generation.json").write_text(json.dumps(records, indent=1))
print(len(records), "calls; ok", sum(r["ok"] for r in records), "; with tests", sum(r["has_tests"] for r in records),
      "; tests", sum(r["tests"] for r in records), "; tokens", sum(r["tokens"] or 0 for r in records), "; tool calls", sum(r["tool_calls"] for r in records))
