"""E007-only Luna runs on the gym v3 dev set (plan 2026-10-21, "Selection discipline", "Budget").

--wave W      the pilot of a dev wave: every dev family of that wave in the certified suite (or,
              with --repilot, only the families gym_band.py left pending for that wave).
--acceptance  the first fresh E007 replicate of the kept dev set (every wave's kept and regression
              families), once, and only when every dev wave of the split has a finalized band
              decision; it is the A/A threshold's first half and is never used for any selection.

Refuses (fail closed, content-free) unless: the launch gate holds (confirmation-3 isolation);
the split (runs/c2-gym3/split.json) is committed; the suite is certified and current
(gym_export.py certify); no family is a dev-holdout family (the holdout is never piloted); the
harness is E007 (sha256 pinned); and the daily Luna cap leaves room for the run's worst case.

The run is `a2d lift run`, arm X:inc (E007, its judge lane declared in its header), Luna low, k = 5,
on a copy of the certified tasks; the binary is the gym's own build (<gym root>/target). Its out
dir is runs/c2-gym3-pilot-w<W>/r<N> (or runs/c2-gym3-accept/r1) in the repo, so the ledger is at
runs/c2-*/*/ledger.jsonl, the depth the existing daily-cap globs count. It is registered in
runs/c2-gym3/replicates.json before launch (durable), with a manifest binding the content hash of
every task, the rubric hash and the harness hash. After the run, a seal check confirms that no
hidden test name appears anywhere in the out dir (no hidden test or reference reaches the repo).

usage: python3 -I tools/gym/gym_pilot.py --wave W [--repilot] [--binary <a2d>]
       python3 -I tools/gym/gym_pilot.py --acceptance [--binary <a2d>]      (--binary: tests only)
"""
import datetime
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import (BINARY, HOLDOUT_VIEW, OUT_REPO, RECORDS, REPO, RUNS, SUITE, TESTING, DriverLock,  # noqa: E402
                    _cfg, build_a2d, child_env, emit, gate_or_exit, gateway, launch_gate, load_json, prereqs_ok,
                    repo_write_root, safe_copy, save_json, spec_sha)
from gymvendor import strict_json, test_names  # noqa: E402

LUNA = "codex/gpt-6-luna"
DAILY_CAP = 3000
K = 5
CALLS_PER_TASK_WORST = 6          # --max-invocations per task (k = 5 samples plus a margin)
E007 = Path(_cfg("GYM_TEST_HARNESS", str(REPO / "nano" / "harness.py")))
E007_SHA = _cfg("GYM_TEST_E007_SHA", "c1e3e897d9557f974d01c244c0919deacc9c7bd5c164d56baab9695e2edbf639")
CAP_GLOBS = ("c2-*/*/ledger.jsonl", "c2-*/ledger.jsonl", "c2-*/*/*/ledger.jsonl")   # eval.py / confirm3.py
REGISTRY = OUT_REPO / "runs" / "c2-gym3" / "replicates.json"


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def lanes_of(path):
    lanes = {}
    for m in re.finditer(r"^# lanes:\s*(.+)$", Path(path).read_text(), re.M):
        for spec in m.group(1).split():
            name, value = spec.split("=", 1)
            lanes[name] = value
    return lanes


def out_dir(acceptance, wave, rep):
    rel = Path("runs") / ("c2-gym3-accept" if acceptance else f"c2-gym3-pilot-w{wave}") / f"r{rep}"
    return rel


def cap_visible(rel):
    """The ledger path matches one of the daily-cap globs over runs/."""
    ledger = Path(rel) / "ledger.jsonl"
    return ledger.parts[0] == "runs" and any(Path(*ledger.parts[1:]).match(g) and
                                             len(ledger.parts[1:]) == len(Path(g).parts) for g in CAP_GLOBS)


