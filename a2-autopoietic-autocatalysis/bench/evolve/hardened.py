#!/usr/bin/env python3
"""Sandboxed pytest evaluation for the harness evolution loop."""

from __future__ import annotations

import argparse
import ast
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "e1"))

import common  # noqa: E402
import grading  # noqa: E402
import tasks  # noqa: E402
try:
    from .seatbelt import pytest_profile, run_sandboxed  # noqa: E402
except ImportError:  # direct `python bench/evolve/hardened.py`
    from seatbelt import pytest_profile, run_sandboxed  # type: ignore # noqa: E402



def venv_pytest_plugins(venv: str | Path) -> list[str]:
    """Modules of the venv's installed pytest11 entry points (diagnostic).

    Grading lets pytest autoload them, as E1 did: the venv is read-only to code under test, and the
    audit rejects package metadata in candidate diffs, so a candidate cannot add an entry point.
    Passing them explicitly with -p broke repos that import a plugin package twice (xdist) under
    `filterwarnings = error`.
    """
    plugins = []
    for entry_points in sorted(Path(venv).glob("lib/python*/site-packages/*.dist-info/entry_points.txt")):
        section = None
        for line in entry_points.read_text(errors="ignore").splitlines():
            line = line.strip()
            if line.startswith("["):
                section = line.strip("[]").strip()
            elif section == "pytest11" and "=" in line:
                plugins.append(line.split("=", 1)[1].split(":", 1)[0].strip())
    return sorted(set(plugins))
CONFIG_NAMES = {"pytest.ini", "tox.ini", "setup.cfg", "pyproject.toml", "conftest.py"}
CONTENT_TOKENS = re.compile(r"_pytest|\bpytest\b|junit|TestReport|pytest_runtest")
SYS_MODULES = re.compile(r"\bsys\s*\.\s*modules\s*\[")


@dataclass
class HardenedResult:
    returncode: int
    passed: set[str] = field(default_factory=set)
    failed: set[str] = field(default_factory=set)
    missing: set[str] = field(default_factory=set)
    duplicates: set[str] = field(default_factory=set)
    messages: dict[str, str] = field(default_factory=dict)
    output: str = ""
    report_issues: list[str] = field(default_factory=list)


def _diff_files(diff_text: str) -> list[tuple[str, list[str]]]:
    files: list[tuple[str, list[str]]] = []
    current = None
    added: list[str] = []
    for line in diff_text.splitlines():
        match = grading.DIFF_GIT_HEADER.match(line)
        if match:
            if current is not None:
                files.append((current, added))
            current, added = match.group(2), []
            continue
        if line.startswith("+++ b/"):
            current = line[6:].split("\t", 1)[0]
        if line.startswith("+") and not line.startswith("+++") and current is not None:
            added.append(line[1:])
    if current is not None:
        files.append((current, added))
    return files


IMPORTABLE_SUFFIXES = (".py", ".pyc", ".so", ".pyd")
# Shadowing these breaks the grader itself, whatever the repo's import roots are.
ALWAYS_PROTECTED = {"pytest", "_pytest", "py", "pluggy", "sitecustomize", "usercustomize"}


def _top_level_name(path: str, root: str) -> str | None:
    """The import name `path` would define if `root` is on sys.path, or None if not importable there."""
    prefix = "" if root in (".", "") else root.strip("/") + "/"
    if prefix and not path.startswith(prefix):
        return None
    rel = path[len(prefix):]
    if not rel.endswith(IMPORTABLE_SUFFIXES):
        return None
    top = rel.split("/", 1)[0]
    return top.split(".", 1)[0] if "/" not in rel else top


