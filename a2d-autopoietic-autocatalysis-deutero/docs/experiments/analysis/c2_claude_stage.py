"""Stage copies-only cases for Claude-subagent analyses of the E002 incumbent
(cycle 2). No hidden test content is ever written to a case directory.

usage: python3 -I c2_claude_stage.py <stage-dir> <key-out.json>

judge/<case>/PROMPT.md : E002's exact judge prompt (spec + shuffled distinct
  public-green files) for every decisive tuple (>= 2 distinct greens that
  split on hidden tests). The answer key goes to <key-out.json>, outside the
  stage dir.
taxonomy/<case>/        : TASK.md (what the coder saw) and every distinct
  public-green file E002 produced in a run where its submission failed, all
  labelled "fails at least one hidden test" (train-label rule).
"""
import collections, hashlib, json, pathlib, random, re, sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
stage, key_out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
POOL = ROOT / "runs/2026-10-16-c2-e002-pool"
RUNS = {"c2-E002-screen": "runs/c2-E002/screen", "c2-E002b-rest": "runs/c2-E002b/rest",
        "c2-E002b-confirm": "runs/c2-E002b/confirm"}
D_PROMPTS = ROOT / "runs/c2-aa-200daaa763c4/run/invocations/D"
src = (ROOT / "nano/harness.py").read_text()
JUDGE = re.search(r'JUDGE = """(.*?)"""', src, re.S).group(1)


def task_prompt(task):
    return (D_PROMPTS / task / "r0/inv00-lift-coder.prompt.txt").read_text().split("PROMPT:\n", 1)[1]


key, n_tax = {}, 0
for name, run in RUNS.items():
    pool = json.loads((POOL / f"{name}.json").read_text())
    by = collections.defaultdict(list)
    for p in pool:
        if p["arm"] == "X-cand":
            by[p["task"]].append(p)
    sub = {}
    for line in open(ROOT / run / "ledger.jsonl"):
        r = json.loads(line)
        if r["kind"] == "score" and r["arm"] == "X:cand":
            sub[r["task"]] = r["result"]["all_passed"]
    for task, cs in sorted(by.items()):
        greens = {}
        for c in sorted(cs, key=lambda c: c["inv"])[:4]:
            if c.get("public", {}).get("ok"):
                greens.setdefault(pathlib.Path(c["code_path"]).read_text().split("```rust", 1)[-1].split("```", 1)[0].strip(),
                                  c["hidden"]["ok"])
        files = list(greens.items())
        case = hashlib.sha256(f"{name}/{task}".encode()).hexdigest()[:10]
        if len(files) >= 2 and any(ok for _, ok in files) and not all(ok for _, ok in files):
            order = list(range(len(files)))
            random.Random("c2-claude-judge-" + case).shuffle(order)
            letters = "ABCDEFGH"[:len(files)]
            body = "TASK:\n" + task_prompt(task) + "\n\nCANDIDATES:\n"
            for letter, i in zip(letters, order):
                body += f"\n### {letter}.rs\n```rust\n{files[i][0]}\n```\n"
            d = stage / "judge" / case
            d.mkdir(parents=True, exist_ok=True)
            (d / "PROMPT.md").write_text("SYSTEM:\n" + JUDGE + "\n\nUSER:\n" + body)
            key[case] = {"run": name, "task": task, "passing": [l for l, i in zip(letters, order) if files[i][1]]}
        if not sub[task] and files:
            d = stage / "taxonomy" / case
            d.mkdir(parents=True, exist_ok=True)
            (d / "TASK.md").write_text(task_prompt(task))
            for j, (code, ok) in enumerate(files, 1):
                assert not ok
                (d / f"F{j}.rs").write_text("// Passes every visible test; FAILS at least one hidden test.\n" + code + "\n")
            key.setdefault("_taxonomy", {})[case] = {"run": name, "task": task, "files": len(files)}
            n_tax += 1
key_out.parent.mkdir(parents=True, exist_ok=True)
key_out.write_text(json.dumps(key, indent=1, sort_keys=True) + "\n")
print("judge cases", len([k for k in key if not k.startswith("_")]), "taxonomy cases", n_tax)
