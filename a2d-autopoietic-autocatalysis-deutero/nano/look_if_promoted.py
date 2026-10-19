"""Look 2 of the confirmation-3 candidate rule, mechanically (plan 2026-10-18-confirmation-3
§0.1; LOG "C2-LOOK2").

usage: eval "$(python3 -I nano/look_if_promoted.py <batch id>...)"

Prints one shell command for the queue runner to eval:
- an `echo` if no batch keep was promoted (the candidate stays E002), if look 2 was
  already attempted (exactly one look: a failed look is never rerun), if it is past
  T = 2026-10-23 23:59 UTC, if the incumbent is not the promoted keep, or if the
  registration is missing;
- otherwise the look of the incumbent (the promoted keep) at the registration commit.
A cap deferral inside the look creates no holdout run, so a later line may still run it.
Reads results.tsv, the promoted run's candidate snapshot and the registration file only.
"""
import datetime
import hashlib
import json
import pathlib
import shlex
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
NANO = ROOT / "nano"
HOLDOUT_RUN = pathlib.Path.home() / ".a2d-holdout" / "runs" / "c2-look2"
REGISTERED_AT = ROOT / "runs" / "c2-look2-registration" / "registered_at.txt"
T = datetime.datetime(2026, 10, 23, 23, 59, tzinfo=datetime.timezone.utc)
SEED = 6262


def say(msg):
    # The queue runner only passes through lines carrying a "decision" key.
    return "printf '%s\\n' " + shlex.quote(json.dumps({"decision": "look 2: " + msg}))


def promoted(batch, results_text):
    lines = results_text.splitlines()
    rows = [dict(zip(lines[0].split("\t"), line.split("\t"))) for line in lines[1:] if line.strip()]
    return [r["id"] for r in rows if r["id"] in batch and r["stage"] == "confirm" and r["decision"] == "keep"]


def plan(batch, results_text, now, attempted, incumbent_is_promoted, registered_at):
    keeps = promoted(batch, results_text)
    if not keeps:
        return say("no promoted keep; the confirmation-3 candidate stays E002")
    if attempted:
        return say("already attempted (exactly one look)")
    if now > T:
        return say("past T (2026-10-23 23:59 UTC); the confirmation-3 candidate is E002")
    if not incumbent_is_promoted:
        return say(f"the incumbent is not the promoted keep {keeps[0]}; not run")
    if not registered_at:
        return say("registration missing; not run")
    return f"python3 -I nano/eval.py look --n 2 --registered-at {registered_at} --seed {SEED} --jobs 8"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main(batch):
    """Never fails silently: an `eval "$(...)"` of empty output would be a no-op."""
    try:
        text = (NANO / "results.tsv").read_text()
        keeps = promoted(batch, text)
        same = bool(keeps) and sha(ROOT / "runs" / f"c2-{keeps[0]}" / "candidate.py") == sha(NANO / "harness.py")
        reg = REGISTERED_AT.read_text().strip() if REGISTERED_AT.exists() else None
        return plan(batch, text, datetime.datetime.now(datetime.timezone.utc), HOLDOUT_RUN.exists(), same, reg)
    except Exception as error:  # noqa: BLE001
        return say(f"error: {type(error).__name__}: {error}")


if __name__ == "__main__":
    print(main(sys.argv[1:]))
