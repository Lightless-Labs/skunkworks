"""Score every existing coder candidate (E07/E07b) on a development task set
with public and hidden tests (oracle layout: candidate root + #[path] test
module). Development data only. Writes a JSON pool for offline analyses.

usage: python3 -I candidate_pool.py <suite-dir> <out.json> <run-dir>...
"""
import concurrent.futures, json, pathlib, re, subprocess, sys, tempfile
RUSTC = str(pathlib.Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc")
suite = pathlib.Path(sys.argv[1]); out = pathlib.Path(sys.argv[2]); runs = [pathlib.Path(r) for r in sys.argv[3:]]

def extract(text):
    m = re.search(r"```rust(.*?)```", text, re.S)
    return m.group(1).strip() if m else None

def _sb(path):
    return '"' + str(path).replace("\\", "\\\\").replace('"', '\\"') + '"'

def run_profile(binary, workdir):
    """The oracle's run-step seatbelt profile (crates/a2d-core/src/sandbox.rs): no network,
    no file access under HOME, TMPDIR or the temp roots except this step's own directory,
    and no process spawning, Mach lookups or Apple Events beyond the binary itself."""
    import os
    denied = ["/private/tmp", "/private/var/folders", "/private/var/tmp"]
    denied += [os.path.realpath(os.environ[n]) for n in ("HOME", "TMPDIR") if os.environ.get(n)]
    binary = os.path.realpath(binary)
    return ("(version 1)(allow default)(deny network*)(deny file-read* file-write* "
            + " ".join(f"(subpath {_sb(d)})" for d in denied) + ")"
            + f"(deny process-fork process-exec* mach-lookup appleevent-send)"
            + f"(allow process-exec (literal {_sb(binary)}))(allow file-read* (literal {_sb(binary)}))"
            + f"(allow file-read* file-write* (subpath {_sb(os.path.realpath(workdir))}))")

def score(code, tests, timeout):
    with tempfile.TemporaryDirectory() as d:
        d = pathlib.Path(d); lines = code.splitlines(); attrs = []
        while lines and (lines[0].strip().startswith("#![") or lines[0].strip().startswith("//") or not lines[0].strip()):
            attrs.append(lines.pop(0))
        (d / "c.rs").write_text("\n".join(attrs) + "\n#[cfg(test)]\n#[path = \"t.rs\"]\nmod oracle;\n" + "\n".join(lines))
        (d / "t.rs").write_text("#![allow(unused_imports, dead_code)]\nuse super::*;\n" + tests)
        if subprocess.run([RUSTC, "--test", "--edition", "2024", str(d / "c.rs"), "-o", str(d / "b")], capture_output=True).returncode:
            return {"compiled": False, "passed": 0, "ok": False}
        try:
            # Model-written code runs only under the oracle's run-step sandbox (C2-ISO2).
            r = subprocess.run(["/usr/bin/sandbox-exec", "-p", run_profile(d / "b", d), str(d / "b"), "oracle::"],
                               capture_output=True, timeout=timeout, text=True, cwd=d, env={})
        except subprocess.TimeoutExpired:
            return {"compiled": True, "passed": 0, "ok": False, "timed_out": True}
        m = re.findall(r"test result: \w+\. (\d+) passed", r.stdout)
        return {"compiled": True, "passed": int(m[-1]) if m else 0, "ok": r.returncode == 0}

jobs = []
tasks = {d.name: d for d in suite.iterdir() if (d / "task.json").exists()}
for run in runs:
    for arm_dir in sorted((run / "invocations").iterdir()):
        for task_dir in sorted(arm_dir.iterdir()):
            if task_dir.name not in tasks:
                continue
            for f in sorted((task_dir / "r0").glob("inv*-*coder.response.txt")):
                jobs.append((run.name, arm_dir.name, task_dir.name, f.name.split("-")[0], f))

def work(job):
    run, arm, task, inv, f = job
    t = tasks[task]; meta = json.loads((t / "task.json").read_text())
    code = extract(f.read_text())
    rec = {"run": run, "arm": arm, "task": task, "family": meta["family"], "inv": inv, "has_code": code is not None}
    if code is not None:
        rec["public"] = score(code, (t / "public_tests.rs").read_text(), meta.get("test_timeout_secs", 10))
        rec["hidden"] = score(code, (t / "hidden_tests.rs").read_text(), meta.get("test_timeout_secs", 10))
        rec["code_path"] = str(f)
    return rec

with concurrent.futures.ThreadPoolExecutor(4) as ex:
    pool = list(ex.map(work, jobs))
out.write_text(json.dumps(pool))
print(len(pool), "candidates over", len({p["task"] for p in pool}), "tasks")
