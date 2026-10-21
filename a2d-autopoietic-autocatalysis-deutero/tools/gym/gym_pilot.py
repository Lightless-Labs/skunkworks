"""E007-only Luna runs on the gym v3 dev set (plan 2026-10-21, "Selection discipline", "Budget").

--wave W            the wave's ONE initial pilot: every family holding a slot of the wave's frozen
                    membership (runs/c2-gym3/waves.json; gym_split.py freeze), all of which must be
                    certified (a partial wave is refused, never silently narrowed). Refused once a
                    spent pilot of the wave exists.
--wave W --repilot  only the wave's families pending as void ("repilot") or "invalidated" (their
                    content changed since their pilot data) with no re-pilot spent yet (one each).
--acceptance        the first fresh E007 replicate of the final selection (gym_band.py finalize:
                    kept + regression stratum, floors met), once; refused with floors_unmet, or
                    when the selection's content hashes differ from the current certification.

Refuses (fail closed, content-free) unless: the launch gate holds (confirmation-3 isolation); the
split is committed; the suite is certified and current; no family is a dev-holdout family; the
harness is E007 (sha256 pinned); the daily Luna cap leaves room for the run's worst case (today's
Luna calls across the cap globs + 6 x tasks <= 3000, as nano/eval.py checks); the pinned codex
passes (version + sha256) and is the one the a2d child resolves on its PATH (fingerprint), and the
temp-root ancestry is clean (isolation_ok). The build and a second gate check come BEFORE the
registry reservation, so a build failure or a closed gate never spends a run; the entry goes
reserved -> launched (right before a2d starts) -> complete | failed. The fingerprint and the
temp-root check are repeated after the run; a change fails the run.

Sealing: a2d's --out dir, the suite copy, the harness snapshot and the manifest stay under the gym
root (RUNS/pilot-<name>, RECORDS/pilots/). The repo receives only, at runs/c2-gym3-pilot-w<W>/r<N>/
(or runs/c2-gym3-accept/r1/), the depth the daily-cap globs (c2-*/*/ledger.jsonl) count:
  - ledger.jsonl: an ACCOUNTING ledger, written incrementally while a2d runs (a tail thread reads
    the sealed ledger's complete lines with gymvendor.lines + strict JSON every second and appends
    each `invocation` record reduced to an allowlist: kind, ts_ms, provider, arm, task, replicate,
    index, ok, duration_ms, tool_calls, token counts, seq), so in-flight Luna calls are visible to
    nano/eval.py's and confirm3.py's luna_today; a damaged invocation line is published as a
    damaged Luna-side record (cap side); every record is scanned for hidden test names (literal
    substring) BEFORE it is written; an interrupted run is swept again by the next driver;
  - summary.json: content-free (manifest hashes and ids, counts, statuses), scanned before writing.
A hidden test name anywhere in the sealed out dir (a submission or prompt) fails the run.

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
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymlib import (BINARY, HOLDOUT_VIEW, OUT_REPO, RECORDS, REPO, RUNS, SUITE, TASK_RE, TESTING,  # noqa: E402
                    DriverLock, _cfg, _check_chain, binary_allowed, build_a2d, child_env, codex_fingerprint,
                    codex_pinned, emit, gate_or_exit, gateway, hashes_digest, launch_gate, load_json, prereqs_ok,
                    repo_write_root, safe_copy, save_json, spec_sha, tree_hashes)
from gym_rubric import leaked_names  # noqa: E402
from gymvendor import isolation_ok, lines, strict_json, test_names  # noqa: E402

LUNA = "codex/gpt-6-luna"
DAILY_CAP = 3000
K = 5
CALLS_PER_TASK_WORST = 6          # --max-invocations per task (k = 5 samples plus a margin)
E007 = Path(_cfg("GYM_TEST_HARNESS", str(REPO / "nano" / "harness.py")))
E007_SHA = _cfg("GYM_TEST_E007_SHA", "c1e3e897d9557f974d01c244c0919deacc9c7bd5c164d56baab9695e2edbf639")
CAP_GLOBS = ("c2-*/*/ledger.jsonl", "c2-*/ledger.jsonl", "c2-*/*/*/ledger.jsonl")   # eval.py / confirm3.py
REGISTRY = OUT_REPO / "runs" / "c2-gym3" / "replicates.json"
SPENT = ("launched", "complete", "failed")
PROVIDER_RE = re.compile(r"[a-z0-9_-]{1,32}/[A-Za-z0-9._:-]{1,64}", re.ASCII)
ARM_RE = re.compile(r"[A-Za-z0-9:_-]{1,32}", re.ASCII)
INT_FIELDS = ("seq", "replicate", "index", "duration_ms", "tool_calls")
USAGE_FIELDS = ("prompt_tokens", "completion_tokens", "thinking_tokens", "reported_total")


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
    return Path("runs") / ("c2-gym3-accept" if acceptance else f"c2-gym3-pilot-w{wave}") / f"r{rep}"


def cap_visible(rel):
    """The ledger path matches one of the daily-cap globs over runs/."""
    ledger = Path(rel) / "ledger.jsonl"
    return ledger.parts[0] == "runs" and any(Path(*ledger.parts[1:]).match(g) and
                                             len(ledger.parts[1:]) == len(Path(g).parts) for g in CAP_GLOBS)


def luna_today():
    """Luna invocations since 00:00 UTC across the cycle's ledgers (as eval.py / confirm3.py count
    them, the gym's accounting ledgers included); a damaged line that may be a Luna call counts."""
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


