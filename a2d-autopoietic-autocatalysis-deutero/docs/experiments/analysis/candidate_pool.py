"""Score every existing coder candidate (E07/E07b) on a development task set
with public and hidden tests (oracle layout: candidate root + #[path] test
module). Development data only. Writes a JSON pool for offline analyses.

usage: python3 -I candidate_pool.py [--all-blocks] <suite-dir> <out.json> <run-dir>...

--all-blocks: one record per parsed file of a reply, by E010's parse_files rule (fences on
their own lines, deduplicated, at most the two longest, in reply order); the second file's
record is <inv>b. For harnesses whose coder calls return two files (E010). Default: the
first ```rust block of each reply, as before.
"""
import concurrent.futures, json, pathlib, re, subprocess, sys, tempfile
RUSTC = str(pathlib.Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc")
ALL_BLOCKS = len(sys.argv) > 1 and sys.argv[1] == "--all-blocks"
args = sys.argv[2:] if ALL_BLOCKS else sys.argv[1:]
suite = pathlib.Path(args[0]); out = pathlib.Path(args[1]); runs = [pathlib.Path(r) for r in args[2:]]

FENCE = r"^[ \t]*```(?:rust|rs)[^\n]*\n(.*?)^[ \t]*```[ \t]*$"

def extract_all(text):
    """E010's parse_files: at most two blocks (the longest), in reply order."""
    blocks = []
    for body in re.findall(FENCE, text, re.S | re.M):
        if body.strip() and body not in blocks:
            blocks.append(body)
    if len(blocks) > 2:
        longest = sorted(range(len(blocks)), key=lambda i: len(blocks[i]), reverse=True)[:2]
        blocks = [blocks[i] for i in sorted(longest)]
    return [b.strip() for b in blocks]

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
                inv = f.name.split("-")[0]
                n = max(1, len(extract_all(f.read_text()))) if ALL_BLOCKS else 1
                for block in range(n):
                    jobs.append((run.name, arm_dir.name, task_dir.name, inv + ("b" if block else ""), f, block))

def work(job):
    run, arm, task, inv, f, block = job
    t = tasks[task]; meta = json.loads((t / "task.json").read_text())
    if ALL_BLOCKS:
        blocks = extract_all(f.read_text())
        code = blocks[block] if block < len(blocks) else None
    else:
        code = extract(f.read_text())
    rec = {"run": run, "arm": arm, "task": task, "family": meta["family"], "inv": inv, "has_code": code is not None}
    if code is not None:
        rec["public"] = score(code, (t / "public_tests.rs").read_text(), meta.get("test_timeout_secs", 10))
        rec["hidden"] = score(code, (t / "hidden_tests.rs").read_text(), meta.get("test_timeout_secs", 10))
        rec["code_path"] = str(f)
        if ALL_BLOCKS:
            rec["block"] = block
    return rec

with concurrent.futures.ThreadPoolExecutor(4) as ex:
    pool = list(ex.map(work, jobs))
out.write_text(json.dumps(pool))
print(len(pool), "candidates over", len({p["task"] for p in pool}), "tasks")
