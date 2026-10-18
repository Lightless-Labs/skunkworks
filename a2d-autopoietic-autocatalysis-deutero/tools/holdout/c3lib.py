"""Shared pieces of the confirmation-3 blind authoring pipeline.

Plan: docs/plans/2026-10-18-confirmation-3.md (§0.4, §0.6, §0.10); review dispositions in
tools/holdout/README.md. Rules this module enforces mechanically:

- Output gateway: scripts print ONLY `emit(...)` lines. Keys are allowlisted; `id` values
  must be opaque confirmation-3 ids; other strings must be short code-defined tokens.
  Exceptions print only their class name.
- Every codex job runs inside an outer macOS seatbelt profile (`sandbox-exec`):
  file writes are denied everywhere except the job's workdir and xcrun's cache file;
  reads are denied under /Users, the Data-volume twin, /Volumes and the temp roots,
  except the workdir, the Rust toolchain, the codex binary dir and auth.json (read-only).
  /dev/tty is denied and Apple Events are denied. Codex's own sandbox is off inside it
  (macOS cannot nest seatbelt profiles). Network stays open (codex needs the API);
  disclosed.
- Each job has a fresh private CODEX_HOME inside its own workdir (auth.json only).
- The driver keeps everything it writes or later trusts (profiles, transcripts, exit
  codes, attempt records) in a records dir the job cannot reach; artifacts produced by a
  job are read only through symlink-refusing helpers, after its process group is dead.
- Launch isolation is checked before every job: ancestry free of agent/codex context
  files (those under $HOME must be unreadable in the profile), pinned codex binary hash
  and version, a positive control (a workdir file is readable in the profile) and denial
  probes (canary, repo, prior holdout sets, user codex config, ~/.agents) that must fail.
- Prompts are passed as a pointer ("read INSTRUCTIONS.md"), never in argv.
- Process groups are killed on timeout, exception or normal exit.
"""
import errno
import hashlib
import json
import os
import re
import signal
import stat
import subprocess
import sys
import time
from pathlib import Path