def audit_diff(diff_text: str, sys_path_roots: list[str | Path], protected_names: set[str]) -> list[str]:
    """Reject import shadowing, .pth files and conftest edits, and evaluator tokens in production code.

    Repo config files (setup.cfg, tox.ini, pyproject.toml, pytest.ini) are allowed: the runner passes a
    frozen copy of the base config with -c, so a candidate's edits to them cannot reach pytest.
    """
    reasons: list[str] = []
    roots = [Path(root).as_posix() for root in sys_path_roots]
    for path, added in _diff_files(diff_text):
        basename = Path(path).name
        if basename.endswith(".pth"):
            reasons.append(f"forbidden .pth file: {path}")
        if re.search(r"\.(dist|egg)-info(/|$)", path) or basename in ("entry_points.txt", "top_level.txt"):
            # Package metadata on an import root could register a pytest11 plugin that autoloads.
            reasons.append(f"forbidden package metadata: {path}")
        if basename == "conftest.py":
            reasons.append(f"forbidden conftest edit: {path}")
        top = _top_level_name(path, ".")
        if top in ALWAYS_PROTECTED:
            reasons.append(f"protected module shadow: {path}")
        for root in roots:
            top = _top_level_name(path, root)
            if top is not None and top in protected_names:
                reasons.append(f"protected module shadow: {path}")
        if not common.is_test_path(path):
            for line in added:
                if CONTENT_TOKENS.search(line) or SYS_MODULES.search(line):
                    reasons.append(f"pytest/evaluator manipulation in production path: {path}")
                    break
    reasons.extend(_unscanned_content(diff_text))
    return sorted(set(reasons))


def _unscanned_content(diff_text: str) -> list[str]:
    """Content the line audit cannot see: renames/copies carry no '+' lines, binary patches are encoded.

    Candidate diffs are generated with --no-renames, so a rename or copy header means a hand-made diff.
    """
    reasons: list[str] = []
    current = None
    for line in diff_text.splitlines():
        match = grading.DIFF_GIT_HEADER.match(line)
        if match:
            current = match.group(2)
            continue
        if current is None:
            continue
        for kind in ("rename", "copy"):
            if line.startswith(kind + " from ") or line.startswith(kind + " to "):
                reasons.append(f"forbidden {kind} (moved content is not audited): {current}")
        if (line == "GIT binary patch" or line.startswith("Binary files ")) and not common.is_test_path(current):
            reasons.append(f"forbidden binary patch in production path: {current}")
    return reasons


PY39_STDLIB = set("""__future__ _abc _ast _asyncio _bisect _blake2 _bootlocale _bootsubprocess _bz2 _codecs _codecs_cn _codecs_hk _codecs_iso2022 _codecs_jp _codecs_kr _codecs_tw _collections _contextvars _crypt _csv _ctypes _ctypes_test _curses _curses_panel _datetime _dbm _decimal _elementtree _functools _gdbm _hashlib _heapq _imp _io _json _locale _lsprof _lzma _md5 _multibytecodec _multiprocessing _opcode _operator _pickle _posixshmem _posixsubprocess _queue _random _sha1 _sha256 _sha3 _sha512 _signal _socket _sqlite3 _sre _ssl _stat _statistics _string _struct _symtable _thread _threading_local _tracemalloc _uuid _warnings _weakref _weakrefset _xxsubinterpreters _zoneinfo abc aifc antigravity argparse array ast asynchat asyncio asyncore atexit audioop base64 bdb binascii binhex bisect builtins bz2 cProfile calendar cgi cgitb chunk cmath cmd code codecs codeop collections colorsys compileall concurrent configparser contextlib contextvars copy copyreg crypt csv ctypes curses dataclasses datetime dbm decimal difflib dis distutils doctest email encodings ensurepip enum errno faulthandler filecmp fileinput fnmatch fractions ftplib functools gc genericpath getopt getpass gettext glob graphlib grp gzip hashlib heapq hmac html http idlelib imaplib imghdr imp importlib inspect io ipaddress itertools json keyword linecache locale logging lzma mailbox mailcap marshal math mimetypes mmap modulefinder multiprocessing netrc nis nntplib ntpath nturl2path numbers operator optparse os pathlib pdb pickle pickletools pipes pkgutil platform plistlib poplib posix pprint profile pstats pty pwd py_compile pyclbr pydoc pydoc_data queue quopri random re readline reprlib resource rlcompleter runpy sched secrets select selectors shelve shutil signal site smtpd smtplib sndhdr socket socketserver sqlite3 sre_compile sre_constants sre_parse ssl stat statistics string stringprep struct subprocess sunau symtable sys sysconfig syslog tabnanny tarfile telnetlib tempfile termios test textwrap threading time timeit tkinter token tokenize tomllib trace traceback tracemalloc tty turtle types typing unicodedata unittest urllib uu uuid venv warnings wave weakref webbrowser winreg wsgiref xdrlib xml xmlrpc zipapp zipfile zipimport zlib""".split())


