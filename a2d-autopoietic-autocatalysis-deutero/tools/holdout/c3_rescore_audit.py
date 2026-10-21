"""Independent re-score of the step-5 audit's submissions (Sol registration re-check, 10-21).

The audit's controller binary cannot be rebuilt byte for byte (debug builds are not
reproducible here), so the audit's evidence is bound another way, with no model call: each
task's final informative audit round's submitted file is compiled with the task's hidden tests
and run by an oracle independent of `a2d` (Python + rustc), and the verdict is compared with
the audit's recorded one. Compile and run are both confined by seatbelt profiles modelled on
the controller's oracle (crates/a2d-core/src/sandbox.rs): no network, no file access under
HOME, TMPDIR or the temp roots except the step's own directory (plus the toolchain for the
compile), and the run step may not spawn processes, look up Mach services or send Apple Events.

It also recounts the tool events of every invocation transcript of those tuples with the bound
runner's own port of the provider's allowlist detector (nano/confirm3.py `py_tool_events`),
independently of the audit binary's recorded counts (Sol second re-check, 10-21).

Prints counts only and writes runs/c2-confirm3/audit-rescore.json (counts only), which the
registration generator's certification requires (all agree; every transcript present, 0 events).

usage: python3 -I tools/holdout/c3_rescore_audit.py
"""
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import HOLDOUT, RECORDS, REPO, SUITE, TASK_RE, load_json, safe_read_text  # noqa: E402

sys.path.insert(0, str(REPO / "nano"))
import confirm3  # noqa: E402

