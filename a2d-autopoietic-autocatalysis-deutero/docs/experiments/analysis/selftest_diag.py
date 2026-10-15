"""Blind diagnostic: for hidden compile failures, compile the submission
WITHOUT hidden tests (plain lib, and --test with only its own tests).
Uses no hidden tests; prints counts only."""
import json, pathlib, subprocess, sys, tempfile, collections
RUSTC = str(pathlib.Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc")
run_dir = pathlib.Path(sys.argv[1]); repo_ledger = pathlib.Path(sys.argv[2])
counts = collections.Counter()
for line in repo_ledger.read_text().splitlines():
    r = json.loads(line)
    if r["kind"] != "score" or r["result"]["compiled"] or r["result"]["all_passed"]:
        continue
    sub = run_dir / "submissions" / r["arm"] / r["task"] / f"r{r['replicate']}.rs"
    if not sub.exists():
        counts[(r["arm"], "no_code")] += 1; continue
    with tempfile.TemporaryDirectory() as d:
        d = pathlib.Path(d); src = d / "c.rs"; src.write_bytes(sub.read_bytes())
        plain = subprocess.run([RUSTC, "--edition", "2024", "--crate-type", "lib", str(src), "-o", str(d / "l.rlib")], capture_output=True).returncode == 0
        test = subprocess.run([RUSTC, "--edition", "2024", "--test", str(src), "-o", str(d / "t")], capture_output=True).returncode == 0
    kind = "solution_does_not_compile" if not plain else ("own_tests_break_test_build" if not test else "compiles_alone_fails_with_hidden_tests (API mismatch)")
    counts[(r["arm"], kind)] += 1
for k in sorted(counts): print(k, counts[k])
