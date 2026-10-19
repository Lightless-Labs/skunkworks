"""Confirmation-3 registration review packet (plan 2026-10-18-confirmation-3 §0.6 step 6).

Builds a copies-only packet in an empty directory: the plan, the registration and frozen
candidate (once written), the runner and its tests, the registration generator, the
authoring pipeline README, the provider diff against main, and the LOG's confirmation-3
entries, plus REVIEW.md, the fixed brief. Reviewers read only the packet: Codex Sol from the
packet dir with web search disabled (`--run-sol`), and a fresh Claude subagent given the
packet path. Nothing under the holdout dir is copied.

usage: python3 -I docs/experiments/analysis/c3_review.py <packet dir> [--run-sol]
"""
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
FILES = ["docs/plans/2026-10-18-confirmation-3.md", "runs/c2-confirm3/registration.json", "runs/c2-confirm3/frozen.py",
         "nano/confirm3.py", "nano/test_confirm3.py", "nano/look_if_promoted.py",
         "docs/experiments/analysis/c3_registration.py", "docs/experiments/analysis/c3_power.py",
         "tools/holdout/README.md", "runs/c2-confirm2/registration.json"]
GLOBS = ["tools/holdout/*.py", "tools/holdout/*.sh", "tools/holdout/authoring/*", "tools/holdout/tests/*.py",
         "tools/holdout/tests/*.sh"]
BRIEF = """# Review: confirmation-3 pre-registration (A²D cycle 2)

You are one of two independent reviewers (the other is a fresh Claude). Read only the files in
this directory. Do not use the network.

**What is being registered:** a blind, spend-once confirmation of a frozen coding harness
(`runs/c2-confirm3/frozen.py`, arm X:frozen) against a plain repair loop (D) and best-of-k
sampling (B) on 144 newly authored tasks, r = 2 replicates on separate UTC days. The claim is
SUPPORTED only if X beats both D and B by >= +10 pp all-pass with CI95 low > 0 (plan §0, §0.13).

**Your job: find anything that could make the outcome invalid, biased toward SUPPORTED, or
unfalsifiable, before any confirmation-3 call.** In particular, check:
1. `nano/confirm3.py` implements the plan's §0.4, §0.10 and §0.13 rules exactly: tuple voids,
   worst/best-case imputation, IntegrityReview and Invalid thresholds, the spend-day, latency,
   drain and fingerprint gates, attempt-before-call, and relaunch-once.
2. `registration.json` (if present) matches the plan and the runner: arms, k, seeds, lanes,
   suite hash, fingerprint, decision rule, disclosures.
3. Any path by which hidden tests, holdout content or the sealed confirmation-2 decision could
   reach the harness, a proposer or the operator before the decision.
4. Anything in the generator (`c3_registration.py`) that a human could steer.
5. Open question from §0.13: should the CI also be computed under imputation?

The review may only **tighten** §0.4 (plan §0.10). Anything else in §0 is fixed.

## Output
- **BLOCKERS:** numbered, each with the file and line, the failure scenario, and the minimal fix.
- **NON-BLOCKING:** numbered, short.
- **Answer to the open question.**
- Last line exactly: `VERDICT: Yes` (registration may be committed once the blockers are fixed)
  or `VERDICT: No` (with why).
"""


def build(packet):
    packet.mkdir(parents=True, exist_ok=True)
    if any(packet.iterdir()):
        sys.exit(f"{packet} is not empty")
    for rel in FILES:
        src = ROOT / rel
        if src.exists():
            dst = packet / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(src, dst)
    for pattern in GLOBS:                      # the pipeline's implementation, not only its README
        for src in sorted(ROOT.glob(pattern)):
            if src.is_file():
                dst = packet / src.relative_to(ROOT)
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(src, dst)
    log = (ROOT / "docs/experiments/LOG.md").read_text()
    start = log.find("### C2-MCP")
    (packet / "LOG-confirmation-3.md").write_text(log[start:] if start >= 0 else log[-60000:])
    diff = subprocess.run(["git", "-C", str(ROOT), "diff", "main...c2-web-provider", "--", "crates"],
                          capture_output=True, text=True).stdout
    (packet / "provider-branch.diff").write_text(diff)
    (packet / "REVIEW.md").write_text(BRIEF)
    return packet


def run_sol(packet):
    prompt = "Follow REVIEW.md in the current directory. Read only files in this directory."
    with open(packet / "sol-review.jsonl", "w") as out, open(packet / "sol-review.err", "w") as err:
        code = subprocess.run(["codex", "exec", "-m", "gpt-6.1-sol", "-c", 'model_reasoning_effort="high"',
                               "-c", 'web_search="disabled"', "--sandbox", "read-only", "--skip-git-repo-check",
                               "--json", "--output-last-message", str(packet / "SOL-REVIEW.md"), prompt],
                              cwd=packet, stdin=subprocess.DEVNULL, stdout=out, stderr=err).returncode
    searches = sum('"web_search"' in line for line in open(packet / "sol-review.jsonl"))
    print(f"sol review exit {code}, web_search items {searches}, verdict file {packet / 'SOL-REVIEW.md'}")


if __name__ == "__main__":
    p = build(pathlib.Path(sys.argv[1]).resolve())
    print(f"packet: {p} ({sum(1 for _ in p.rglob('*') if _.is_file())} files)")
    if "--run-sol" in sys.argv:
        run_sol(p)