# ------------------------------------------------------------------ accounting ledger

def _int(v):
    return v if isinstance(v, int) and not isinstance(v, bool) and v >= 0 else None


def reduce_invocation(rec):
    """An invocation record reduced to content-free, type-checked accounting fields (the shape
    luna_today counts: `"invocation"`, the provider and ts_ms kept verbatim)."""
    prov = rec.get("provider")
    if not (isinstance(prov, str) and PROVIDER_RE.fullmatch(prov)):
        prov = LUNA if isinstance(prov, str) and LUNA in prov else "unknown"
    out = {"kind": "invocation", "provider": prov,
           "ts_ms": _int(rec.get("ts_ms")) or int(time.time() * 1000), "ok": rec.get("ok") is True}
    for k in INT_FIELDS:
        out[k] = _int(rec.get(k))
    task, arm = rec.get("task"), rec.get("arm")
    out["task"] = task if isinstance(task, str) and TASK_RE.match(task) else None
    out["arm"] = arm if isinstance(arm, str) and ARM_RE.fullmatch(arm) else None
    usage = rec.get("usage")
    out["usage"] = {k: _int(usage.get(k)) for k in USAGE_FIELDS} if isinstance(usage, dict) else None
    return out


def _append(path, text, root):
    """Append-only, durable write under the repo's runs/c2-gym3* (never through a symlink)."""
    _check_chain(path, root)
    fd = os.open(path, os.O_WRONLY | os.O_APPEND | os.O_CREAT | os.O_NOFOLLOW, 0o644)
    try:
        os.write(fd, text.encode("utf-8"))
        os.fsync(fd)
    finally:
        os.close(fd)