def protected_module_names(venv: str | Path) -> set[str]:
    """Return stdlib, installed top-level import names, and fixed pytest controls."""
    names = set(getattr(sys, "stdlib_module_names", PY39_STDLIB))
    names.update({"pytest", "_pytest", "py", "pluggy", "sitecustomize", "usercustomize"})
    root = Path(venv)
    candidates = list(root.glob("lib/python*/site-packages")) + list(root.glob("Lib/site-packages"))
    for site in candidates:
        for child in site.iterdir():
            if child.is_dir() and not child.name.endswith((".dist-info", ".egg-info")):
                if child.name.endswith(".data"):
                    continue
                names.add(child.name.split(".", 1)[0])
            elif child.suffix == ".py":
                names.add(child.stem)
            elif child.name.endswith(".dist-info"):
                top = child / "top_level.txt"
                if top.is_file():
                    names.update(line.strip() for line in top.read_text(errors="ignore").splitlines() if line.strip())
    return names


def _test_functions(module: ast.Module):
    out = []
    def walk(node, classes=()):
        for child in getattr(node, "body", []):
            if isinstance(child, ast.ClassDef):
                walk(child, classes + (child.name,))
            elif isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)) and child.name.startswith("test"):
                out.append((child, classes))
    walk(module)
    return out


def _without_param_suffix(node_id: str) -> str:
    return re.sub(r"\[[^\]]*\]", "", node_id)


def strip_hidden_tests(tree: str | Path, visible_ids: list[str], hidden_ids: list[str], keep_ids: list[str]) -> dict:
    """Remove hidden-only test functions by AST line spans while preserving other source text."""
    root = Path(tree)
    visible = {_without_param_suffix(i) for i in visible_ids}
    hidden = {_without_param_suffix(i) for i in hidden_ids}
    keep = {_without_param_suffix(i) for i in keep_ids}
    paths = sorted({i.split("::", 1)[0] for i in hidden_ids})
    stripped: list[str] = []
    inseparable: list[str] = []
    for rel in paths:
        source = root / rel
        if not source.is_file():
            continue
        text = source.read_text()
        lines = text.splitlines(keepends=True)
        try:
            module = ast.parse(text)
        except SyntaxError:
            continue
        tests = _test_functions(module)
        remove_nodes = []
        class_methods: dict[tuple[str, ...], list[tuple[ast.AST, set[str], set[str]]]] = {}
        for node, classes in tests:
            base = rel + "::" + "::".join((*classes, node.name))
            h = {item for item in hidden if item == base}
            v = {item for item in visible | keep if item == base}
            if h and v:
                inseparable.extend(i for i in hidden_ids if _without_param_suffix(i) == base)
            if h and not v:
                class_methods.setdefault(classes, []).append((node, h, v))
                remove_nodes.append((node, h))
            elif classes:
                class_methods.setdefault(classes, []).append((node, h, v))
        # If every test method in a class is hidden, delete the whole class including decorators.
        class_ranges = []
        for class_name, entries in class_methods.items():
            if entries and all(h and not v for _, h, v in entries):
                class_node = next((n for n in ast.walk(module) if isinstance(n, ast.ClassDef)
                                   and tuple(_class_path(module, n)) == class_name), None)
                if class_node:
                    start = min([class_node.lineno, *[d.lineno for d in class_node.decorator_list]])
                    class_ranges.append((start, class_node.end_lineno))
                    for method, _h, _v in entries:
                        method_id = rel + "::" + "::".join((*class_name, method.name))
                        stripped.extend(i for i in hidden_ids if _without_param_suffix(i) == method_id)
        ranges = list(class_ranges)
        for node, ids in remove_nodes:
            if any(a <= node.lineno <= b for a, b in class_ranges):
                continue
            start = min([node.lineno, *[d.lineno for d in node.decorator_list]])
            ranges.append((start, node.end_lineno))
            stripped.extend(i for i in hidden_ids if _without_param_suffix(i) == rel + "::" + "::".join((*next((c for n, c in tests if n is node), ()), node.name)))
        for start, end in sorted(ranges, reverse=True):
            del lines[start - 1:end]
        source.write_text("".join(lines))
    return {"stripped": sorted(set(stripped)), "inseparable": sorted(set(inseparable))}


