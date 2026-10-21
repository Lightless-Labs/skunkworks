"""Certify the gym v3 suite and export what the repo may hold (plan 2026-10-21, "Sealing").

certify: the suite is clean and settled -> records/certified.json binds its digest, every
  task's content hash and the frozen rubric's (SPEC.md) sha256. Clean: the latest sandboxed
  validation (rubric included) and similarity both on exactly this suite with nothing rejected,
  conflicting or pending; no excluded task in it. Settled (gym_export.settled): every task's
  latest audit round is on its current content and adjudicated, M4 included; a task void twice on
  its current content is kept after its M4 spec review and disclosed.
  Pilots run only on the certified suite (gym_pilot.py); a later repair changes the content and
  invalidates that family's pilot data (gym_band.py).

export (publication of the final gym): only with a final selection (gym_band.py finalize) whose
  floors are met (else status floors_unmet) and whose content hashes equal the current
  certification; writes runs/c2-gym3/export/ in the repo: for the retained dev families (kept +
  regression stratum), task.json (id, family, requirements, test_timeout_secs) and
  public_tests.rs; for every retained dev and surviving holdout task, its file hashes
  (hashes.json). Hidden tests and references stay under the gym root; dev-holdout requirements
  are not exported. Every exported text, hashes.json included, is checked BEFORE it is written to
  contain no hidden test name (literal substring).

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
from gymlib import (MAX_VOID_ROUNDS, OUT_REPO, RECORDS, REPO, SUITE, TASK_RE, DriverLock, emit, gate_or_exit,  # noqa: E402
                    gateway, latest_round, void_rounds,
                    hashes_digest, load_json, parse_task_json, repo_write_root, safe_write_text, save_json, spec_sha,
                    suite_digest,
                    tree_hashes)
from gym_rubric import leaked_names  # noqa: E402
from gymvendor import test_names  # noqa: E402

EXPORT = OUT_REPO / "runs" / "c2-gym3" / "export"
PENDING_SIM = ("reauthor", "reauthor-done", "reauthor-failed")
TASK_KEYS = ("id", "family", "requirements", "test_timeout_secs")


def content(tid):
    return hashes_digest(tree_hashes(SUITE / tid))


def settled(tid, audit, adj, cur):
    """'ok', 'void-twice' or a pending reason for one task, always about its CURRENT content:
    its latest audit round is on the current content and is adjudicated (M3 for failing tests and
    the M4 spec review), not excluded; or the current content has 2 void rounds and the latest
    got the M4 spec review without solver evidence ('void-twice', disclosed)."""
    a, j = audit.get(tid, {}), adj.get(tid, {})
    name, last = latest_round(a)
    if name is None or any(r.get("reserved") for r in a.values()):
        return "audit-pending"
    if last.get("content") != cur:
        return "stale-audit"                        # gym_audit.py schedules the next round
    if last["void"] and void_rounds(a, cur) < MAX_VOID_ROUNDS:
        return "audit-pending"
    rec = j.get(name)
    if not rec:
        return "adjudication-pending"
    if rec.get("excluded"):
        return "excluded"
    if rec.get("repaired"):
        return "stale-audit"                        # the repair changed the content: a new round is due
    return "void-twice" if last["void"] else "ok"


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
    pending = sorted(t for t, s in states.items() if s not in ("ok", "void-twice"))
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


def export():
    from gym_band import SELECTION
    cert = certified_current()
    sel = load_json(SELECTION)
    if not cert or not sel:
        emit(stage="export", status="not-certified" if not cert else "not-finalized")
        return False
    if sel.get("status") != "ok":
        emit(stage="export", status="floors_unmet")
        return False
    content = dict(sel["content"], **sel["holdout_content"])
    if any(cert["content"].get(t) != h for t, h in content.items()):
        emit(stage="export", status="content-mismatch")
        return False
    hidden_names = set()
    for tid in cert["content"]:
        hidden_names |= test_names((SUITE / tid / "hidden_tests.rs").read_text())
    files, hashes = {}, {}
    for tid in sorted(content):
        dev = tid in sel["content"]
        hashes[tid] = {"content": cert["content"][tid], "files": tree_hashes(SUITE / tid),
                       "set": "dev" if dev else "holdout"}
        if not dev:
            continue
        tj = parse_task_json((SUITE / tid / "task.json").read_bytes())
        files[f"{tid}/task.json"] = json.dumps({k: tj[k] for k in TASK_KEYS if k in tj}, indent=1, sort_keys=True) + "\n"
        files[f"{tid}/public_tests.rs"] = (SUITE / tid / "public_tests.rs").read_text()
    files["hashes.json"] = json.dumps({"suite": cert["suite"], "spec_sha256": cert["spec_sha256"], "tasks": hashes},
                                      indent=1, sort_keys=True) + "\n"
    if any(leaked_names(hidden_names, text) for text in files.values()):     # before anything is written
        raise RuntimeError("seal: hidden test name in an exported file")
    root = repo_write_root(EXPORT / "hashes.json")
    if EXPORT.exists():
        shutil.rmtree(EXPORT)
    for rel, text in files.items():
        (EXPORT / rel).parent.mkdir(parents=True, exist_ok=True)
        safe_write_text(EXPORT / rel, text, root)
    dev = len(sel["retained"])
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
