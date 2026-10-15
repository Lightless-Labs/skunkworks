"""S2: clause-by-clause review-and-fix applied to D's public-green picks
(development lane). Reverts to the original if the fix breaks public tests.

usage: python3 -I s2_review.py <suite-dir> <pool.json> <out-dir>
"""
import collections, concurrent.futures, json, pathlib, re, subprocess, sys, tempfile, time
RUSTC = str(pathlib.Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc")
DISABLED = ["shell_tool", "unified_exec", "js_repl", "web_search_request", "standalone_web_search", "browser_use", "computer_use", "apps"]
suite, pool_path, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
SYSTEM = ("You are a meticulous Rust engineer reviewing an implementation against a numbered specification. "
          "Respond with a clause checklist followed by the complete corrected file in a single ```rust fenced block.")
INSTRUCTIONS = """Review the implementation above against EVERY numbered requirement clause (R1, R2, ...).

1. Checklist: one line per clause, `R<n>: OK` or `R<n>: GAP — <what is wrong or missing>`, citing the function that implements it. Be skeptical: check exact error variants and their precedence, boundaries, ordering and tie-breaks, output formats, and interactions between clauses. The public tests already pass; look for what they do not cover.
2. Then output the COMPLETE corrected file (all of it, not a diff) in a single ```rust block. Fix every GAP. If everything is OK, output the file unchanged. Keep the public API exactly as specified and keep the public tests passing."""

def extract(text):
    m = re.search(r"```rust(.*?)```", text, re.S)
    return m.group(1).strip() if m else None

def score(code, tests, timeout):
    with tempfile.TemporaryDirectory() as d:
        d = pathlib.Path(d); lines = code.splitlines(); attrs = []
        while lines and (lines[0].strip().startswith(("#![", "//")) or not lines[0].strip()):
            attrs.append(lines.pop(0))
        (d / "c.rs").write_text("\n".join(attrs) + '\n#[cfg(test)]\n#[path = "t.rs"]\nmod oracle;\n' + "\n".join(lines))
        (d / "t.rs").write_text("#![allow(unused_imports, dead_code)]\nuse super::*;\n" + tests)
        if subprocess.run([RUSTC, "--test", "--edition", "2024", str(d / "c.rs"), "-o", str(d / "b")], capture_output=True).returncode:
            return False
        try:
            return subprocess.run([str(d / "b"), "oracle::"], capture_output=True, timeout=timeout).returncode == 0
        except subprocess.TimeoutExpired:
            return False

def review(c):
    t = suite / c["task"]; meta = json.loads((t / "task.json").read_text())
    public = (t / "public_tests.rs").read_text(); hidden = (t / "hidden_tests.rs").read_text()
    code = extract(pathlib.Path(c["code_path"]).read_text())
    prompt = (f"{meta['requirements']}\n\n## Public tests (all passing)\n\n```rust\n{public}\n```\n\n"
              f"## Implementation under review\n\n```rust\n{code}\n```\n\n{INSTRUCTIONS}")
    args = ["codex", "exec", f"{SYSTEM}\n\n{prompt}", "-m", "gpt-6-luna", "-c", 'model_reasoning_effort="low"',
            "--sandbox", "read-only", "--skip-git-repo-check", "--ephemeral", "--ignore-user-config", "--ignore-rules"]
    for f in DISABLED: args += ["--disable", f]
    start = time.time()
    with tempfile.TemporaryDirectory() as cwd:
        try:
            r = subprocess.run(args, cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=300)
            stdout, stderr = r.stdout, r.stderr
        except subprocess.TimeoutExpired:
            stdout, stderr = "", "TIMEOUT"
    name = f"{c['run']}-{c['task']}"
    (out / f"{name}.response.txt").write_text(stdout)
    fixed = extract(stdout)
    timeout = meta.get("test_timeout_secs", 10)
    fixed_public = bool(fixed) and score(fixed, public, timeout)
    final = fixed if fixed_public else code
    tok = re.findall(r"tokens used\s*\n\s*([\d,]+)", stderr)
    return {"run": c["run"], "task": c["task"], "before_hidden": c["hidden"]["ok"],
            "changed": bool(fixed) and fixed.strip() != code.strip(), "fixed_public": fixed_public,
            "after_hidden": score(final, hidden, timeout), "gaps": len(re.findall(r"(?m)^\s*\**R\d+\**\s*:\s*\**GAP", stdout)),
            "tokens": int(tok[-1].replace(",", "")) if tok else None, "secs": round(time.time() - start, 1),
            "tool_calls": sum(1 for l in stderr.splitlines() if l.strip() == "exec")}

pool = json.loads(pool_path.read_text())
by = collections.defaultdict(list)
for c in pool:
    if c["arm"] == "D" and c["has_code"]: by[(c["run"], c["task"])].append(c)
picks = [max(v, key=lambda c: (c["public"]["passed"], c["public"]["compiled"], c["inv"])) for v in by.values()]
green = [p for p in picks if p["public"]["ok"]]
with concurrent.futures.ThreadPoolExecutor(4) as ex:
    recs = list(ex.map(review, green))
(out / "results.json").write_text(json.dumps(recs, indent=1))
hf = [r for r in recs if not r["before_hidden"]]; hp = [r for r in recs if r["before_hidden"]]
conv = sum(r["after_hidden"] for r in hf); regr = sum(not r["after_hidden"] for r in hp)
end = sum(p["hidden"]["ok"] for p in picks if not p["public"]["ok"]) + sum(r["after_hidden"] for r in recs)
print(f"calls {len(recs)}; changed {sum(r['changed'] for r in recs)}; reverted (fix broke public or no code) {sum(not r['fixed_public'] for r in recs)}; tokens {sum(r['tokens'] or 0 for r in recs)}; tool calls {sum(r['tool_calls'] for r in recs)}")
print(f"CONV = {conv}/{len(hf)} = {conv/max(len(hf),1):.3f}   [kill if < 0.25; break-even ~0.43]")
print(f"REGR = {regr}/{len(hp)} = {regr/max(len(hp),1):.3f}   [kill if > 0.05]")
print(f"END: D + review = {end}/{len(picks)} = {end/len(picks):.3f} vs D {sum(p['hidden']['ok'] for p in picks)/len(picks):.3f}   [promotion bar >= 0.793]")