def _class_path(module: ast.Module, target: ast.ClassDef) -> tuple[str, ...]:
    result = []
    def visit(node, path=()):
        for child in getattr(node, "body", []):
            if isinstance(child, ast.ClassDef):
                if child is target:
                    result.extend(path + (child.name,))
                else:
                    visit(child, path + (child.name,))
    visit(module)
    return tuple(result)


def _frozen_dir(tree: Path) -> Path:
    """Frozen config lives inside the tree (pytest scans the -c file's directory), written after the
    candidate diff is applied and read before any candidate code runs."""
    path = tree / ".evolve"
    if path.exists():
        shutil.rmtree(path)
    path.mkdir()
    return path


def frozen_pytest_config(base_repo: str | Path, out_dir: str | Path) -> Path:
    """Copy the selected pristine pytest config, or create an empty pytest.ini."""
    base, out = Path(base_repo), Path(out_dir)
    choices = [("pytest.ini", True), ("pyproject.toml", "[tool.pytest.ini_options]"),
               ("tox.ini", "[pytest]"), ("setup.cfg", "[tool:pytest]")]
    for filename, marker in choices:
        candidate = base / filename
        if candidate.is_file() and (marker is True or marker in candidate.read_text()):
            target = out / filename
            shutil.copy2(candidate, target)
            return target
    target = out / "pytest.ini"
    target.write_text("[pytest]\n")
    return target


def _venv_python(venv: Path) -> Path:
    for name in ("python", "python3"):
        candidate = venv / "bin" / name
        if candidate.exists():
            return candidate
    return venv / "Scripts" / "python.exe"


def _python_runtime_paths(venv: Path) -> list[Path]:
    cfg = venv / "pyvenv.cfg"
    home = None
    if cfg.is_file():
        for line in cfg.read_text().splitlines():
            if line.lower().startswith("home ="):
                home = line.split("=", 1)[1].strip()
                break
    paths = []
    if home:
        raw = Path(os.path.abspath(home))
        resolved = Path(os.path.realpath(home))
        paths.extend([raw.parent, resolved, resolved.parent, Path(os.path.realpath(resolved / "python3")).parent])
    return list(dict.fromkeys(paths))


def enforce_required_ids(cases: dict[str, list[str]], node_ids: list[str]) -> dict:
    passed, failed, missing, duplicates = set(), set(), set(), set()
    for node in node_ids:
        states = cases.get(node, [])
        if len(states) != 1:
            (missing if not states else duplicates).add(node)
        elif states[0] == "pass":
            passed.add(node)
        else:
            failed.add(node)
    return {"passed": passed, "failed": failed, "missing": missing, "duplicates": duplicates}


