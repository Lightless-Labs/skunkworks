"""T0 staging: one dir per D pick that is public-green but hidden-fail on
c1-train (E07 + E07b), holding the spec, the candidate, the hidden tests and
the failing-test output.

usage: python3 -I t0_stage.py <pool.json> <suite-dir> <out-dir>
"""
import collections, json, pathlib, re, subprocess, sys, tempfile

RUSTC = str(pathlib.Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc")
pool_path, suite, out = (pathlib.Path(a) for a in sys.argv[1:4])

PROMPT = """You are auditing why a Rust solution passed its visible tests but failed hidden acceptance tests.
Files in the current directory: SPEC.md (the task requirements, numbered clauses R1..Rn), candidate.rs (the submitted solution),
hidden_tests.rs (the acceptance tests; each test is preceded by a comment naming the clauses it checks) and failures.txt (which hidden tests failed and their panic output).
Find the root cause of each failure in candidate.rs. Do not modify any file. Reply with ONE JSON object and nothing else:
{"violated_clauses": ["R7", ...],
 "category": one of "error-precedence" | "boundary-edge-case" | "unit-or-offset-counting" | "output-format-or-ordering" | "state-or-sequencing" | "numeric-overflow-or-precision" | "performance-timeout" | "missing-requirement" | "test-or-spec-ambiguity" | "other",
 "root_cause": "one sentence, specific to this code",
 "spec_was_clear": true or false,
 "clause_check_would_catch": true or false (would a careful pass over every numbered clause, before coding, have caught it?),
 "general_lesson": "a TASK-AGNOSTIC instruction to a programmer (no names from this task, at most 25 words) that would have prevented this failure, or null if none would"}
"""


def code_of(path):
    m = re.search(r"```rust(.*?)```", pathlib.Path(path).read_text(), re.S)
    return m.group(1).strip() if m else None


def run_hidden(code, tests, timeout=30):
    with tempfile.TemporaryDirectory() as d:
        d = pathlib.Path(d)
        (d / "c.rs").write_text('#[cfg(test)]\n#[path = "t.rs"]\nmod oracle;\n' + code)
        (d / "t.rs").write_text("#![allow(unused_imports, dead_code, unused_variables)]\nuse super::*;\n" + tests)
        c = subprocess.run([RUSTC, "--test", "--edition", "2024", str(d / "c.rs"), "-o", str(d / "b")], capture_output=True, text=True)
        if c.returncode:
            return "COMPILE ERROR\n" + c.stderr[-4000:]
        try:
            r = subprocess.run([str(d / "b"), "--test-threads", "1"], capture_output=True, text=True, timeout=timeout)
            text = r.stdout + r.stderr
        except subprocess.TimeoutExpired:
            return per_test(d / "b")
        failed = re.findall(r"^test (oracle::\w+) \.\.\. FAILED", text, re.M)
        detail = text[text.find("failures:"):] if "failures:" in text else text
        return "FAILED TESTS: " + ", ".join(failed) + "\n\n" + detail[:6000]


def per_test(binary, timeout=10):
    """The whole binary hung: run each test alone to find the hung and failing ones."""
    names = re.findall(r"^(oracle::\w+): test", subprocess.run([str(binary), "--list"], capture_output=True, text=True).stdout, re.M)
    lines, details = [], []
    for name in names:
        try:
            r = subprocess.run([str(binary), name, "--exact"], capture_output=True, text=True, timeout=timeout)
            if r.returncode:
                lines.append(name + " FAILED")
                out = r.stdout + r.stderr
                details.append(out[out.find("---- "):][:1500])
        except subprocess.TimeoutExpired:
            lines.append(name + " TIMEOUT (> %ss alone)" % timeout)
    return "PER-TEST RUN (the full hidden binary exceeded its time limit)\n" + "\n".join(lines) + "\n\n" + "\n".join(details)[:6000]


pool = json.loads(pool_path.read_text())
by = collections.defaultdict(list)
for c in pool:
    if c["arm"] == "D" and c["has_code"]:
        by[(c["run"], c["task"])].append(c)
picks = [max(v, key=lambda c: (c["public"]["passed"], c["public"]["compiled"], c["inv"])) for v in by.values()]
cases = sorted((c for c in picks if c["public"]["ok"] and not c["hidden"]["ok"]), key=lambda c: (c["task"], c["run"]))
out.mkdir(parents=True, exist_ok=True)
for c in cases:
    d = out / f"{c['task']}--{c['run']}"
    d.mkdir(exist_ok=True)
    task = suite / c["task"]
    code = code_of(c["code_path"])
    tests = (task / "hidden_tests.rs").read_text()
    (d / "SPEC.md").write_text(json.loads((task / "task.json").read_text())["requirements"])
    (d / "candidate.rs").write_text(code)
    (d / "hidden_tests.rs").write_text(tests)
    (d / "failures.txt").write_text(run_hidden(code, tests))
    (d / "PROMPT.md").write_text(PROMPT)
print(len(cases), "cases staged in", out)
