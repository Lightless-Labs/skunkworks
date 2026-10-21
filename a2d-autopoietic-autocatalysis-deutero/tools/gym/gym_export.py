"""Certify the gym v3 suite and export what the repo may hold (plan 2026-10-21, "Sealing").

certify: the suite is clean and settled -> records/certified.json binds its digest, every
  task's content hash and the frozen rubric's (SPEC.md) sha256. Clean: the latest sandboxed
  validation (rubric included) and similarity both on exactly this suite with nothing rejected,
  conflicting or pending; no excluded task in it. Settled: every task has an audit round on its
  current content; its first informative round is adjudicated (M4 included); a repaired task
  was re-audited and, if it failed again, adjudicated; a task void twice is kept and disclosed.
  Pilots run only on the certified suite (gym_pilot.py); a later repair changes the content and
  invalidates that family's pilot data (gym_band.py).

export: writes runs/c2-gym3/export/ in the repo: for dev tasks, task.json (id, family,
  requirements, test_timeout_secs) and public_tests.rs; for every task, its file hashes
  (hashes.json). Hidden tests and references stay under the gym root; dev-holdout requirements
  are not exported. Every exported text is checked to contain no hidden test name.

usage: python3 -I tools/gym/gym_export.py certify | export
"""
import json
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import (OUT_REPO, RECORDS, REPO, SUITE, TASK_RE, DriverLock, emit, gate_or_exit, gateway,  # noqa: E402
                    hashes_digest, load_json, repo_write_root, safe_write_text, save_json, spec_sha, suite_digest,
                    tree_hashes)
from gymvendor import test_names  # noqa: E402

EXPORT = OUT_REPO / "runs" / "c2-gym3" / "export"
PENDING_SIM = ("reauthor", "reauthor-done", "reauthor-failed")
TASK_KEYS = ("id", "family", "requirements", "test_timeout_secs")


def content(tid):
    return hashes_digest(tree_hashes(SUITE / tid))


def settled(tid, audit, adj, cur):
    """'ok', 'void-twice' or a pending reason for one task."""
    a, j = audit.get(tid, {}), adj.get(tid, {})
    r1, r2 = a.get("r1"), a.get("r2")
    if r1 is None or any(r.get("reserved") for r in (r1, r2) if r):
        return "audit-pending"
    if r1["void"]:
        if r2 is None:
            return "audit-pending"
        if r2["void"]:
            return "void-twice"
        if r2["content"] != cur:
            return "stale-audit"
        return "ok" if j.get("r2") else "adjudication-pending"
    if not j.get("r1"):
        return "adjudication-pending"
    if j["r1"].get("excluded"):
        return "excluded"
    if j["r1"].get("repaired"):
        if r2 is None:
            return "audit-pending"
        if r2["void"]:
            return "void-after-repair" if r2["content"] == cur else "stale-audit"
        if r2["content"] != cur:
            return "stale-audit"
        if not r2["passed"] and not j.get("r2"):
            return "adjudication-pending"
        return "excluded" if (j.get("r2") or {}).get("excluded") else "ok"
    return "ok" if r1["content"] == cur else "stale-audit"


def certify():
    suite = sorted(p.name for p in SUITE.iterdir() if TASK_RE.match(p.name))
    if not suite:
        raise ValueError("empty suite")
    digest = suite_digest()
    val = load_json(RECORDS / "validate.json", {})
    sim = load_json(RECORDS / "similarity-last.json", {})
    state = load_json(RECORDS / "similarity.json", {})
    excluded = set(load_json(RECORDS / "excluded.json", {"tasks": []})["tasks"])
    clean = (val.get("suite") == digest and val.get("rejected") == [] and val.get("sandboxed") is True
             and sim.get("suite") == digest and sim.get("conflicts") == 0
             and not any(v in PENDING_SIM for v in state.values()) and not excluded & set(suite))
    if not clean:
        emit(stage="certify", status="suite-not-clean")
        return False
    audit = load_json(RECORDS / "audit.json", {})
    adj = load_json(RECORDS / "adjudication.json", {})
    cur = {t: content(t) for t in suite}
    states = {t: settled(t, audit, adj, cur[t]) for t in suite}
    pending = sorted(t for t, s in states.items() if s not in ("ok", "void-twice", "void-after-repair"))
    if pending:
        emit(stage="certify", status="not-settled", pending=len(pending))
        return False
    head = subprocess.run(["git", "-C", str(REPO), "rev-parse", "HEAD"], capture_output=True, text=True,
                          stdin=subprocess.DEVNULL).stdout.strip()
    save_json(RECORDS / "certified.json", {
        "suite": digest, "content": cur, "spec_sha256": spec_sha(), "head": head,
        "voids": sorted(t for t, s in states.items() if s != "ok"),
        "certified_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})
    emit(stage="certify", ok=True, tasks=len(suite), voids=sum(s != "ok" for s in states.values()))
    return True


def certified_current():
    cert = load_json(RECORDS / "certified.json")
    return cert if cert and cert["suite"] == suite_digest() and cert["spec_sha256"] == spec_sha() else None


def _token(name):
    return re.compile(r"(?<![A-Za-z0-9_])" + re.escape(name) + r"(?![A-Za-z0-9_])")


def export():
    from gym_split import assignment, load_split
    cert = certified_current()
    sp = load_split()
    if not cert or not sp:
        emit(stage="export", status="not-certified" if not cert else "no-split")
        return False
    act = load_json(RECORDS / "active.json")
    hidden_names = set()
    for tid in cert["content"]:
        hidden_names |= test_names((SUITE / tid / "hidden_tests.rs").read_text())
    patterns = [_token(n) for n in sorted(hidden_names)]
    root = repo_write_root(EXPORT / "hashes.json")
    if EXPORT.exists():
        shutil.rmtree(EXPORT)
    hashes, dev = {}, 0
    for tid in sorted(cert["content"]):
        a = assignment(tid.rsplit("-l", 1)[0], sp, act)
        hashes[tid] = {"content": cert["content"][tid], "files": tree_hashes(SUITE / tid),
                       "set": a["set"] if a else None}
        if not a or a["set"] != "dev":
            continue
        tj = json.loads((SUITE / tid / "task.json").read_text())
        texts = {"task.json": json.dumps({k: tj[k] for k in TASK_KEYS if k in tj}, indent=1, sort_keys=True) + "\n",
                 "public_tests.rs": (SUITE / tid / "public_tests.rs").read_text()}
        for name, text in texts.items():
            if any(p.search(text) for p in patterns):
                raise RuntimeError("seal: hidden test name in an exported file")
            (EXPORT / tid).mkdir(parents=True, exist_ok=True)
            safe_write_text(EXPORT / tid / name, text, root)
        dev += 1
    save_json(EXPORT / "hashes.json", {"suite": cert["suite"], "spec_sha256": cert["spec_sha256"], "tasks": hashes})
    emit(stage="export", tasks=len(hashes), n=dev, ok=True)
    return True


@gateway("export")
def main():
    args = sys.argv[1:]
    gate_or_exit("export")
    with DriverLock():
        if args[:1] == ["certify"]:
            ok = certify()
        elif args[:1] == ["export"]:
            ok = export()
        else:
            raise ValueError("usage")
    if not ok:
        sys.exit(1)


if __name__ == "__main__":
    main()
