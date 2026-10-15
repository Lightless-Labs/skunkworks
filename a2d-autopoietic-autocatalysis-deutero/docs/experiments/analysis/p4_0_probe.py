"""P4-0: label-only bug diagnosis by Luna-low on the T0 cases.

usage: python3 -I p4_0_probe.py <t0-dir> <t0-results.json> <out.json>
"""
import concurrent.futures, json, math, pathlib, re, subprocess, sys, tempfile

t0_dir, t0_results, out_path = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
PROMPT = """This Rust file passes every visible test, but it FAILS at least one hidden acceptance test that checks the requirements below.
Find the bug. Reply with ONE JSON object and nothing else:
{{"suspect_clauses": [up to 3 clause ids such as "R7", most likely first], "bug": "one sentence", "lesson": "a TASK-AGNOSTIC instruction to a programmer (no names from this task, at most 25 words) that would have prevented this bug"}}

REQUIREMENTS:
{spec}

FILE:
```rust
{code}
```"""


def ask(case):
    spec = (t0_dir / case / "SPEC.md").read_text()
    code = (t0_dir / case / "candidate.rs").read_text()
    with tempfile.TemporaryDirectory() as cwd:
        last = pathlib.Path(cwd) / "last.txt"
        subprocess.run(["codex", "exec", PROMPT.format(spec=spec, code=code), "-m", "gpt-6-luna",
                        "-c", 'model_reasoning_effort="low"', "-c", 'approval_policy="never"',
                        "--sandbox", "read-only", "--skip-git-repo-check", "--ephemeral",
                        "--ignore-user-config", "--ignore-rules", "-o", str(last)],
                       cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True, timeout=600)
        text = last.read_text() if last.exists() else ""
    m = re.search(r"\{.*\}", text, re.S)
    try:
        return case, json.loads(m.group(0))
    except Exception:
        return case, {"error": text[:300]}


def clause_count(spec):
    return len(set(re.findall(r"^R(\d+)\.", spec, re.M)))


truth = {r["case"]: r for r in json.loads(t0_results.read_text())}
cases = sorted(truth)
with concurrent.futures.ThreadPoolExecutor(4) as ex:
    answers = dict(ex.map(ask, cases))
hits = chance = 0.0
rows = []
for case in cases:
    a, t = answers[case], truth[case]
    violated = set(t["violated_clauses"])
    suspects = [s.strip() for s in a.get("suspect_clauses", [])][:3]
    hit = bool(violated & set(suspects))
    n = clause_count((t0_dir / case / "SPEC.md").read_text())
    v, k = len(violated), min(3, n)
    p_chance = 1 - (math.comb(n - v, k) / math.comb(n, k) if n - v >= k else 0)
    hits += hit
    chance += p_chance
    rows.append({"case": case, "suspects": suspects, "violated": sorted(violated), "hit": hit,
                 "chance": round(p_chance, 3), "bug": a.get("bug"), "lesson": a.get("lesson"), "error": a.get("error")})
out_path.write_text(json.dumps(rows, indent=1))
n = len(cases)
print(f"clause hit rate {hits/n:.3f} ({int(hits)}/{n}); chance {chance/n:.3f}; errors {sum(1 for r in rows if r['error'])}")