RUSTC = str(Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc")
OUT = REPO / "runs" / "c2-confirm3" / "audit-rescore.json"


def sb(path):
    return '"' + str(path).replace("\\", "\\\\").replace('"', '\\"') + '"'


def denied():
    roots = ["/private/tmp", "/private/var/folders", "/private/var/tmp"]
    roots += [os.path.realpath(os.environ[n]) for n in ("HOME", "TMPDIR") if os.environ.get(n)]
    return " ".join(f"(subpath {sb(r)})" for r in roots)


def xcrun_cache_rule():
    """The linker's xcrun shim caches its toolchain lookup in $TMPDIR/xcrun_db (as the oracle allows)."""
    tmp = os.environ.get("TMPDIR")
    if not tmp:
        return ""
    real = os.path.realpath(tmp).rstrip("/")
    escaped = "".join("\\" + c if c in "\\.^$|?*+()[]{}\"" else c for c in real)
    return f'(allow file-read* file-write* (regex #"^{escaped}/xcrun_db(-[A-Za-z0-9]+)?$"))'


def compile_profile(workdir, sysroot):
    return (f"(version 1)(allow default)(deny network*)(deny file-read* file-write* {denied()})"
            f"(allow file-read* (subpath {sb(sysroot)})){xcrun_cache_rule()}"
            f"(allow file-read* file-write* (subpath {sb(os.path.realpath(workdir))}))")


def run_profile(binary, workdir):
    binary = os.path.realpath(binary)
    return (f"(version 1)(allow default)(deny network*)(deny file-read* file-write* {denied()})"
            f"(deny process-fork process-exec* mach-lookup appleevent-send)"
            f"(allow process-exec (literal {sb(binary)}))(allow file-read* (literal {sb(binary)}))"
            f"(allow file-read* file-write* (subpath {sb(os.path.realpath(workdir))}))")


def score(code, tests, timeout, sysroot):
    """The controller's oracle layout: candidate root + #[cfg(test)] #[path] test module."""
    with tempfile.TemporaryDirectory() as d:
        d = Path(d)
        lines, attrs = code.splitlines(), []
        while lines and (lines[0].strip().startswith("#![") or lines[0].strip().startswith("//") or not lines[0].strip()):
            attrs.append(lines.pop(0))
        (d / "c.rs").write_text("\n".join(attrs) + "\n#[cfg(test)]\n#[path = \"t.rs\"]\nmod oracle;\n" + "\n".join(lines))
        (d / "t.rs").write_text("#![allow(unused_imports, dead_code)]\nuse super::*;\n" + tests)
        # The oracle's compile environment: these variables only, from the parent (sandbox.rs `sandboxed`).
        env = {k: os.environ[k] for k in ("PATH", "HOME", "LANG", "TMPDIR", "DEVELOPER_DIR", "SDKROOT") if k in os.environ}
        try:
            c = subprocess.run(["/usr/bin/sandbox-exec", "-p", compile_profile(d, sysroot), RUSTC, "--test", "--edition",
                                "2024", str(d / "c.rs"), "-o", str(d / "b")], capture_output=True, cwd=d, env=env,
                               timeout=180)
        except subprocess.TimeoutExpired:
            return {"compiled": False, "ok": False, "compile_timed_out": True}
        if c.returncode:
            return {"compiled": False, "ok": False}
        (d / "c.rs").unlink()
        (d / "t.rs").unlink()               # the code under test cannot read the sources at run time
        try:
            r = subprocess.run(["/usr/bin/sandbox-exec", "-p", run_profile(d / "b", d), str(d / "b"), "oracle::"],
                               capture_output=True, timeout=timeout, cwd=d, env={"TMPDIR": str(d)})
        except subprocess.TimeoutExpired:
            return {"compiled": True, "ok": False, "timed_out": True}
        return {"compiled": True, "ok": r.returncode == 0}


def main():
    sysroot = subprocess.run([RUSTC, "--print", "sysroot"], capture_output=True, text=True).stdout.strip()
    audit = load_json(RECORDS / "audit.json", {})
    tasks = sorted(p.name for p in SUITE.iterdir() if TASK_RE.match(p.name))
    counts = {"tasks": len(tasks), "agree": 0, "disagree": 0, "missing": 0, "compile_failed": 0, "timed_out": 0,
              "transcripts": 0, "transcripts_missing": 0, "recounted_tool_events": 0}
    ledgers = {}
    by_run = {}
    for t in tasks:
        final = audit.get(t, {}).get("r2") or audit.get(t, {}).get("r1") or {}
        run = HOLDOUT / "runs" / f"c3-audit-{final.get('run')}"
        subs = sorted((run / "submissions" / "D" / t).glob("*.rs")) if (run / "submissions" / "D" / t).exists() else []
        if not subs or not isinstance(final.get("passed"), bool):
            counts["missing"] += 1
            continue
        if run not in ledgers:
            ledgers[run], bad = confirm3.records_tolerant(run)
            counts["transcripts_missing"] += bad      # a damaged audit-ledger line is unauditable
        for r in ledgers[run]:
            if r.get("kind") == "invocation" and r.get("task") == t and r.get("arm") == "D":
                text = confirm3.transcript_text(run, r.get("prompt_path"))
                if text is None or confirm3.CODEX_STDERR_MARKER not in text:
                    counts["transcripts_missing"] += 1
                else:
                    counts["transcripts"] += 1
                    counts["recounted_tool_events"] += confirm3.py_tool_events(text, failed=r.get("ok") is False)
        meta = json.loads(safe_read_text(SUITE / t / "task.json", HOLDOUT))
        res = score(safe_read_text(subs[-1], HOLDOUT), safe_read_text(SUITE / t / "hidden_tests.rs", HOLDOUT),
                    meta.get("test_timeout_secs", 10), sysroot)
        counts["compile_failed"] += not res["compiled"]
        counts["timed_out"] += bool(res.get("timed_out"))
        agree = res["ok"] == final["passed"]
        counts["agree" if agree else "disagree"] += 1
        key = str(final.get("run"))
        by_run.setdefault(key, {"tasks": 0, "agree": 0})
        by_run[key]["tasks"] += 1
        by_run[key]["agree"] += agree
    record = {**counts, "by_audit_run": by_run,
              "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "oracle": "python3 + rustc --test --edition 2024, oracle layout, compile and run under seatbelt profiles "
                        "modelled on crates/a2d-core/src/sandbox.rs; verdict = test binary exit 0"}
    OUT.write_text(json.dumps(record, indent=1) + "\n")
    print(json.dumps(record))


if __name__ == "__main__":
    main()