class Accounting:
    """Publishes the sealed ledger's invocation records, reduced, to the repo's accounting ledger
    while the run goes on (and once more at the end). Resumable: records carry the sealed seq and
    a sweep skips what the published ledger already holds."""

    def __init__(self, sealed_ledger, publish_ledger, names):
        self.src, self.dst, self.names = Path(sealed_ledger), Path(publish_ledger), names
        self.root = repo_write_root(self.dst)
        self.offset, self.published, self.breaches, self.seen = 0, 0, 0, 0
        self.done = self._already()
        self.lock = threading.Lock()

    def _already(self):
        try:
            text = self.dst.read_text(errors="replace")
        except FileNotFoundError:
            return set()
        out = set()
        for line in lines(text):
            try:
                r = strict_json(line)
            except ValueError:
                continue
            out.add(r.get("src") if isinstance(r, dict) else None)
        return out

    def sweep(self, final=False):
        with self.lock:
            try:
                with open(self.src, "rb") as f:
                    f.seek(self.offset)
                    data = f.read()
            except FileNotFoundError:
                return
            if not final:
                data = data[:data.rfind(b"\n") + 1]           # hold back a partial last line
            self.offset += len(data)
            start = self.offset - len(data)
            pos = 0
            for raw in data.split(b"\n"):
                at, pos = start + pos, pos + len(raw) + 1
                line = raw.decode("utf-8", errors="replace")
                if not line.strip():
                    continue
                try:
                    rec = strict_json(line)
                    if not isinstance(rec, dict) or rec.get("kind") != "invocation":
                        continue
                    out = reduce_invocation(rec)
                except ValueError:
                    if '"invocation"' not in line:
                        continue
                    out = {"kind": "invocation", "provider": LUNA if LUNA in line else "unknown",
                           "ts_ms": int(time.time() * 1000), "damaged": True}
                self.seen += 1
                out["src"] = f"b{at}"                         # the record's byte offset in the sealed ledger
                if out["src"] in self.done:
                    self.published += 1
                    continue
                text = json.dumps(out, sort_keys=True) + "\n"
                if leaked_names(self.names, text):            # scanned BEFORE publishing
                    self.breaches += 1
                    continue
                _append(self.dst, text, self.root)
                self.done.add(out["src"])
                self.published += 1

    def run_while(self, proc, every=1.0):
        stop = threading.Event()

        def loop():
            while not stop.wait(every):
                self.sweep()
        t = threading.Thread(target=loop, daemon=True)
        t.start()
        try:
            return proc.wait()
        finally:
            stop.set()
            t.join()
            self.sweep(final=True)


def sealed_hits(out, names):
    """Files in the sealed out dir that contain any hidden test name (literal substring)."""
    n = 0
    for p in sorted(Path(out).rglob("*")) if Path(out).exists() else []:
        if p.is_file() and not p.is_symlink() and leaked_names(names, p.read_text(errors="replace")):
            n += 1
    return n


def hidden_names(tasks):
    names = set()
    for t in tasks:
        names |= test_names((SUITE / t / "hidden_tests.rs").read_text())
    return names


def publish_summary(rel, summary, names):
    text = json.dumps(summary, indent=1, sort_keys=True) + "\n"
    if leaked_names(names, text):
        raise RuntimeError("seal: hidden test name in a summary")
    save_json(OUT_REPO / rel / "summary.json", summary)


# ------------------------------------------------------------------ registry

def reconcile(reg, names_of):
    """A run left `launched` (its driver died): sweep its accounting once more from the sealed
    ledger, then mark it failed (interrupted). It stays spent."""
    from gym_band import sealed_out
    changed = False
    for rel, e in reg.items():
        if e.get("status") == "launched":
            acc = Accounting(sealed_out(rel) / "ledger.jsonl", OUT_REPO / rel / "ledger.jsonl", names_of(e))
            acc.sweep(final=True)
            e.update({"status": "failed", "interrupted": True, "accounting_published": acc.published})
            changed = True
    return changed


class Refusal(Exception):
    def __init__(self, status, code=2, **fields):
        super().__init__(status)
        self.status, self.code, self.fields = status, code, fields


