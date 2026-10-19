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


MAX_CONFIRMATIONS = 2  # pre-registered (LOG, 10-17): at most 2 before phase B


def say(msg):
    # The queue runner only passes through lines carrying a "decision" key.
    return "printf '%s\\n' " + shlex.quote(json.dumps({"decision": "confirm-best: " + msg}))


def plan(batch, results_text, incumbent_sha, runs):
    rows = [dict(zip(results_text.splitlines()[0].split("\t"), line.split("\t")))
            for line in results_text.splitlines()[1:] if line.strip()]
    for exp in batch:
        if any(r["id"] == exp and r["stage"] == "confirm" and r["decision"] == "keep" for r in rows):
            return say(f"{exp} was already promoted")
    pending, provisional = [], []
    for exp in batch:
        manifest = runs / f"c2-{exp}" / "manifest.json"
        mine = [r for r in rows if r["id"] == exp]
        if not manifest.exists() or not mine:
            pending.append(exp)
            continue
        if json.loads(manifest.read_text())["incumbent_sha"] != incumbent_sha:
            return say(f"{exp} was registered against another incumbent")
        # Final without a keep: a discard at any stage, a used confirmation, or an
        # integrity review (eval.py stops an experiment on an uncounted tool call).
        if any(r["stage"] == "confirm" or r["decision"].startswith(("discard", "integrity review")) for r in mine):
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
    used = len({r["id"] for r in rows if r["id"] in batch and r["stage"] == "confirm"})
    if used >= MAX_CONFIRMATIONS:
        return say(f"confirmation limit reached ({used} of {MAX_CONFIRMATIONS}); {exp} waits until after phase B")
    d = runs / f"c2-{exp}"
    hyp = (d / "hypothesis.txt").read_text().strip()
    return " ".join(["python3 -I nano/eval.py run --id", shlex.quote(exp),
                     "--candidate", shlex.quote(f"runs/{d.name}/candidate.py"),
                     "--proposer", shlex.quote(proposer), "--jobs 6 --hypothesis", shlex.quote(hyp)])


def main(batch, results_text=None):
    """Never fails silently: an `eval "$(...)"` of empty output would be a no-op."""
    try:
        text = results_text if results_text is not None else (NANO / "results.tsv").read_text()
        return plan(batch, text, sha(NANO / "harness.py"), ROOT / "runs")
    except Exception as error:  # noqa: BLE001
        return say(f"error: {type(error).__name__}: {error}")


if __name__ == "__main__":
    print(main(sys.argv[1:]))
