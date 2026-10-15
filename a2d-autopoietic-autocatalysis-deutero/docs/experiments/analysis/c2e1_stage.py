"""C2-E1 staging: the decisive tuples of the S0 pool (>= 2 public-green
candidates among B's first 4 that differ in hidden outcome), one blind dir
per tuple: SPEC.md (exactly the coder's task prompt) and candidates A.rs..
in a seeded shuffle. The answer key is written separately.

usage: python3 -I c2e1_stage.py <pool.json> <prompt-run-dir> <out-dir> <key.json>
"""
import collections, json, pathlib, random, re, sys

pool_path, prompt_run, out, key_path = (pathlib.Path(a) for a in sys.argv[1:5])
pool = json.loads(pool_path.read_text())
by = collections.defaultdict(list)
for c in pool:
    if c["arm"] == "B":
        by[(c["run"], c["task"])].append(c)
rng = random.Random(20261015)
key = {}
for (run, task), cands in sorted(by.items()):
    cands = [c for c in sorted(cands, key=lambda c: c["inv"])[:4] if c["has_code"]]
    green = [c for c in cands if c["public"]["ok"]]
    if len(green) < 2 or len({c["hidden"]["ok"] for c in green}) < 2:
        continue
    prompt = (prompt_run / "invocations/D" / task / "r0/inv00-lift-coder.prompt.txt").read_text()
    spec = prompt.split("\n\nPROMPT:\n", 1)[1]
    rng.shuffle(green)
    tid = f"{task}--{run}"
    d = out / tid
    d.mkdir(parents=True, exist_ok=True)
    (d / "SPEC.md").write_text(spec)
    key[tid] = {}
    for letter, c in zip("ABCDE", green):
        code = re.search(r"```rust(.*?)```", pathlib.Path(c["code_path"]).read_text(), re.S).group(1).strip()
        (d / f"{letter}.rs").write_text(code + "\n")
        key[tid][letter] = c["hidden"]["ok"]
key_path.write_text(json.dumps(key, indent=1))
print(len(key), "tuples staged")