def select_families(args, sp, act, cert, reg):
    """(families, wave, purpose) for this call, or Refusal."""
    from gym_band import SELECTION, acceptance_spent, wave_record
    from gym_split import holder, load_waves
    if "--acceptance" in args:
        if acceptance_spent(reg):
            raise Refusal("acceptance-exists")
        sel = load_json(SELECTION)
        if not sel:
            raise Refusal("acceptance-not-ready")
        if sel.get("status") != "ok":
            raise Refusal("floors_unmet")
        if any(cert["content"].get(t) != h for t, h in sel["content"].items()):
            raise Refusal("content-mismatch")                   # finalize again on the current certification
        return sorted(sel["retained"]), None, "acceptance"
    wave = int(args[args.index("--wave") + 1])
    frozen = load_waves().get(str(wave))
    if not frozen:
        raise Refusal("wave-not-frozen")
    if "--repilot" in args:
        rec = wave_record(wave)
        fams = sorted(r["family"] for r in rec["slots"].values()
                      if r["status"] == "pending" and r["reason"] in ("repilot", "invalidated"))
        return fams, wave, "repilot"
    if any(e.get("wave") == wave and e.get("purpose") == "pilot" and e.get("status") in SPENT for e in reg.values()):
        raise Refusal("wave-already-piloted")
    fams = [f for f in (holder(s, act) for s in frozen["slots"]) if f]
    if any(not all(f"{f}-l{lv}" in cert["content"] for lv in (1, 2, 3, 4)) for f in fams):
        raise Refusal("wave-not-certified")                     # never silently narrowed
    return sorted(fams), wave, "pilot"


def next_rel(reg, acceptance, wave, purpose):
    """A stale `reserved` entry (nothing launched) of the same purpose is reused; else the next N."""
    parent = str(out_dir(acceptance, wave, 0).parent)
    mine = {k: e for k, e in reg.items() if k.startswith(parent + "/")}
    for k, e in sorted(mine.items(), key=lambda kv: kv[1].get("seq", 0)):
        if e.get("status") == "reserved" and e.get("purpose") == purpose:
            return Path(k)
    return out_dir(acceptance, wave, len(mine) + 1)


