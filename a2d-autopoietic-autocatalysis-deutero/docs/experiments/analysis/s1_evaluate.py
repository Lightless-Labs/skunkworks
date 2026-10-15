"""S1 evaluation (development lane): validity of blind spec tests against the
reference, and their discrimination on the frozen S0 candidate pool.

usage: python3 -I s1_evaluate.py <suite-dir> <spec-dir> <pool.json> <out.json>
"""
import collections, concurrent.futures, json, pathlib, re, subprocess, sys, tempfile

RUSTC = str(pathlib.Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc")
suite, spec_dir, pool_path, out_path = (pathlib.Path(a) for a in sys.argv[1:5])
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


def eval_task(task):
    task_dir = suite / task
    spec_file = spec_dir / f"{task}.spec_tests.rs"
    if not spec_file.exists():
        return task, None
    kept_src, ref_results, total = keep_api_correct(task_dir, spec_file.read_text())
    record = {"total": total, "kept": len(ref_results), "valid": sum(ref_results.values()), "candidates": []}
    if not kept_src:
        return task, record
    for c in pool_by_task.get(task, []):
        if not c["has_code"]:
            continue
        code = re.search(r"```rust(.*?)```", pathlib.Path(c["code_path"]).read_text(), re.S).group(1).strip()
        compiled, res = run(code, kept_src)
        frac = (sum(res.values()) / len(ref_results)) if (compiled and ref_results) else 0.0
        valid_names = [n for n, ok in ref_results.items() if ok]
        vfrac = (sum(res.get(n, False) for n in valid_names) / len(valid_names)) if (compiled and valid_names) else 0.0
        record["candidates"].append({**{k: c[k] for k in ("run", "arm", "task", "inv")},
                                     "public_ok": c["public"]["ok"], "public_passed": c["public"]["passed"],
                                     "public_compiled": c["public"]["compiled"], "hidden_ok": c["hidden"]["ok"],
                                     "spec_compiled": compiled, "spec_frac": frac, "spec_valid_frac": vfrac})
    return task, record


pool = json.loads(pool_path.read_text())
pool_by_task = collections.defaultdict(list)
for c in pool:
    pool_by_task[c["task"]].append(c)
tasks = sorted(d.name for d in suite.iterdir() if (d / "task.json").exists())
with concurrent.futures.ThreadPoolExecutor(4) as ex:
    results = dict(ex.map(eval_task, tasks))
out_path.write_text(json.dumps(results))


def auc(pos, neg):
    if not pos or not neg:
        return float("nan")
    wins = sum((p > n) + 0.5 * (p == n) for p in pos for n in neg)
    return wins / (len(pos) * len(neg))


recs = [r for r in results.values() if r]
total = sum(r["total"] for r in recs)
kept = sum(r["kept"] for r in recs)
valid = sum(r["valid"] for r in recs)
cands = [c for r in recs for c in r["candidates"]]
green = [c for c in cands if c["public_ok"]]
pghf = [c for c in green if not c["hidden_ok"]]
pghp = [c for c in green if c["hidden_ok"]]
print(f"tasks with tests {len(recs)}/{len(tasks)}; tests generated {total}, API-correct (compile vs reference) {kept} ({kept/max(total,1):.2f})")
print(f"VALIDITY (pass on reference / API-correct) = {valid}/{kept} = {valid/max(kept,1):.3f}   [kill if < 0.85]")
print(f"AUC spec_frac -> hidden pass, among public-green ({len(pghp)} pass / {len(pghf)} fail) = {auc([c['spec_frac'] for c in pghp], [c['spec_frac'] for c in pghf]):.3f}   [kill if < 0.65]")
print(f"RECALL on public-green hidden-fail (>=1 spec test fails) = {sum(c['spec_frac'] < 1 for c in pghf)}/{len(pghf)} = {sum(c['spec_frac'] < 1 for c in pghf)/max(len(pghf),1):.3f}   [kill if < 0.45]")
print(f"false alarm on public-green hidden-pass = {sum(c['spec_frac'] < 1 for c in pghp)}/{len(pghp)} = {sum(c['spec_frac'] < 1 for c in pghp)/max(len(pghp),1):.3f}")
print(f"(valid-tests-only, not harness-realistic) AUC = {auc([c['spec_valid_frac'] for c in pghp], [c['spec_valid_frac'] for c in pghf]):.3f}")
by = collections.defaultdict(list)
for c in cands:
    by[(c["run"], c["task"])].append(c)
n = len(by)
def pick(cs, key):
    return max(cs, key=key) if cs else None
public_key = lambda c: (c["public_passed"], c["public_compiled"], c["inv"])
spec_key = lambda c: (c["public_passed"], c["spec_frac"], c["public_compiled"], c["inv"])
b_pub = sum(bool(p and p["hidden_ok"]) for p in (pick([c for c in v if c["arm"] == "B"], public_key) for v in by.values()))
b_spec = sum(bool(p and p["hidden_ok"]) for p in (pick([c for c in v if c["arm"] == "B"], spec_key) for v in by.values()))
d_pub = sum(bool(p and p["hidden_ok"]) for p in (pick([c for c in v if c["arm"] == "D"], public_key) for v in by.values()))
print(f"END (offline selection over B's 5, {n} tuples): public pick {b_pub/n:.3f}, (public, spec) pick {b_spec/n:.3f}; D pick {d_pub/n:.3f}")