def luna_today():
    """Luna invocations since 00:00 UTC across the cycle's ledgers (as eval.py / confirm3.py count
    them, plus the gym's own); a damaged line that may be a Luna call counts (cap side)."""
    lo_ms = datetime.datetime.combine(datetime.datetime.now(datetime.timezone.utc).date(), datetime.time(),
                                      datetime.timezone.utc).timestamp() * 1000
    seen, total = set(), 0
    runs = OUT_REPO / "runs"
    ledgers = [p for g in CAP_GLOBS for p in runs.glob(g)] \
        + list(runs.glob("c2-pc/round*/*/attempt*/smoke/ledger.jsonl")) \
        + list((HOLDOUT_VIEW / "runs").glob("c2-*/ledger.jsonl")) + list((HOLDOUT_VIEW / "audit").glob("*/*/ledger.jsonl"))
    for led in ledgers:
        real = led.resolve()
        if real in seen or real.stat().st_mtime * 1000 < lo_ms:
            continue
        seen.add(real)
        with open(real) as f:
            for line in f:
                if '"invocation"' in line and LUNA in line:
                    try:
                        r = strict_json(line)
                    except ValueError:
                        total += 1
                        continue
                    if r.get("kind") == "invocation" and r.get("provider") == LUNA and r.get("ts_ms", 0) >= lo_ms:
                        total += 1
    return total


def seal_breaches(out, tasks):
    """Files in the out dir that contain any hidden test name of the run's tasks (count only)."""
    names = set()
    for t in tasks:
        names |= test_names((SUITE / t / "hidden_tests.rs").read_text())
    if not names:
        return 0
    pat = re.compile(r"(?<![A-Za-z0-9_])(?:" + "|".join(map(re.escape, sorted(names))) + r")(?![A-Za-z0-9_])")
    n = 0
    for p in sorted(Path(out).rglob("*")) if Path(out).exists() else []:
        if p.is_file() and not p.is_symlink() and pat.search(p.read_text(errors="replace")):
            n += 1
    return n


class AcceptanceNotReady(Exception):
    """A dev wave of the split has no finalized band decision yet: the write-once acceptance
    replicate (the A/A threshold's first half) would be spent on a partial kept set."""


def select_families(args, sp, act, cert):
    from gym_band import band_path, kept_dev
    from gym_split import assignment, families_for
    if "--acceptance" in args:
        waves = sorted({a["wave"] for a in sp["assign"].values() if a["set"] == "dev"})
        if not all((load_json(band_path(w)) or {}).get("finalized") for w in waves):
            raise AcceptanceNotReady()
        return kept_dev(waves), None
    wave = int(args[args.index("--wave") + 1])
    if "--repilot" in args:
        rec = load_json(band_path(wave), {})
        fams = sorted(f for f, d in rec.get("decisions", {}).items() if d in ("repilot", "invalidated"))
    else:
        if load_json(band_path(wave)):
            raise RuntimeError("wave already piloted (use --repilot)")
        fams = families_for(sp, act, "dev", wave)
    for f in fams:
        a = assignment(f, sp, act)
        if not a or a["set"] != "dev" or a["wave"] != wave:
            raise ValueError("a non-dev or off-wave family")
    in_cert = [f for f in fams if all(f"{f}-l{lv}" in cert["content"] for lv in (1, 2, 3, 4))]
    return in_cert, wave


