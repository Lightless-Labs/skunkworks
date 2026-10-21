"""Validate the assembled gym v3 suite with a sandboxed oracle (plan 2026-10-21, "Isolation":
every compile is sandboxed, the API-surface check included; `a2d lift validate` is not used,
because its API-surface check runs rustc outside any sandbox, crates/a2d-core/src/sandbox.rs
`compile_tests_against_library`, and seatbelt profiles cannot nest).

The checks are crates/a2d-lift/src/validate.rs's, ported: >= 3 public and >= 20 hidden tests;
the reference passes public and hidden (twice, identically); public and hidden tests compile
against the reference as a separate crate (the API surface); >= 4 mutants, each failing a
hidden assertion (a compile error, or a timeout, is not a catch); >= 2 mutants pass public;
the public tests call every `pub fn` the requirements name; no hidden test repeats a public
name. Added for the frozen rubric: M1, M2, M5 and the difficulty tags (gym_rubric), and for
scale tasks the timing rule: the reference's hidden tests run within 5% of `test_timeout_secs`
(libtest's own `finished in` time, measured while the validator runs --jobs tasks in parallel),
and the mutant labelled `naive` exceeds the limit (it is caught by the timeout, and only it may be);
so the naive mutant is at least 20x slower than the reference.

Oracle layout as crates/a2d-lift/src/oracle.rs: the candidate is the crate root, the tests a
#[cfg(test)] #[path] module declared first with trusted assertion macros; sources are deleted
before the binary runs. rustc runs under gymlib.compile_profile (HOME, TMPDIR and the temp roots
denied except the step dir; sysroot and xcrun cache allowed; env PATH/HOME/LANG/TMPDIR/
DEVELOPER_DIR/SDKROOT); the test binary under gymlib.run_profile (no fork/exec/Mach/AE).

Verdict lines (`<id>: OK (...)` / `<id>: REJECT <problems>`) go to records/validate-<stamp>.txt,
never to stdout; each family with rejected tasks gets FIX-VERDICTS.txt; tasks still rejected
after a family's 2nd repair round are excluded (reason validation-cap). Prints only counts and
opaque task ids.

usage: python3 -I tools/gym/gym_validate.py [--jobs 4]
"""
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gym_rubric  # noqa: E402
from gymlib import (GYM, RECORDS, REPO, SUITE, TASK_RE, TESTING, DriverLock, compile_env, emit,  # noqa: E402
                    gate_or_exit, gateway, hashes_digest, load_json, parse_task_json, run_profile, safe_write_text,
                    sandboxed_rustc, save_json, strict_json_doc, suite_digest, task_timeout, tree_hashes)
from gymvendor import lines as rust_lines, test_names  # noqa: E402

MIN_MUTANTS, MIN_PUBLIC_GREEN, MIN_HIDDEN, MIN_PUBLIC = 4, 2, 20, 3
COMPILE_TIMEOUT = 120
SCALE_SHARE = 0.05              # the rule (SPEC.md): the reference uses at most 5% of test_timeout_secs
SCALE_MARGIN_SHARE = 0.04       # applied at validation: max(libtest time, wall) <= 4% (noise margin)
MAX_FIX_ROUNDS = 2
TRUSTED_MACROS = ("use ::std::{assert, assert_eq, assert_ne, debug_assert, debug_assert_eq, debug_assert_ne, "
                  "matches, unreachable};")
_counter = [0]
_counter_lock = threading.Lock()
# Tests only: identical (code, tests) pairs across fixture families are scored once.
_MEMO = {} if TESTING else None
_memo_lock = threading.Lock()


def count_tests(source):
    return source.count("#[test]")


def split_inner_attributes(code):
    """Leading `#![...]` attributes, `//` comments and blank lines (oracle.rs)."""
    attrs, lines, open_brackets, i = [], rust_lines(code), 0, 0
    while i < len(lines):
        t = lines[i].strip()
        if open_brackets > 0 or t.startswith("#!["):
            open_brackets += t.count("[") - t.count("]")
            attrs.append(lines[i])
        elif not t or t.startswith("//"):
            attrs.append(lines[i])
        else:
            break
        i += 1
    return "\n".join(attrs), "\n".join(lines[i:])


