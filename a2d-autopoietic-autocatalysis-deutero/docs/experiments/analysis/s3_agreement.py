"""S3: CodeT-style dual-agreement selection using blind spec tests
(development lane). Caches per-test pass vectors of B's candidates.

usage: python3 -I s3_agreement.py <suite-dir> <spec-dir> <pool.json> <cache.json>
"""
import collections, concurrent.futures, json, pathlib, re, subprocess, sys, tempfile

RUSTC = str(pathlib.Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc")
suite, spec_dir, pool_path, cache_path = (pathlib.Path(a) for a in sys.argv[1:5])
RESULT = re.compile(r"^test oracle::(\w+) \.\.\. (ok|FAILED)", re.M)


def split_tests(src):
    """Prelude (helpers before the first test) and one chunk per #[test]."""
    starts = [m.start() for m in re.finditer(r"(?m)^\s*#\[test\]", src)]
    if not starts:
        return src, []
    prelude = src[: starts[0]]
    chunks = [src[a:b] for a, b in zip(starts, starts[1:] + [len(src)])]
    return prelude, chunks


def run(code, tests, timeout=20):
    """Compile `code` with `tests` as a #[path] test module; return
    (compiled, {test_name: passed})."""
    with tempfile.TemporaryDirectory() as d:
        d = pathlib.Path(d)
        lines = code.splitlines()
        attrs = []
        while lines and (lines[0].strip().startswith(("#![", "//")) or not lines[0].strip()):
            attrs.append(lines.pop(0))
        (d / "c.rs").write_text("\n".join(attrs) + '\n#[cfg(test)]\n#[path = "t.rs"]\nmod oracle;\n' + "\n".join(lines))
        (d / "t.rs").write_text("#![allow(unused_imports, dead_code, unused_variables)]\nuse super::*;\n" + tests)
        if subprocess.run([RUSTC, "--test", "--edition", "2024", str(d / "c.rs"), "-o", str(d / "b")], capture_output=True).returncode:
            return False, {}
        try:
            r = subprocess.run([str(d / "b"), "oracle::", "--test-threads", "1"], capture_output=True, timeout=timeout, text=True)
            out = r.stdout
        except subprocess.TimeoutExpired as e:
            out = (e.stdout or b"").decode("utf-8", "replace") if isinstance(e.stdout, bytes) else (e.stdout or "")
        return True, {name: status == "ok" for name, status in RESULT.findall(out)}


def keep_api_correct(task_dir, src):
    """Tests that compile against the reference, and which of them pass."""
    reference = (task_dir / "reference.rs").read_text()
    compiled, results = run(reference, src)
    prelude, chunks = split_tests(src)
    if compiled:
        return src, results, len(chunks)
    kept = []
    for chunk in chunks:
        ok, _ = run(reference, prelude + chunk)
        if ok:
            kept.append(chunk)
    kept_src = prelude + "".join(kept)
    compiled, results = run(reference, kept_src) if kept else (False, {})
    return (kept_src if compiled else ""), results, len(chunks)



pool = json.loads(pool_path.read_text())
by_task = collections.defaultdict(list)
for c in pool:
    if c["arm"] == "B" and c["has_code"]:
        by_task[c["task"]].append(c)

def vectors(task):
    spec_file = spec_dir / f"{task}.spec_tests.rs"
    kept_src, ref, _ = keep_api_correct(suite / task, spec_file.read_text()) if spec_file.exists() else ("", {}, 0)
    out = []
    for c in by_task.get(task, []):
        code = re.search(r"```rust(.*?)```", pathlib.Path(c["code_path"]).read_text(), re.S).group(1).strip()
        compiled, res = run(code, kept_src) if kept_src else (False, {})
        out.append({"run": c["run"], "inv": c["inv"], "public_ok": c["public"]["ok"], "public_passed": c["public"]["passed"],
                    "public_compiled": c["public"]["compiled"], "hidden_ok": c["hidden"]["ok"],
                    "passed_tests": sorted(t for t in ref if compiled and res.get(t, False))})
    return task, out

if cache_path.exists():
    cache = json.loads(cache_path.read_text())
else:
    with concurrent.futures.ThreadPoolExecutor(4) as ex:
        cache = dict(ex.map(vectors, sorted(by_task)))
    cache_path.write_text(json.dumps(cache))

def codet_pick(cands):
    pool = [c for c in cands if c["public_ok"]] or cands
    groups = collections.defaultdict(list)
    for c in pool:
        groups[tuple(c["passed_tests"])].append(c)
    best = max(groups.values(), key=lambda g: (len(g) * len(g[0]["passed_tests"]), max(x["inv"] for x in g)))
    return max(best, key=lambda c: (c["public_passed"], c["public_compiled"], c["inv"]))

def public_pick(cands):
    return max(cands, key=lambda c: (c["public_passed"], c["public_compiled"], c["inv"]))

for label, limit in [("budget-faithful (B's first 4)", 4), ("insight (all 5 of B)", 5)]:
    n = codet = pub = 0
    for task, cands in cache.items():
        for run_id in sorted({c["run"] for c in cands}):
            cs = sorted([c for c in cands if c["run"] == run_id], key=lambda c: c["inv"])[:limit]
            if not cs:
                continue
            n += 1; codet += codet_pick(cs)["hidden_ok"]; pub += public_pick(cs)["hidden_ok"]
    print(f"{label}: {n} tuples; public pick {pub/n:.3f}; CodeT agreement pick {codet/n:.3f}   [bar >= 0.793; D = 0.743]")