@gateway("pilot")
def main():
    from gym_export import certified_current
    from gym_split import assignment, committed, load_split
    args = sys.argv[1:]
    binary = Path(args[args.index("--binary") + 1]) if "--binary" in args else BINARY
    if binary != BINARY and not TESTING:
        emit(stage="pilot", status="binary-override-refused")
        sys.exit(2)
    gate_or_exit("pilot")
    if not prereqs_ok():
        emit(stage="pilot", status="prereqs-not-met")
        sys.exit(2)
    with DriverLock() as lock:
        sp = load_split()
        if not sp or not committed():
            emit(stage="pilot", status="split-not-committed")
            sys.exit(2)
        cert = certified_current()
        if not cert:
            emit(stage="pilot", status="not-certified")
            sys.exit(2)
        act = load_json(RECORDS / "active.json")
        acceptance = "--acceptance" in args
        reg = load_json(REGISTRY, {})
        if acceptance and any(r.get("purpose") == "acceptance" for r in reg.values()):
            emit(stage="pilot", status="acceptance-exists")
            sys.exit(2)
        try:
            fams, wave = select_families(args, sp, act, cert)
        except AcceptanceNotReady:
            emit(stage="pilot", status="acceptance-not-ready")
            sys.exit(2)
        if any((assignment(f, sp, act) or {}).get("set") != "dev" for f in fams):
            emit(stage="pilot", status="holdout-refused")
            sys.exit(2)
        if not fams:
            emit(stage="pilot", families=0, status="nothing-due")
            return
        if not E007.is_file() or sha256(E007) != E007_SHA:
            emit(stage="pilot", status="harness-not-e007")
            sys.exit(2)
        tasks = sorted(f"{f}-l{lv}" for f in fams for lv in (1, 2, 3, 4))
        worst = CALLS_PER_TASK_WORST * len(tasks)
        if luna_today() + worst > DAILY_CAP:
            emit(stage="pilot", status="deferred-daily-cap", tasks=len(tasks))
            sys.exit(3)
        prior = [k for k in reg if k.startswith(str(out_dir(acceptance, wave, 0).parent) + "/")]
        rel = out_dir(acceptance, wave, len(prior) + 1)
        if not cap_visible(rel):
            raise RuntimeError("ledger depth outside the cap globs")
        out = OUT_REPO / rel
        repo_write_root(out / "ledger.jsonl")
        if out.exists():
            raise RuntimeError("run exists")
        sub = RUNS / f"pilot-{rel.parent.name}-{rel.name}-suite"
        if sub.exists():
            shutil.rmtree(sub)
        for t in tasks:
            for p in sorted((SUITE / t).rglob("*")):
                if p.is_file():
                    safe_copy(p, sub / t / p.relative_to(SUITE / t), SUITE)
        from gymlib import hashes_digest, tree_hashes
        content = {t: hashes_digest(tree_hashes(sub / t)) for t in tasks}
        if any(content[t] != cert["content"][t] for t in tasks):
            raise RuntimeError("pilot copy differs from the certified suite")
        snap = out.parent / "harness.py"
        if not snap.exists():
            repo_write_root(snap)
            out.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(E007, snap)
        if sha256(snap) != E007_SHA:
            raise RuntimeError("harness snapshot")
        seed = int(hashlib.sha256(f"gym-v3-pilot:{sp['seed']}:{rel}".encode()).hexdigest()[:8], 16)
        manifest = {"purpose": "acceptance" if acceptance else "pilot", "wave": wave, "families": fams,
                    "content": content, "suite": cert["suite"], "spec_sha256": spec_sha(), "harness_sha256": E007_SHA,
                    "split_sha256": sha256(OUT_REPO / "runs" / "c2-gym3" / "split.json"), "seed": seed, "k": K,
                    "model": LUNA, "max_invocations": worst,
                    "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
        save_json(OUT_REPO / f"{rel}.manifest.json", manifest)
        reg[str(rel)] = {"purpose": manifest["purpose"], "wave": wave, "threshold": acceptance,
                         "status": "reserved", "families": fams}
        save_json(REGISTRY, reg)                      # durable before any call
        argv = [str(binary), "lift", "run", "--experiment", f"C2-GYM3-{rel.parent.name.upper()}-{rel.name.upper()}",
                "--unregistered", "--suite", str(sub), "--model", LUNA, "--effort", "low", "--arms", "X:inc",
                "--k", str(K), "--seed", str(seed), "--jobs", "4", "--min-families", "1",
                "--max-invocations", str(worst), "--max-wall-clock-secs", "21600", "--out", str(out),
                "--external", f"inc={snap}"]
        for name, spec in lanes_of(snap).items():
            argv += ["--lane", f"{name}={spec}"]
        if not TESTING and not build_a2d():
            reg[str(rel)]["status"] = "build-failed"
            save_json(REGISTRY, reg)
            emit(stage="pilot", status="build-failed")
            sys.exit(1)
        if not launch_gate():
            reg[str(rel)]["status"] = "gate-closed"
            save_json(REGISTRY, reg)
            emit(stage="pilot", status="launch-gate-closed")
            sys.exit(2)
        with open(RECORDS / f"pilot-{rel.parent.name}-{rel.name}.log", "wb") as log:
            code = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                                  pass_fds=(lock.fd,), env=child_env()).returncode
        breaches = seal_breaches(out, tasks)
        reg = load_json(REGISTRY, {})
        reg[str(rel)].update({"status": "complete" if code == 0 and not breaches else "failed", "exit": code,
                              "seal_breaches": breaches})
        save_json(REGISTRY, reg)
        emit(stage="pilot", exit=code, families=len(fams), tasks=len(tasks), ok=code == 0 and not breaches,
             status="seal-breach" if breaches else ("acceptance" if acceptance else f"wave-{wave}"))
        if breaches or code:
            sys.exit(1)


if __name__ == "__main__":
    main()
