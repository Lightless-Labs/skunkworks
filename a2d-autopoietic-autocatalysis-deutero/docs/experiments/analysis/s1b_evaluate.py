"""S1b: consensus-filtered spec tests (pre-registered revision of S1).
Per tuple, among B's candidates, drop spec tests failed by more than half of
the public-green candidates; select by (public, retained-spec fraction,
compiled, latest). Development data only.

usage: python3 -I s1b_evaluate.py <suite-dir> <spec-dir> <pool.json>
"""
import collections, concurrent.futures, json, pathlib, re, subprocess, sys, tempfile

RUSTC = str(pathlib.Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc")
suite, spec_dir, pool_path = (pathlib.Path(a) for a in sys.argv[1:4])
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

def eval_task(task):
    spec_file = spec_dir / f"{task}.spec_tests.rs"
    if not spec_file.exists():
        return task, None
    kept_src, ref, _ = keep_api_correct(suite / task, spec_file.read_text())
    cands = []
    for c in by_task.get(task, []):
        code = re.search(r"```rust(.*?)```", pathlib.Path(c["code_path"]).read_text(), re.S).group(1).strip()
        compiled, res = run(code, kept_src) if kept_src else (False, {})
        cands.append({"run": c["run"], "inv": c["inv"], "public_ok": c["public"]["ok"], "public_passed": c["public"]["passed"],
                      "public_compiled": c["public"]["compiled"], "hidden_ok": c["hidden"]["ok"],
                      "per_test": {t: (compiled and res.get(t, False)) for t in ref}})
    return task, {"ref": ref, "cands": cands}

tasks = sorted(by_task)
with concurrent.futures.ThreadPoolExecutor(4) as ex:
    results = dict(ex.map(eval_task, tasks))

def auc(pos, neg):
    if not pos or not neg:
        return float("nan")
    return sum((p > n) + 0.5 * (p == n) for p in pos for n in neg) / (len(pos) * len(neg))

retained_total = retained_valid = 0
pos, neg, picks_pub, picks_spec, tuples = [], [], 0, 0, 0
recall_hits = recall_n = fa_hits = fa_n = 0
for task, r in results.items():
    if not r:
        continue
    for run_id in sorted({c["run"] for c in r["cands"]}):
        cs = [c for c in r["cands"] if c["run"] == run_id]
        green = [c for c in cs if c["public_ok"]]
        tests = list(r["ref"])
        if green:
            retained = [t for t in tests if sum(not c["per_test"][t] for c in green) <= len(green) / 2]
        else:
            retained = tests
        retained_total += len(retained); retained_valid += sum(r["ref"][t] for t in retained)
        frac = lambda c: (sum(c["per_test"][t] for t in retained) / len(retained)) if retained else 1.0
        for c in green:
            (pos if c["hidden_ok"] else neg).append(frac(c))
            if c["hidden_ok"]:
                fa_n += 1; fa_hits += frac(c) < 1
            else:
                recall_n += 1; recall_hits += frac(c) < 1
        tuples += 1
        pub = max(cs, key=lambda c: (c["public_passed"], c["public_compiled"], c["inv"]))
        spec = max(cs, key=lambda c: (c["public_passed"], frac(c), c["public_compiled"], c["inv"]))
        picks_pub += pub["hidden_ok"]; picks_spec += spec["hidden_ok"]
print(f"retained tests {retained_total}; RETAINED VALIDITY = {retained_valid}/{retained_total} = {retained_valid/max(retained_total,1):.3f}   [kill if < 0.85]")
print(f"AUC retained-spec fraction -> hidden pass among B public-green ({len(pos)} pass / {len(neg)} fail) = {auc(pos, neg):.3f}   [kill if < 0.65]")
print(f"recall {recall_hits}/{recall_n} = {recall_hits/max(recall_n,1):.3f}; false alarm {fa_hits}/{fa_n} = {fa_hits/max(fa_n,1):.3f}")
print(f"END over B's 5 ({tuples} tuples): public pick {picks_pub/tuples:.3f}; consensus-spec pick {picks_spec/tuples:.3f}   [promotion bar >= 0.793]")