HOLDOUT = Path(os.environ.get("A2D_HOLDOUT_DIR", str(Path.home() / ".a2d-holdout"))).resolve()
C3 = HOLDOUT / "authoring" / "confirmation-3"
RECORDS = C3 / "_records"
SUITE = HOLDOUT / "confirmation-3"
REPO = Path(__file__).resolve().parents[2]
CODEX = os.environ.get("C3_CODEX", "/opt/homebrew/bin/codex")
PINNED_CODEX = os.environ.get("C3_PINNED_CODEX", "codex-cli 0.162.0")
PINNED_SHA = os.environ.get("C3_PINNED_SHA", "8238273076041e55")  # prefix of sha256(codex binary)
TOOLCHAIN = Path(os.environ.get("C3_TOOLCHAIN", str(Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin")))
REAL_AUTH = Path(os.environ.get("C3_AUTH", str(Path.home() / ".codex" / "auth.json")))
CANARY = Path(os.environ.get("C3_CANARY", str(Path.home() / ".a2d-canary.txt")))
AUTHORING_FILES = Path(__file__).resolve().parent / "authoring"

DISABLE = [
    # web search is off via -c web_search="disabled" (verified live 10-17); passing the
    # deprecated web_search_request flag only produces an `error` item in the stream.
    "browser_use", "browser_use_external",
    "browser_use_full_cdp_access", "browser_annotation_api", "computer_use", "apps", "plugins",
    "remote_plugin", "multi_agent", "image_generation", "in_app_browser", "skill_search",
    "skill_mcp_dependency_install", "tool_suggest", "js_repl", "goals", "hooks",
]
ALLOWED_ITEMS = {"agent_message", "reasoning", "command_execution", "file_change", "todo_list"}
ALLOWED_TOP = {"thread.started", "turn.started", "turn.completed", "item.started", "item.updated",
               "item.completed"}

ID_RE = re.compile(r"^c3-f\d{2}$")
TASK_RE = re.compile(r"^c3-f\d{2}-l[1-4]$")
_KEYS = {"stage", "id", "exit", "count", "ok", "n", "max_sim", "conflicts", "other_events",
         "status", "error", "pending", "done", "skipped", "missing", "families", "tasks", "round",
         "max_contain", "unparsed"}
_TOKEN = re.compile(r"^[a-z0-9][a-z0-9_.:=-]{0,31}$")


# ---------------------------------------------------------------- output gateway

def emit(**fields):
    """The only way these scripts print. Unknown keys and non-opaque values are redacted."""
    out = {}
    for k, v in fields.items():
        if k not in _KEYS:
            continue
        if isinstance(v, bool) or v is None:
            out[k] = v
        elif isinstance(v, (int, float)):
            out[k] = round(v, 3) if isinstance(v, float) else v
        elif k == "id":
            out[k] = v if isinstance(v, str) and (ID_RE.match(v) or TASK_RE.match(v)) else "redacted"
        elif isinstance(v, str) and _TOKEN.match(v):
            out[k] = v
        else:
            out[k] = "redacted"
    sys.stdout.write(json.dumps(out) + "\n")
    sys.stdout.flush()


def gateway(stage):
    """Decorator: run main(); any exception becomes a content-free error line."""
    def wrap(fn):
        def run(*a, **kw):
            try:
                return fn(*a, **kw)
            except SystemExit:
                raise
            except BaseException as e:  # noqa: BLE001 - deliberately content-free
                emit(stage=stage, status="error", error=type(e).__name__.lower())
                sys.exit(1)
        return run
    return wrap


def opaque_id(i):
    return f"c3-f{i:02d}"


def read_ids(path, expected=None):
    ids = [x.strip() for x in safe_read_text(path).splitlines() if x.strip()]
    if not ids or not all(ID_RE.match(i) for i in ids) or len(set(ids)) != len(ids):
        raise ValueError("bad id list")
    if expected is not None and len(ids) != expected:
        raise ValueError("wrong id count")
    return ids


# ------------------------------------------------------- symlink-refusing host I/O

def _check_chain(path, root):
    """path must be inside root and no component from root down may be a symlink."""
    path, root = Path(os.path.abspath(path)), Path(os.path.abspath(root))
    if path != root and root not in path.parents:
        raise PermissionError("outside root")
    cur = root
    for part in path.relative_to(root).parts:
        cur = cur / part
        try:
            if stat.S_ISLNK(os.lstat(cur).st_mode):
                raise PermissionError("symlink")
        except FileNotFoundError:
            return


def safe_read_bytes(path, root=None):
    _check_chain(path, root or HOLDOUT)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise PermissionError("not a regular file")
        with os.fdopen(fd, "rb") as f:
            fd = None
            return f.read()
    finally:
        if fd is not None:
            os.close(fd)


def safe_read_text(path, root=None):
    return safe_read_bytes(path, root).decode("utf-8", errors="replace")


def safe_write_text(path, text, root=None):
    """Create or replace a regular file without following symlinks."""
    _check_chain(path, root or HOLDOUT)
    p = Path(path)
    try:
        if stat.S_ISLNK(os.lstat(p).st_mode) or not stat.S_ISREG(os.lstat(p).st_mode):
            raise PermissionError("not a regular file")
        os.unlink(p)
    except FileNotFoundError:
        pass
    fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    with os.fdopen(fd, "w") as f:
        f.write(text)


def safe_write_bytes(path, data, root=None):
    _check_chain(path, root or HOLDOUT)
    p = Path(path)
    try:
        if stat.S_ISLNK(os.lstat(p).st_mode) or not stat.S_ISREG(os.lstat(p).st_mode):
            raise PermissionError("not a regular file")
        os.unlink(p)
    except FileNotFoundError:
        pass
    fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    with os.fdopen(fd, "wb") as f:
        f.write(data)


def safe_copy(src, dst, src_root, dst_root=None, mode=None):
    data = safe_read_bytes(src, src_root)
    _check_chain(dst, dst_root or HOLDOUT)
    Path(dst).parent.mkdir(parents=True, exist_ok=True)
    safe_write_bytes(dst, data, dst_root or HOLDOUT)
    if mode is not None:
        os.chmod(dst, mode)


# ------------------------------------------------------------- sandbox profile

def _rp(p):
    return str(Path(p).resolve())


def _user_temp_dir():
    try:
        out = subprocess.run(["/usr/bin/getconf", "DARWIN_USER_TEMP_DIR"], capture_output=True, text=True,
                             timeout=10).stdout.strip()
    except (OSError, subprocess.TimeoutExpired):
        return None
    return _rp(out) if out else None


def _q(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def profile(workdir):
    """SBPL profile for one job."""
    home = str(Path.home())
    w = _rp(workdir)
    codex_dir = _rp(Path(CODEX).resolve().parent)
    brokers = ["/bin/launchctl", "/usr/bin/open", "/usr/bin/osascript", "/usr/bin/crontab", "/usr/bin/at",
               "/usr/bin/batch", "/usr/bin/pbcopy", "/usr/bin/pbpaste", "/usr/bin/ssh", "/usr/bin/ssh-add",
               "/usr/bin/scp", "/usr/bin/sftp", "/usr/bin/sudo", "/usr/bin/su", "/usr/bin/security",
               "/usr/bin/defaults", "/usr/bin/screen", "/usr/bin/tmux"]
    lines = [
        "(version 1)",
        "(allow default)",
        "(deny appleevent-send)",
        # Signals and process inspection only within this sandbox (codex manages its own
        # children); outside processes (the driver, other sessions) are unreachable.
        "(deny signal)",
        "(allow signal (target same-sandbox))",
        "(deny process-info*)",
        "(allow process-info* (target same-sandbox))",
        "(deny process-exec " + " ".join(f"(literal {_q(b)})" for b in brokers) + ")",
        # Unix sockets under the user's dirs and temp roots (ssh-agent, launchd listeners,
        # other daemons) are closed; only sockets inside the workdir are allowed.
        "(deny network-outbound (remote unix-socket"
        f" (subpath {_q(home)}) (subpath \"/Users\") (subpath \"/private/tmp\") (subpath \"/private/var/folders\")))",
        f"(allow network-outbound (remote unix-socket (subpath {_q(w)})))",
        "(deny file-write*)",
        "(deny file-read*"
        f" (subpath {_q(home)}) (subpath {_q('/System/Volumes/Data' + home)}) (subpath \"/Users\")"
        f" (subpath \"/System/Volumes/Data/Users\") (subpath \"/Volumes\")"
        f" (subpath {_q(str(HOLDOUT))}) (subpath {_q(str(REPO))})"
        f" (subpath {_q(_rp(os.environ.get('TMPDIR', '/private/tmp')))})"
        ' (subpath "/private/tmp") (subpath "/private/var/folders") (subpath "/private/var/tmp"))',
        f"(allow file-read* file-write* (subpath {_q(w)}))",
        '(allow file-write* (literal "/dev/null") (literal "/dev/zero"))',
        f"(allow file-read* (literal {_q(_rp(REAL_AUTH))}))",
        f"(allow file-read* (subpath {_q(_rp(TOOLCHAIN))}) (subpath {_q(codex_dir)}))",
        "(allow file-read-metadata)",
        '(deny file-read* file-write* (regex #"^/dev/tty"))',
    ]
    # The linker runs through the xcrun shim, which caches toolchain lookups in
    # <user temp dir>/xcrun_db (resolved via confstr, not $TMPDIR). Without this anchored
    # rule every link re-resolves the toolchain and takes ~1 min (seatbelt oracle lesson,
    # 2026-10-17; same rule as crates/a2d-core/src/sandbox.rs).
    user_tmp = _user_temp_dir()
    if user_tmp:
        esc = re.sub(r'([\\.^$|?*+()\[\]{}"])', r"\\\1", user_tmp.rstrip("/"))
        lines.append(f'(allow file-read* file-write* (regex #"^{esc}/xcrun_db(-[A-Za-z0-9]+)?$"))')
    return "\n".join(lines) + "\n"


def record_dir(name):
    d = RECORDS / name
    d.mkdir(parents=True, exist_ok=True)
    return d


def job_codex_home(workdir):
    """Fresh private CODEX_HOME inside the job's workdir: auth.json symlink only."""
    home = Path(workdir) / "_codex_home"
    if home.exists() or home.is_symlink():
        if home.is_symlink() or not home.is_dir():
            raise PermissionError("codex home")
        for p in sorted(home.rglob("*"), reverse=True):
            if p.is_symlink() or p.is_file():
                p.unlink()
            elif p.is_dir():
                p.rmdir()
        home.rmdir()
    home.mkdir()
    (home / "auth.json").symlink_to(REAL_AUTH)
    return home


def _env(workdir, home):
    env = {"HOME": str(Path.home()), "PATH": "/usr/bin:/bin:/opt/homebrew/bin",
           "CODEX_HOME": str(home), "TMPDIR": _rp(workdir), "LANG": "en_US.UTF-8",
           "RUSTC": str(TOOLCHAIN / "bin" / "rustc")}
    env.update({k: v for k, v in os.environ.items() if k.startswith("C3_TEST_")})  # test hooks only
    return env


def sandboxed(workdir, argv, record=None, **kw):
    """Run argv under the job's profile with a cleared environment (no codex home needed)."""
    rec = record_dir(record or "_probe")
    prof = rec / f"probe-{os.getpid()}-{time.time_ns()}.sb"
    safe_write_text(prof, profile(workdir))
    env = _env(workdir, Path(workdir) / "_codex_home")
    return subprocess.run(["/usr/bin/sandbox-exec", "-f", str(prof), *argv], cwd=workdir, env=env,
                          stdin=subprocess.DEVNULL, **kw)


# ------------------------------------------------------------ isolation checks

AMBIENT = (".codex", "AGENTS.md", "AGENTS.override.md", ".agents", "skills")


def codex_pinned():
    try:
        v = subprocess.run([CODEX, "--version"], capture_output=True, text=True, timeout=30).stdout.strip()
        digest = hashlib.sha256(Path(CODEX).resolve().read_bytes()).hexdigest()
    except (OSError, subprocess.TimeoutExpired):
        return False
    return v == PINNED_CODEX and digest.startswith(PINNED_SHA)


def isolation_check(workdir):
    """True iff the job may launch. Never prints; callers emit the boolean."""
    w = Path(workdir).resolve()
    if C3 not in w.parents:
        return False
    home = Path.home().resolve()
    under_home = []
    for d in [w, *w.parents]:
        for bad in AMBIENT:
            p = d / bad
            if os.path.lexists(p):
                if home in p.parents or p.parent == home:
                    under_home.append(p)   # must be unreadable in the profile (probed below)
                else:
                    return False
    if any(os.path.lexists(w / "_codex_home" / f) for f in
           ("config.toml", "plugins", "skills", "memories", "AGENTS.md", "hooks.json", "prompts", "rules")):
        return False
    if not codex_pinned():
        return False
    return probes_ok(w, under_home)


def probes_ok(w, extra_denied=()):
    """Positive control must succeed; every denial probe must fail without leaking."""
    import threading
    tag = f"{os.getpid()}-{threading.get_ident()}-{time.time_ns()}"
    secret = f"C3-CANARY-{tag}"
    canary = CANARY.parent / f"{CANARY.name}.{tag}"   # one per check: parallel checks never collide
    safe_write_text(canary, secret + "\n", CANARY.parent)
    try:
        return _probes(w, canary, secret, extra_denied)
    finally:
        try:
            os.unlink(canary)
        except FileNotFoundError:
            pass


def _probes(w, canary, secret, extra_denied):
    ctl = w / f".c3-control-{secret[-24:]}"
    safe_write_text(ctl, "CONTROL-OK\n", w)
    r = sandboxed(w, ["/bin/cat", str(ctl)], capture_output=True, text=True, timeout=60)
    if r.returncode != 0 or "CONTROL-OK" not in r.stdout:
        return False
    ctl.unlink()
    denied = [canary, REPO / "README.md", HOLDOUT / "confirmation-2", HOLDOUT / "dev-holdout",
              Path.home() / ".codex" / "config.toml", *extra_denied]
    for target in denied:
        if not os.path.lexists(target):
            continue
        argv = ["/bin/ls", str(target)] if Path(target).is_dir() else ["/bin/cat", str(target)]
        r = sandboxed(w, argv, capture_output=True, text=True, timeout=60)
        if r.returncode == 0 or secret in r.stdout + r.stderr:
            return False
    # writes outside the workdir must fail too
    r = sandboxed(w, ["/usr/bin/touch", str(C3 / f".c3-write-probe-{os.getpid()}")],
                  capture_output=True, text=True, timeout=60)
    if r.returncode == 0:
        return False
    return True


def disable_args():
    out = []
    for f in DISABLE:
        out += ["--disable", f]
    return out


# ------------------------------------------------------------------ job runner

def _kill_group(p):
    try:
        os.killpg(p.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass


def run_codex(workdir, record, model="gpt-6.1-sol", effort="high", timeout=7200):
    """Run one pointer-prompted codex job in workdir under the outer sandbox.

    `record` names the driver-owned records dir (outside the workdir) that receives the
    profile, transcript, stderr and exit code. Returns (exit_code, other_event_count);
    the driver never reads model text, only event types.
    """
    w = Path(workdir).resolve()
    rec = record_dir(record)
    home = job_codex_home(w)
    prof = rec / "job.sb"
    safe_write_text(prof, profile(w))
    argv = [CODEX, "exec", "-m", model, "-c", f'model_reasoning_effort="{effort}"',
            "-c", 'web_search="disabled"', "-c", 'approval_policy="never"',
            "--sandbox", "danger-full-access", "--skip-git-repo-check", "--json",
            *disable_args(), "Read INSTRUCTIONS.md in the current directory and follow it."]
    code = None
    with open(rec / "codex.jsonl", "wb") as out, open(rec / "codex.err", "wb") as err:
        p = subprocess.Popen(["/usr/bin/sandbox-exec", "-f", str(prof), *argv], cwd=w, env=_env(w, home),
                             stdin=subprocess.DEVNULL, stdout=out, stderr=err, start_new_session=True)
        with open(RECORDS / "pids.txt", "a") as f:   # process-group ids (new session)
            f.write(f"{p.pid}\n")
        try:
            code = p.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            code = 124
        finally:
            _kill_group(p)
            p.wait()
    safe_write_text(rec / "exit.txt", f"{code}\n")
    return code, count_other_events(rec / "codex.jsonl")


def count_other_events(jsonl):
    """Count events outside the allowlist (unknown types, disallowed or malformed items,
    unparsable lines). A missing or empty transcript counts as 1 (fail closed)."""
    try:
        lines = [x for x in safe_read_text(jsonl).splitlines() if x.strip()]
    except OSError:
        return 1
    if not lines:
        return 1
    other = 0
    for line in lines:
        try:
            e = json.loads(line)
        except ValueError:
            other += 1
            continue
        t = e.get("type") if isinstance(e, dict) else None
        if t not in ALLOWED_TOP:
            other += 1
            continue
        if t.startswith("item."):
            item = e.get("item")
            if not isinstance(item, dict) or item.get("type") not in ALLOWED_ITEMS:
                other += 1
    return other


def exit_code(record):
    try:
        return int(safe_read_text(RECORDS / record / "exit.txt").strip())
    except (OSError, ValueError):
        return None


# ------------------------------------------------------------- state and gates

def load_json(path, default=None):
    try:
        return json.loads(safe_read_text(path))
    except FileNotFoundError:
        return default


def save_json(path, obj):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    safe_write_text(path, json.dumps(obj, indent=1, sort_keys=True) + "\n")


def _git(*args):
    return subprocess.run(["git", "-C", str(REPO), *args], capture_output=True, text=True, timeout=600)


def prereqs_ok():
    """§0.6 gates checked before any selection/authoring job:
    (a) confirmation-2 replicate 2's web audit is committed at HEAD (its generation and audit finished);
    (b) the pipeline and the confirmation-3 plan are unmodified relative to HEAD (frozen by commit).
    Records the pipeline commit. Tests may skip this only against a non-default holdout dir."""
    if os.environ.get("C3_TEST_SKIP_PREREQS") and HOLDOUT != (Path.home() / ".a2d-holdout").resolve():
        return True
    if _git("ls-files", "--error-unmatch", "runs/c2-confirm2/web-audit-r2.json").returncode != 0:
        return False
    if _git("diff", "--quiet", "HEAD", "--", "tools/holdout", "docs/plans/2026-10-18-confirmation-3.md").returncode != 0:
        return False
    head = _git("rev-parse", "HEAD").stdout.strip()
    save_json(RECORDS / "pipeline.json", {"commit": head, "checked_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})
    return bool(head)


class DriverLock:
    """One driver at a time; its pid is recorded so drained() can see it."""

    def __init__(self):
        self.path = RECORDS / "driver.pid"

    def __enter__(self):
        RECORDS.mkdir(parents=True, exist_ok=True)
        try:
            pid = int(safe_read_text(self.path).strip())
            os.kill(pid, 0)
            raise RuntimeError("driver already running")
        except (FileNotFoundError, ValueError, ProcessLookupError):
            pass
        safe_write_text(self.path, f"{os.getpid()}\n")
        if not (RECORDS / "pids.txt").exists():
            safe_write_text(RECORDS / "pids.txt", "")
        return self

    def __exit__(self, *exc):
        try:
            os.unlink(self.path)
        except FileNotFoundError:
            pass
        return False


def stop_requested():
    return os.path.lexists(C3 / "STOP")


def drained():
    """§0.10 drain: STOP requested, no live driver, no live job process group."""
    if not stop_requested():
        return False
    try:
        pid = int(safe_read_text(RECORDS / "driver.pid").strip())
        os.kill(pid, 0)
        return False
    except (FileNotFoundError, ValueError, ProcessLookupError):
        pass
    try:
        pids = [int(x) for x in safe_read_text(RECORDS / "pids.txt").split()]
    except (FileNotFoundError, ValueError):
        return False
    for pgid in pids:
        try:
            os.killpg(pgid, 0)
            return False
        except ProcessLookupError:
            continue
        except PermissionError:
            return False
    return True


def task_files(root, exclude=()):
    """Every task.json under root, following symlinked dirs without cycles."""
    seen_dirs, seen_files, out = set(), set(), []
    for dirpath, dirs, files in os.walk(root, followlinks=True):
        real = os.path.realpath(dirpath)
        if real in seen_dirs or any(x in dirpath for x in exclude):
            dirs[:] = []
            continue
        seen_dirs.add(real)
        if "task.json" in files:
            p = os.path.realpath(os.path.join(dirpath, "task.json"))
            if p not in seen_files:
                seen_files.add(p)
                out.append(Path(dirpath) / "task.json")
    return sorted(out)


def prior_roots():
    """Every prior task set: the repo's tasks/ and everything under the holdout dir except
    confirmation-3 (this includes authoring history such as excluded families)."""
    return [("repo-tasks", REPO / "tasks", ()),
            ("holdout", HOLDOUT, ("/confirmation-3", "/_records"))]


EXPECTED_HOLDOUT_SETS = ("confirmation-1", "confirmation-2", "dev-holdout", "c1-validation")


def prior_tasks():
    """[(family, requirements)] for every prior task; fails if a root or an expected set is
    empty or a record is malformed. Returns also per-root counts (opaque)."""
    rows, counts = [], {}
    for name, root, excl in prior_roots():
        files = task_files(root, excl)
        counts[name] = len(files)
        for tj in files:
            d = json.loads(Path(tj).read_text())
            fam, req = d.get("family"), d.get("requirements")
            if not isinstance(fam, str) or not fam or not isinstance(req, str) or not req.strip():
                raise ValueError("malformed prior task")
            rows.append((fam, req))
    if counts.get("repo-tasks", 0) == 0:
        raise ValueError("empty repo root")
    if os.environ.get("C3_TEST_SKIP_PREREQS") is None:
        for s in EXPECTED_HOLDOUT_SETS:
            if not task_files(HOLDOUT / s):
                raise ValueError("missing prior set")
    return rows, counts


def replace_family(fid, reason):
    """§0.11: an excluded family is replaced by the next unused spare (in id order) while
    spares remain; otherwise it is dropped. Deterministic; recorded in active.json."""
    act = load_json(RECORDS / "active.json")
    if fid not in act["active"]:
        return None
    spare = act["spares"].pop(0) if act["spares"] else None
    i = act["active"].index(fid)
    if spare:
        act["active"][i] = spare
    else:
        act["active"].pop(i)
    act["events"].append({"family": fid, "reason": reason, "replacement": spare})
    save_json(RECORDS / "active.json", act)
    return spare