def run_pytest_hardened(tree: str | Path, venv: str | Path, node_ids: list[str], pythonpath: list[str],
                        frozen_ini: str | Path, plugins: list[str], timeout: int, home: str | Path,
                        select: list[str] | None = None, extra_env: dict | None = None) -> HardenedResult:
    """Run pytest on `select` (default: `node_ids`) and report each required `node_ids` entry exactly once."""
    tree, venv = Path(tree).resolve(), Path(venv).resolve()
    guard_dir = Path(__file__).resolve().parent / "guard"
    with tempfile.TemporaryDirectory(prefix="evolve-pytest-") as tmp_name:
        tmp = Path(tmp_name)
        junit = tmp / "junit.xml"
        allow_reads = [venv, *_python_runtime_paths(venv), tree, Path(frozen_ini).resolve(), guard_dir, tmp]
        profile = pytest_profile(allow_reads, [tree, tmp], home)
        py_path = [guard_dir, *(tree / item for item in pythonpath)]
        env = {"PATH": f"{venv / 'bin'}:/usr/bin:/bin", "HOME": str(tmp), "TMPDIR": str(tmp),
               "PYTHONDONTWRITEBYTECODE": "1", "PYTHONPATH": os.pathsep.join(map(str, py_path)),
               "EVOLVE_VENV": str(venv), "LANG": "C.UTF-8"}
        env = {**(extra_env or {}), **env}  # repo profile env (e.g. QT_QPA_PLATFORM) never overrides ours
        pytest_args = ["-q", "-rfE", "--tb=short", "-c", str(frozen_ini),
                       "--rootdir", str(tree), "-p", "no:cacheprovider", "-p", "evolve_guard"]
        for plugin in plugins:
            pytest_args.extend(["-p", plugin])
        pytest_args.extend([f"--junitxml={junit}", *(select or node_ids)])
        # Import and pin pytest with no tree path on sys.path (no shadowing), then restore the repo's
        # explicit import roots (PYTHONPATH, which may include the tree root itself) for the tests.
        bootstrap = (
            "import os, sys\n"
            "tree = os.path.realpath(os.getcwd())\n"
            "inside = lambda p: os.path.realpath(p or '.') == tree or os.path.realpath(p or '.').startswith(tree + os.sep)\n"
            "explicit = [p for p in sys.path if p]\n"
            "sys.path[:] = [%r] + [p for p in explicit if not inside(p)]\n"
            "import pytest, _pytest\n"
            "venv = os.path.realpath(os.environ['EVOLVE_VENV'])\n"
            "for m in (pytest, _pytest):\n"
            "    if not os.path.realpath(m.__file__).startswith(venv + os.sep): raise SystemExit(97)\n"
            "sys.path[:] = [%r] + explicit\n"
            "raise SystemExit(pytest.main(%r))\n"
        ) % (str(guard_dir), str(guard_dir), pytest_args)
        args = [str(_venv_python(venv)), "-c", bootstrap]
        read_end, write_end = os.pipe()
        env["EVOLVE_REPORT_FD"] = str(write_end)
        chunks: list[bytes] = []
        reader = threading.Thread(target=lambda: chunks.extend(iter(lambda: os.read(read_end, 65536), b"")))
        reader.start()
        try:
            proc = run_sandboxed(args, profile, cwd=tree, env=env, capture_output=True, text=True, timeout=timeout,
                                 pass_fds=(write_end,))
            output = proc.stdout + proc.stderr
            code = proc.returncode
        except subprocess.TimeoutExpired as exc:
            code, output = -9, f"pytest timed out after {timeout}s\n"
            output += (exc.stdout.decode(errors="replace") if isinstance(exc.stdout, bytes) else (exc.stdout or ""))
        finally:
            os.close(write_end)
            reader.join(timeout=30)
            os.close(read_end)
        result = HardenedResult(code, output=output)
        streamed, issues = parse_report_stream(b"".join(chunks).decode(errors="replace"))
        result.report_issues = issues
        cases: dict[str, list[str]] = {}
        if junit.is_file():
            exists = lambda rel: (tree / rel).is_file()
            for case in ET.parse(junit).getroot().iter("testcase"):
                node = common.junit_case_to_node_id(case.get("classname", ""), case.get("name", ""), exists)
                if node is None:
                    continue
                children = list(case)
                outcome = "fail" if any(c.tag in ("failure", "error") for c in children) else (
                    "skip" if any(c.tag == "skipped" for c in children) else "pass")
                cases.setdefault(node, []).append(outcome)
                if children:
                    detail = "\n".join((c.get("message", "") + "\n" + (c.text or "")).strip() for c in children)
                    result.messages[node] = detail[:1500]
        exact = enforce_required_ids(cases, node_ids)
        # A pass needs junit AND the guard's live stream to agree; a tampered stream passes nothing.
        trusted = {node for node in exact["passed"] if streamed.get(node) == "pass"} if not issues else set()
        disagree = exact["passed"] - trusted
        if disagree:
            result.report_issues.append(f"{len(disagree)} junit passes not confirmed by the live report stream")
        result.passed = trusted
        result.failed = exact["failed"] | exact["duplicates"] | disagree
        result.missing, result.duplicates = exact["missing"], exact["duplicates"]
        return result


