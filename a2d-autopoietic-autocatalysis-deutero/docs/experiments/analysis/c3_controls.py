"""Run the live tool-detector controls on the given lanes and record them (plan
2026-10-18-confirmation-3 §0.4 step 3; Sol re-check B7). Counts and item types only.

usage: python3 -I docs/experiments/analysis/c3_controls.py --lanes codex/gpt-6-luna:low,codex/gpt-6.1-sol:medium
           [--checkout <provider checkout>] [--out runs/c2-confirm3/controls.json]

Runs, with A2D_CONTROL_LANES set, the provider's ignored live tests:
- negative control: the provider as shipped, a PONG call per lane, 0 tool events;
- positive controls: per lane, shell, web search, file change, MCP (a stdio echo server),
  sub-agent and error each succeed and show their own item type, counted by the detector.
The registration generator requires this record for the coder lane and every registered lane,
at the registered provider commit and codex fingerprint.
"""
import argparse
import datetime
import json
import os
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "nano"))
import confirm3 as c3  # noqa: E402

KINDS = ("shell", "web", "file", "mcp", "collab", "error")
LINE = re.compile(r"^positive control (\S+):(\S+) (\w+): (\w+) present (true|false), (\d+) tool events(?:, (\d+) attempts)?$")


def cargo(checkout, test, lanes):
    r = subprocess.run(["cargo", "test", "-p", "a2d-providers", test, "--", "--ignored", "--nocapture", "--test-threads=1"],
                       cwd=checkout, capture_output=True, text=True, env={**os.environ, "A2D_CONTROL_LANES": lanes})
    return r.returncode, r.stdout + r.stderr


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--lanes", required=True)
    ap.add_argument("--checkout", type=pathlib.Path, default=ROOT)
    ap.add_argument("--out", type=pathlib.Path, default=c3.DIR / "controls.json")
    args = ap.parse_args()
    lanes = [s.strip() for s in args.lanes.split(",") if s.strip()]
    neg_code, _ = cargo(args.checkout, "live_codex_call_returns_text_with_zero_tool_events", ",".join(lanes))
    pos_code, text = cargo(args.checkout, "live_codex_positive_controls_count_each_tool_kind", ",".join(lanes))
    positive = {}
    for line in text.splitlines():
        m = LINE.match(line.strip())
        if m:
            model, effort, kind, item, present, n, attempts = m.groups()
            positive.setdefault(f"codex/{model}:{effort}", {})[kind] = {"item": item, "present": present == "true",
                                                                         "events": int(n),
                                                                         "attempts": int(attempts or 1)}
    commit = subprocess.run(["git", "-C", str(args.checkout), "rev-parse", "HEAD"], capture_output=True,
                            text=True).stdout.strip()
    record = {"utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "provider_checkout_head": commit,
              "codex": c3.codex_fingerprint(), "lanes": lanes, "negative_ok": neg_code == 0,
              "positive_ok": pos_code == 0, "positive": positive}
    complete = record["negative_ok"] and record["positive_ok"] and all(
        all(positive.get(lane, {}).get(k, {}).get("present") and positive[lane][k]["events"] >= 1 for k in KINDS)
        for lane in lanes)
    record["complete"] = complete
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(record, indent=1) + "\n")
    print(json.dumps({"written": str(args.out), "complete": complete, "negative_ok": record["negative_ok"],
                      "lanes": len(lanes), "kinds": sum(len(v) for v in positive.values())}))


if __name__ == "__main__":
    main()