def parse_results(output):
    """(passed, failed, secs) from the last `test result:` line of the binary's STDOUT (libtest
    prints its summary there; code under test can write anything to stderr); secs is libtest's
    own `finished in <x>s`, or None."""
    for line in reversed(rust_lines(output)):
        if line.startswith("test result:"):
            passed = failed = 0
            secs = None
            parts = line.split(". ", 1)
            if len(parts) > 1:
                for part in parts[1].split(";"):
                    p = part.strip()
                    if p.endswith(" passed"):
                        passed = int(p[:-len(" passed")].strip() or 0)
                    if p.endswith(" failed"):
                        failed = int(p[:-len(" failed")].strip() or 0)
                    if p.startswith("finished in ") and p.endswith("s"):
                        try:
                            secs = float(p[len("finished in "):-1])
                        except ValueError:
                            secs = None
            return passed, failed, secs
    return 0, 0, None


def _nonce(label, code):
    with _counter_lock:
        _counter[0] += 1
        n = _counter[0]
    seed = f"{label}:{time.time_ns()}:{os.getpid()}:{n}:{hashlib.sha256(code.encode()).hexdigest()}"
    return hashlib.sha256(seed.encode()).hexdigest()[:16]


def _run_binary(binary, run_dir, test_filter, timeout):
    env = dict(compile_env(), TMPDIR=str(run_dir))
    start = time.monotonic()
    p = subprocess.Popen(["/usr/bin/sandbox-exec", "-p", run_profile(binary, run_dir), str(binary), test_filter],
                         cwd=run_dir, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                         stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        out, _err = p.communicate(timeout=timeout)
        return out, p.returncode, False, time.monotonic() - start
    except subprocess.TimeoutExpired:
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
        p.communicate()
        return "", None, True, time.monotonic() - start


def score_tests(code, tests, label, timeout_secs, rep=0):
    """The oracle: compile `code` with `tests` (sandboxed), run the test module (sandboxed)."""
    key = hashlib.sha256(json.dumps([code, tests, label, timeout_secs, rep]).encode()).hexdigest()
    if _MEMO is not None:
        with _memo_lock:
            if key in _MEMO:
                return _MEMO[key]
    expected = count_tests(tests)
    nonce = _nonce(label, code)
    module, tests_file = f"a2d_{label}_{nonce}", f"t_{nonce}.rs"
    attrs, body = split_inner_attributes(code)
    root = f"{attrs}\n#[cfg(test)]\n#[path = \"{tests_file}\"]\nmod {module};\n\n{body}\n"
    tmod = f"#![allow(unused_imports, dead_code)]\nuse super::*;\n{TRUSTED_MACROS}\n\n{tests}\n"
    build = Path(tempfile.mkdtemp(prefix="gym-oracle-")).resolve()
    run = Path(tempfile.mkdtemp(prefix="gym-oracle-run-")).resolve()
    res = {"compiled": False, "passed": 0, "failed": 0, "expected": expected, "timed_out": False,
           "exit_success": False, "all_passed": False, "secs": None}
    try:
        (build / f"c_{nonce}.rs").write_text(root)
        (build / tests_file).write_text(tmod)
        try:
            c = sandboxed_rustc(["--test", "--edition", "2024", str(build / f"c_{nonce}.rs"), "-o",
                                 str(build / "oracle_bin")], build, timeout=COMPILE_TIMEOUT)
        except subprocess.TimeoutExpired:
            res["timed_out"] = True
            return _remember(key, res)
        if c.returncode != 0:
            return _remember(key, res)
        (build / f"c_{nonce}.rs").unlink()
        (build / tests_file).unlink()
        out, code_, timed_out, wall = _run_binary(build / "oracle_bin", run, f"{module}::", timeout_secs)
        passed, failed, secs = (0, 0, None) if timed_out else parse_results(out)
        # the timing rule takes the larger of libtest's own time and the wall clock (a forged
        # or missing summary can only make the reference look slower, never faster)
        res.update({"compiled": True, "passed": passed, "failed": failed, "timed_out": timed_out,
                    "exit_success": code_ == 0, "secs": max(secs or 0.0, wall), "wall_secs": wall})
        res["all_passed"] = (code_ == 0 and not timed_out and failed == 0 and expected > 0 and passed == expected)
        return _remember(key, res)
    finally:
        shutil.rmtree(build, ignore_errors=True)
        shutil.rmtree(run, ignore_errors=True)


def _remember(key, res):
    if _MEMO is not None:
        with _memo_lock:
            _MEMO[key] = res
    return res


def api_surface_ok(reference, tests):
    """Tests compile against the reference as a separate crate (public items only), sandboxed."""
    key = hashlib.sha256(json.dumps(["api", reference, tests]).encode()).hexdigest()
    if _MEMO is not None:
        with _memo_lock:
            if key in _MEMO:
                return _MEMO[key]
    d = Path(tempfile.mkdtemp(prefix="gym-api-")).resolve()
    try:
        (d / "lib.rs").write_text(reference)
        (d / "tests.rs").write_text(f"#![allow(unused_imports, dead_code)]\nuse a2d_reference::*;\n{tests}")
        rlib = d / "liba2d_reference.rlib"
        try:
            lib = sandboxed_rustc(["--edition", "2024", "--crate-type", "lib", "--crate-name", "a2d_reference",
                                   str(d / "lib.rs"), "-o", str(rlib)], d, timeout=COMPILE_TIMEOUT)
            ok = lib.returncode == 0 and sandboxed_rustc(
                ["--test", "--edition", "2024", "--extern", f"a2d_reference={rlib}", str(d / "tests.rs"), "-o",
                 str(d / "tests_bin")], d, timeout=COMPILE_TIMEOUT).returncode == 0
        except subprocess.TimeoutExpired:
            ok = False
        return _remember(key, ok)
    finally:
        shutil.rmtree(d, ignore_errors=True)


def uncovered_api(requirements, public_tests):
    out = set()
    at = requirements.find("pub fn ")
    while at >= 0:
        rest = requirements[at + len("pub fn "):]
        path = ""
        for ch in rest:
            if ch.isascii() and (ch.isalnum() or ch in "_:"):
                path += ch
            else:
                break
        name = path.rsplit("::", 1)[-1]
        if name and f"{name}(" not in public_tests and f"{name}::<" not in public_tests:
            out.add(name)
        at = requirements.find("pub fn ", at + 1)
    return out


def validate_task_safe(d, family_class):
    """validate_task, with any exception turned into a REJECT reason (a malformed model output
    is repairable through FIX, never a crash of the whole validation)."""
    try:
        return validate_task(d, family_class)
    except Exception:  # noqa: BLE001 - deliberately content-free
        return ["malformed task files"], {"mutants": 0, "caught": 0, "public_green": 0}


def validate_task(d, family_class):
    """(problems, counts) for one suite task dir."""
    tid = d.name
    level = int(tid[-1])
    problems, counts = [], {"mutants": 0, "caught": 0, "public_green": 0}
    if family_class not in gym_rubric.CLASSES:
        return ["no rubric class for the family"], counts
    try:
        tj = parse_task_json((d / "task.json").read_bytes())
    except (OSError, ValueError):
        return ["task.json does not match the controller's schema"], counts
    req, timeout = tj["requirements"], task_timeout(tj)
    public = (d / "public_tests.rs").read_text() if (d / "public_tests.rs").exists() else ""
    hidden = (d / "hidden_tests.rs").read_text()
    try:
        rubric = strict_json_doc((d / "rubric.json").read_bytes())
        shape = gym_rubric.rubric_problems(rubric)
    except (OSError, ValueError):
        rubric, shape = {}, ["rubric.json unreadable"]
    problems += shape
    if shape:
        rubric = {}
    kinds = {m: e.get("kind") for m, e in (rubric.get("mutants") or {}).items()}
    scale = "scale" in (rubric.get("difficulty") or [])
    n_public, n_hidden = count_tests(public), count_tests(hidden)
    if n_public < MIN_PUBLIC:
        problems.append(f"only {n_public} public tests (minimum {MIN_PUBLIC})")
    if n_hidden < MIN_HIDDEN:
        problems.append(f"only {n_hidden} hidden tests (minimum {MIN_HIDDEN})")
    reference = (d / "reference.rs").read_text() if (d / "reference.rs").exists() else None
    if reference is None:
        problems.append("missing reference.rs")
    else:
        if n_public and not score_tests(reference, public, "public", timeout)["all_passed"]:
            problems.append("reference fails public")
        for label, tests in (("public", public), ("hidden", hidden)):
            if count_tests(tests) and not api_surface_ok(reference, tests):
                problems.append(f"{label} tests need non-public reference items (fail against a separate crate)")
        h1 = score_tests(reference, hidden, "hidden", timeout, rep=0)
        h2 = score_tests(reference, hidden, "hidden", timeout, rep=1)
        if h1["all_passed"] and (not h2["all_passed"] or h2["passed"] != h1["passed"]):
            problems.append("reference hidden results are not deterministic")
        if not h1["all_passed"]:
            problems.append(f"reference fails hidden ({h1['passed']}/{h1['expected']}"
                            f"{'' if h1['compiled'] else ', does not compile'})")
        if scale and h1["all_passed"]:
            secs = max(h1["secs"] or 0, h2["secs"] or 0)
            if secs > SCALE_MARGIN_SHARE * timeout:
                problems.append("scale: reference hidden run exceeds 4% of the time limit (rule 5%, validation margin)")
    mdir = d / "mutants"
    mutants = sorted(p for p in mdir.iterdir() if p.suffix == ".rs") if mdir.is_dir() else []
    counts["mutants"] = len(mutants)
    if len(mutants) < MIN_MUTANTS:
        problems.append(f"{len(mutants)} mutants (minimum {MIN_MUTANTS})")
    naive_timed_out = False
    for p in mutants:
        stem, code = p.stem, p.read_text()
        h = score_tests(code, hidden, "hidden", timeout)
        public_ok = bool(n_public) and score_tests(code, public, "public", timeout)["all_passed"]
        naive = scale and kinds.get(stem) == "naive"
        if naive:
            if not public_ok:
                problems.append(f"scale: naive mutant {stem} fails the public tests (it must be correct but slow)")
            elif h["compiled"] and h["timed_out"]:
                naive_timed_out = True
                counts["caught"] += 1
            else:
                problems.append(f"scale: naive mutant {stem} does not exceed the time limit")
        elif h["all_passed"]:
            problems.append(f"mutant {stem} passes hidden")
        elif not h["compiled"]:
            problems.append(f"mutant {stem} does not compile against hidden tests")
        elif h["timed_out"]:
            problems.append(f"mutant {stem} is caught only by a timeout")
        elif h["failed"] == 0:
            problems.append(f"mutant {stem} is not caught by a failing test")
        else:
            counts["caught"] += 1
        if public_ok:
            counts["public_green"] += 1
    if scale and not naive_timed_out and not any("naive mutant" in x for x in problems):
        problems.append("scale: no naive mutant")
    if n_public and counts["public_green"] < MIN_PUBLIC_GREEN:
        problems.append(f"{counts['public_green']} mutants pass public tests (minimum {MIN_PUBLIC_GREEN})")
    unc = uncovered_api(req, public)
    if n_public and unc:
        problems.append("public tests never call " + ", ".join(sorted(unc)))
    shared = len(test_names(public) & test_names(hidden))
    if shared:
        problems.append(f"hidden repeats {shared} public test name(s)")
    if not shape:
        rub = gym_rubric.check_task(req, hidden, public, reference or "", rubric, level, family_class)
        for k in gym_rubric.CHECKS:
            if not rub[k]["ok"]:
                detail = ""
                if k == "m1" and rub[k].get("uncovered_ids"):
                    detail = " uncovered " + ",".join(rub[k]["uncovered_ids"])
                problems.append(f"rubric {k.upper()} failed{detail}")
    return problems, counts


ORACLE_FILES = ("gym_validate.py", "gym_rubric.py", "gymvendor.py", "gymlib.py")


def oracle_sha():
    """The validator's own code (oracle, rubric checks, profiles): a cached verdict is reused only
    under exactly this code."""
    h = hashlib.sha256()
    for f in ORACLE_FILES:
        h.update(hashlib.sha256((Path(__file__).resolve().parent / f).read_bytes()).digest())
    return h.hexdigest()


def verdict_line(tid, problems, counts):
    if problems:
        return f"{tid}: REJECT {'; '.join(problems)}"
    return (f"{tid}: OK (mutants caught by hidden {counts['caught']}/{counts['mutants']}, "
            f"passing public {counts['public_green']})")


@gateway("validate")
def main():
    args = sys.argv[1:]
    jobs = int(args[args.index("--jobs") + 1]) if "--jobs" in args else 4
    gate_or_exit("validate")
    with DriverLock():
        head = subprocess.run(["git", "-C", str(REPO), "rev-parse", "HEAD"], capture_output=True, text=True,
                              stdin=subprocess.DEVNULL).stdout.strip()
        expected = sorted(p.name for p in SUITE.iterdir() if TASK_RE.match(p.name))
        if not expected:
            raise ValueError("empty suite")
        digest = suite_digest()
        classes = load_json(RECORDS / "active.json", {}).get("classes", {})
        if any(classes.get(t.rsplit("-l", 1)[0]) not in gym_rubric.CLASSES for t in expected):
            emit(stage="validate", status="class-missing")          # every family needs its rubric class
            sys.exit(2)
        cache = load_json(RECORDS / "validate-cache.json", {})
        oracle = oracle_sha()

        def one(t):
            cls = classes[t.rsplit("-l", 1)[0]]
            key = f"{hashes_digest(tree_hashes(SUITE / t))}:{cls}:{oracle}"
            if key in cache:                                           # passed before on exactly this content
                return t, [], dict(cache[key]), key
            problems, counts = validate_task_safe(SUITE / t, cls)
            return t, problems, counts, key
        with ThreadPoolExecutor(max_workers=max(1, jobs)) as ex:
            scored = list(ex.map(one, expected))
        for t, p, c, key in scored:
            if not p:
                cache[key] = c
        save_json(RECORDS / "validate-cache.json", cache)
        results = [(t, p, c) for t, p, c, _ in scored]
        lines = [verdict_line(t, p, c) for t, p, c in results]
        rejected = sorted(t for t, p, _ in results if p)
        lines.append(f"suite {digest} — {len(expected)} tasks, {len(expected) - len(rejected)} accepted, "
                     f"{len(rejected)} rejected")
        full = "\n".join(lines) + "\n"
        stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
        safe_write_text(RECORDS / f"validate-{stamp}.txt", full)
        per_family = {}
        for t, line in zip(expected, lines):
            if t in rejected:
                per_family.setdefault(t.rsplit("-l", 1)[0], []).append(line)
        excluded = load_json(RECORDS / "excluded.json", {"tasks": [], "reasons": {}})
        excluded.setdefault("reasons", {})
        for fam_dir in GYM.glob("g3-f*"):
            fam = fam_dir.name
            target = fam_dir / "FIX-VERDICTS.txt"
            rounds = load_json(RECORDS / f"fix-{fam}" / "rounds.json", {"rounds": 0})["rounds"]
            if fam in per_family and rounds >= MAX_FIX_ROUNDS:
                excluded["tasks"] = sorted(set(excluded["tasks"]) | {t for t in rejected if t.startswith(fam + "-")})
                excluded["reasons"].setdefault(fam, "validation-cap")
            if fam in per_family and rounds < MAX_FIX_ROUNDS:
                safe_write_text(target, "\n".join(per_family[fam]) + "\n")
            elif target.exists() or target.is_symlink():
                target.unlink()
        save_json(RECORDS / "validate.json", {"stamp": stamp, "sha256": hashlib.sha256(full.encode()).hexdigest(),
                                              "rejected": rejected, "suite": digest, "head": head,
                                              "oracle_sha256": oracle, "sandboxed": True})
        save_json(RECORDS / "excluded.json", excluded)
        emit(stage="validate", tasks=len(expected), count=len(expected) - len(rejected), missing=len(rejected),
             skipped=len(excluded["tasks"]))
        for t in rejected:
            emit(stage="validate-reject", id=t)


if __name__ == "__main__":
    main()
