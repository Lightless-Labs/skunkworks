"""C2-E1 scoring: judge picks on the decisive tuples -> pick rate on all 136.

usage: python3 -I c2e1_score.py <key.json> <picks.json | judge-dir:sol> [fixed_passes=76] [tuples=136]
"""
import json, pathlib, re, sys

key = json.loads(pathlib.Path(sys.argv[1]).read_text())
src = sys.argv[2]
fixed = int(sys.argv[3]) if len(sys.argv) > 3 else 76
total = int(sys.argv[4]) if len(sys.argv) > 4 else 136
if src.endswith(":sol"):
    picks = {}
    for d in pathlib.Path(src[:-4]).iterdir():
        f = d / "sol.json"
        if f.exists():
            m = re.search(r"\{.*\}", f.read_text(), re.S)
            try:
                picks[d.name] = json.loads(m.group(0))["pick"].strip().upper()[:1]
            except Exception:
                picks[d.name] = None
else:
    raw = json.loads(pathlib.Path(src).read_text())
    picks = {k: (v["pick"] if isinstance(v, dict) else v) for k, v in raw.items()}
correct = sum(bool(key[t].get(p)) for t, p in picks.items() if p)
missing = [t for t in key if not picks.get(t)]
rnd = sum(sum(v.values()) / len(v) for v in key.values())
print(f"judged {len(picks)}/{len(key)}; missing {missing}")
print(f"correct {correct}/{len(key)} ({correct/len(key):.2f}); random expectation {rnd:.1f}")
print(f"pick rate = ({fixed} + {correct}) / {total} = {(fixed + correct) / total:.3f}  [rule: >= 0.793; D 0.743]")