def parse_report_stream(text: str) -> tuple[dict[str, str], list[str]]:
    """Per-node outcome from the guard's records, and integrity issues (no END, records after END,
    a phase reported twice). Any issue means the stream, and so every pass, is untrusted."""
    phases: dict[str, dict[str, str]] = {}
    issues, ended = [], False
    for line in text.splitlines():
        try:
            record = json.loads(line)
        except ValueError:
            issues.append("unparseable report record")
            continue
        if record == ["END"]:
            if ended:
                issues.append("duplicate END")
            ended = True
            continue
        if ended:
            issues.append("report record after END")
            continue
        if not (isinstance(record, list) and len(record) == 3 and all(isinstance(x, str) for x in record)):
            issues.append("malformed report record")
            continue
        node, when, outcome = record
        if when in phases.setdefault(node, {}):
            issues.append(f"phase reported twice: {when}")
        phases[node][when] = outcome
    if not ended:
        issues.append("report stream has no END")
    outcomes = {}
    for node, seen in phases.items():
        if "failed" in seen.values():
            outcomes[node] = "fail"
        elif seen.get("call") == "passed":
            outcomes[node] = "pass"
        else:
            outcomes[node] = "skip"
    return outcomes, sorted(set(issues))


OPAQUE_ID = re.compile(r"e1-[0-9a-f]{12}")


def scrub(text: str, tree: Path) -> str:
    """Remove evaluator paths and task ids from text the policy will see (tracebacks name the venv)."""
    text = text.replace(str(tree), "<tree>").replace(os.path.realpath(tree), "<tree>")
    for home in {str(tasks.e1_home()), os.path.realpath(tasks.e1_home())}:
        text = text.replace(home, "<private>")
    return OPAQUE_ID.sub("<task>", text)


def verify_structured(opaque: str, diff: str) -> dict:
    manifest = tasks.load_manifest(opaque)
    profile = tasks.REPO_PROFILES[manifest["repo"]]
    base, priv = tasks.work_dir(opaque) / "repo", tasks.private_dir(opaque)
    with tempfile.TemporaryDirectory(prefix="evolve-verify-", dir=tasks.private_tmp()) as tmp:
        tree = Path(tmp) / "tree"
        try:
            grading.build_candidate_tree(base, tree, diff=diff)
            reasons = audit_diff(diff, profile["pythonpath"], protected_module_names(priv / "venv"))
            if reasons:
                return {"passed": False, "rejected": reasons, "tests": []}
            grading.apply_patch(tree, (priv / "test.patch").read_text())
            visible_ids, hidden_ids = manifest["visible_fail_to_pass"], manifest["hidden_fail_to_pass"]
            keep_ids = manifest["pass_to_pass"]
            stripped = strip_hidden_tests(tree, visible_ids, hidden_ids, keep_ids)
            node_ids = sorted(set(visible_ids) | set(keep_ids))
            ini = frozen_pytest_config(base, _frozen_dir(tree))
            result = run_pytest_hardened(tree, priv / "venv", node_ids, profile["pythonpath"], ini,
                                         [], 1200, Path.home(), extra_env=tasks.profile_env(manifest))
        except grading.CandidateError as exc:
            return {"passed": False, "rejected": [], "tests": [], "error": scrub(str(exc), tree)[:1500]}
        except OSError:
            return {"passed": False, "rejected": [], "tests": [], "error": "evaluator setup failed"}
        tests = []
        hidden_visible = set(stripped["inseparable"])
        for index, node in enumerate(node_ids):
            if node in hidden_visible:
                continue
            if node in result.duplicates:
                outcome = "missing"; message = "duplicate junit entries"
            elif node in result.missing:
                outcome = "missing"; message = "required test missing from junit"
            elif node in result.passed:
                outcome, message = "pass", ""
            else:
                outcome, message = "fail", result.messages.get(node, "test did not pass")[:1500]
            message = scrub(message, tree)
            tests.append({"index": index, "name": node, "outcome": outcome, "message": message[:1500]})
    return {"passed": bool(tests) and all(t["outcome"] == "pass" for t in tests), "rejected": [], "tests": tests}


