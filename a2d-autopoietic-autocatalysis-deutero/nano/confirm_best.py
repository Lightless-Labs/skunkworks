"""The promotion sequencing rule, mechanically (LOG, "Notes before the 10-17 queue").

usage: python3 -I nano/confirm_best.py <id>...   (the batch registered against the current incumbent)

Prints one shell command for the queue runner to eval:
- an `echo` if a batch experiment has no decision yet, if one was already
  promoted, or if no provisional keep is left;
- otherwise the confirmation run of the provisional keep with the largest
  full-stage delta all-pass (ties: smaller SE, then lower id). A discarded
  confirmation makes the next call pick the next one.
Reads results.tsv, the run manifests and hypothesis snapshots only.
"""
import hashlib
import json
import pathlib
import shlex
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
NANO = ROOT / "nano"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def say(msg):
    # The queue runner only passes through lines carrying a "decision" key.
    return "echo " + shlex.quote(json.dumps({"decision": "confirm-best: " + msg}))


def plan(batch, results_text, incumbent_sha, runs):
    rows = [dict(zip(results_text.splitlines()[0].split("\t"), line.split("\t")))
            for line in results_text.splitlines()[1:] if line.strip()]
    pending, provisional = [], []
    for exp in batch:
        manifest = runs / f"c2-{exp}" / "manifest.json"
        mine = [r for r in rows if r["id"] == exp]
        if any(r["stage"] == "confirm" and r["decision"] == "keep" for r in mine):
            return say(f"{exp} was already promoted")
        if not manifest.exists() or not mine:
            pending.append(exp)
            continue
        if json.loads(manifest.read_text())["incumbent_sha"] != incumbent_sha:
            return say(f"{exp} was registered against another incumbent")
        if any(r["stage"] == "confirm" or r["decision"].startswith("discard") for r in mine):
            continue
        full = [r for r in mine if r["stage"] == "full" and r["decision"] == "provisional keep"]
        if full:
            provisional.append((-float(full[-1]["delta_allpass"]), float(full[-1]["se_allpass"]), exp,
                                full[-1]["proposer"]))
        else:
            pending.append(exp)
    if pending:
        return say("waiting for " + " ".join(pending))
    if not provisional:
        return say("no provisional keep left")
    _, _, exp, proposer = min(provisional)
    d = runs / f"c2-{exp}"
    hyp = (d / "hypothesis.txt").read_text().strip()
    return " ".join(["python3 -I nano/eval.py run --id", shlex.quote(exp),
                     "--candidate", shlex.quote(f"runs/{d.name}/candidate.py"),
                     "--proposer", shlex.quote(proposer), "--jobs 6 --hypothesis", shlex.quote(hyp)])


if __name__ == "__main__":
    print(plan(sys.argv[1:], (NANO / "results.tsv").read_text(), sha(NANO / "harness.py"), ROOT / "runs"))