@gateway("pilot")
def main():
    from gym_band import manifest_path, run_name, sealed_out
    from gym_export import certified_current
    from gym_split import assignment, committed, load_split, split_sha256
    args = sys.argv[1:]
    binary = Path(args[args.index("--binary") + 1]) if "--binary" in args else BINARY
    if not binary_allowed(binary, BINARY):
        emit(stage="pilot", status="binary-override-refused")
        sys.exit(2)
    gate_or_exit("pilot")
    if not prereqs_ok():
        emit(stage="pilot", status="prereqs-not-met")
        sys.exit(2)
    with DriverLock() as lock:
        reg = load_json(REGISTRY, {})
        if reconcile(reg, lambda e: hidden_names(sorted(f"{f}-l{lv}" for f in e.get("families", []) for lv in (1, 2, 3, 4)))):
            save_json(REGISTRY, reg)
            emit(stage="pilot", status="interrupted-run-failed")
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
        try:
            fams, wave, purpose = select_families(args, sp, act, cert, reg)
        except Refusal as r:
            emit(stage="pilot", status=r.status)
            sys.exit(r.code)
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
        names = hidden_names(tasks)
        rel = next_rel(reg, acceptance, wave, purpose)
        if not cap_visible(rel):
            raise RuntimeError("ledger depth outside the cap globs")
        publish = OUT_REPO / rel
        repo_write_root(publish / "ledger.jsonl")
        name = run_name(rel)
        out, sub = sealed_out(rel), RUNS / f"pilot-{name}-suite"
        if publish.exists() or out.exists():
            raise RuntimeError("run exists")
        if sub.exists():
            shutil.rmtree(sub)
        for t in tasks:
            for p in sorted((SUITE / t).rglob("*")):
                if p.is_file():
                    safe_copy(p, sub / t / p.relative_to(SUITE / t), SUITE)
        content = {t: hashes_digest(tree_hashes(sub / t)) for t in tasks}
        if any(content[t] != cert["content"][t] for t in tasks):
            raise RuntimeError("pilot copy differs from the certified suite")
        snap = RUNS / f"pilot-{name}-harness.py"
        safe_copy(E007, snap, E007.parent)
        if sha256(snap) != E007_SHA:
            raise RuntimeError("harness snapshot")
        # the pin bracket and the temp-root isolation check (before; repeated after the run)
        before = codex_fingerprint()
        if not codex_pinned() or before is None:
            emit(stage="pilot", status="codex-not-pinned")
            sys.exit(2)
        if not isolation_ok():
            emit(stage="pilot", status="isolation-failed")
            sys.exit(2)
        if not TESTING and not build_a2d():                   # before any reservation: never spends a run
            emit(stage="pilot", status="build-failed")
            sys.exit(1)
        if not launch_gate():
            emit(stage="pilot", status="launch-gate-closed")
            sys.exit(2)
        seed = int(hashlib.sha256(f"gym-v3-pilot:{sp['seed']}:{rel}".encode()).hexdigest()[:8], 16)
        manifest = {"purpose": purpose, "wave": wave, "families": fams, "content": content, "suite": cert["suite"],
                    "spec_sha256": spec_sha(), "harness_sha256": E007_SHA, "split_sha256": split_sha256(),
                    "seed": seed, "k": K, "model": LUNA, "effort": "low", "max_invocations": worst,
                    "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
        save_json(manifest_path(rel), manifest)
        reg[str(rel)] = {"seq": max([e.get("seq", 0) for e in reg.values()] + [0]) + 1, "purpose": purpose,
                         "wave": wave, "threshold": acceptance, "status": "reserved", "families": fams,
                         "manifest_sha256": sha256(manifest_path(rel))}
        save_json(REGISTRY, reg)                                # durable before any call
        argv = [str(binary), "lift", "run", "--experiment", f"C2-GYM3-{rel.parent.name.upper()}-{rel.name.upper()}",
                "--unregistered", "--suite", str(sub), "--model", LUNA, "--effort", "low", "--arms", "X:inc",
                "--k", str(K), "--seed", str(seed), "--jobs", "4", "--min-families", "1",
                "--max-invocations", str(worst), "--max-wall-clock-secs", "21600", "--out", str(out),
                "--external", f"inc={snap}"]
        for lane, spec in lanes_of(snap).items():
            argv += ["--lane", f"{lane}={spec}"]
        publish.mkdir(parents=True, exist_ok=True)
        acc = Accounting(out / "ledger.jsonl", publish / "ledger.jsonl", names)
        reg[str(rel)]["status"] = "launched"
        save_json(REGISTRY, reg)
        with open(RECORDS / f"pilot-{name}.log", "wb") as log:
            proc = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                                    pass_fds=(lock.fd,), env=child_env())
            code = acc.run_while(proc)
        after = codex_fingerprint()
        bracket_ok = after is not None and after == before and isolation_ok()
        hits = sealed_hits(out, names)
        complete_acc = acc.breaches == 0 and acc.published == acc.seen
        ok = code == 0 and hits == 0 and bracket_ok and complete_acc
        reg = load_json(REGISTRY, {})
        reg[str(rel)].update({"status": "complete" if ok else "failed", "exit": code, "seal_breaches": hits + acc.breaches,
                              "codex_bracket_ok": bracket_ok, "accounting_published": acc.published})
        save_json(REGISTRY, reg)
        publish_summary(rel, {"run": str(rel), "purpose": purpose, "wave": wave, "families": fams,
                              "tasks": len(tasks), "content": content, "suite": cert["suite"],
                              "spec_sha256": manifest["spec_sha256"], "harness_sha256": E007_SHA,
                              "split_sha256": manifest["split_sha256"], "manifest_sha256": reg[str(rel)]["manifest_sha256"],
                              "seed": seed, "k": K, "model": LUNA, "max_invocations": worst, "exit": code,
                              "status": reg[str(rel)]["status"], "invocations_published": acc.published,
                              "seal_breaches": hits + acc.breaches, "codex_bracket_ok": bracket_ok}, names)
        emit(stage="pilot", exit=code, families=len(fams), tasks=len(tasks), ok=ok,
             status="seal-breach" if hits or acc.breaches else (
                 ("acceptance" if acceptance else f"wave-{wave}") if ok else "run-failed"))
        if not ok:
            sys.exit(1)


if __name__ == "__main__":
    main()