def grade_hardened(opaque: str, diff: str | None) -> dict:
    manifest = tasks.load_manifest(opaque)
    profile = tasks.REPO_PROFILES[manifest["repo"]]
    base, priv = tasks.work_dir(opaque) / "repo", tasks.private_dir(opaque)
    if diff is None:
        diff = ""
    try:
        reasons = audit_diff(diff, profile["pythonpath"], protected_module_names(priv / "venv"))
    except OSError:
        return {"applied": False, "rejected": [], "resolved": False, "hidden_resolved": False,
                "error": "evaluator audit setup failed"}
    if reasons:
        return {"applied": True, "rejected": reasons, "resolved": False, "hidden_resolved": False}
    with tempfile.TemporaryDirectory(prefix="evolve-grade-", dir=tasks.private_tmp()) as tmp:
        tree = Path(tmp) / "tree"
        try:
            grading.build_candidate_tree(base, tree, diff=diff or None)
            grading.apply_patch(tree, (priv / "test.patch").read_text())
            node_ids = sorted(set(manifest["fail_to_pass"]) | set(manifest["pass_to_pass"]))
            ini = frozen_pytest_config(base, _frozen_dir(tree))
            # Official selection (whole selected files), as E1's grader ran it: node-id subsets can
            # change test order and expose order-dependent tests.
            result = run_pytest_hardened(tree, priv / "venv", node_ids, profile["pythonpath"], ini,
                                         [], 1200, Path.home(), extra_env=tasks.profile_env(manifest),
                                         select=manifest["selected_test_files"])
        except grading.CandidateError as exc:
            return {"applied": False, "rejected": [], "resolved": False, "hidden_resolved": False, "error": str(exc)}
    required = set(manifest["fail_to_pass"]) | set(manifest["pass_to_pass"])
    hidden_required = set(manifest["hidden_fail_to_pass"]) | set(manifest["pass_to_pass"])
    return {"applied": True, "rejected": [], "resolved": common.resolved(required, result.passed),
            "hidden_resolved": common.resolved(hidden_required, result.passed),
            "required": len(required), "required_passed": len(required & result.passed),
            "pytest_returncode": result.returncode}


def _gate(task: str) -> dict:
    manifest = tasks.prepare(task)
    gold = (tasks.private_dir(task) / "gold.patch").read_text()
    gold_result, empty_result = grade_hardened(task, gold), grade_hardened(task, "")
    result = {"opaque_id": task, "gold": gold_result, "empty": empty_result,
              "passed": bool(gold_result.get("resolved") and not empty_result.get("resolved"))}
    tasks.evict(task)
    return result


def _audit_gold(summary_path: str) -> None:
    summary = json.loads(Path(summary_path).read_text())
    for item in summary:
        if not item.get("passed"):
            continue
        opaque = item["opaque_id"]
        manifest = tasks.load_manifest(opaque)
        profile = tasks.REPO_PROFILES[manifest["repo"]]
        diff = (tasks.private_dir(opaque) / "gold.patch").read_text()
        reasons = audit_diff(diff, profile["pythonpath"], protected_module_names(tasks.private_dir(opaque) / "venv"))
        print(json.dumps({"opaque_id": opaque, "reasons": reasons}, sort_keys=True))


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    gate_parser = sub.add_parser("gate"); gate_parser.add_argument("--task", required=True)
    audit_parser = sub.add_parser("audit-gold"); audit_parser.add_argument("--gate-summary", required=True)
    args = parser.parse_args(argv)
    if args.command == "gate":
        print(json.dumps(_gate(args.task), sort_keys=True))
    else:
        _audit_gold(args.gate_summary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
