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
DEFAULT_HOLDOUT = (Path.home() / ".a2d-holdout").resolve()
# Test hooks (C3_* overrides of the codex binary, pins, auth, canary) are honoured only
# against a non-default holdout dir; production always uses the pinned values below.
TESTING = HOLDOUT != DEFAULT_HOLDOUT and bool(os.environ.get("C3_TEST_SKIP_PREREQS"))


def _cfg(name, default):
    return os.environ.get(name, default) if TESTING else default
C3 = HOLDOUT / "authoring" / "confirmation-3"
RECORDS = C3 / "_records"
SUITE = HOLDOUT / "confirmation-3"
REPO = Path(__file__).resolve().parents[2]
CODEX = _cfg("C3_CODEX", "/opt/homebrew/bin/codex")
PINNED_CODEX = _cfg("C3_PINNED_CODEX", "codex-cli 0.162.0")
PINNED_SHA = _cfg("C3_PINNED_SHA", "8238273076041e55")  # prefix of sha256(codex binary)
TOOLCHAIN = Path(_cfg("C3_TOOLCHAIN", str(Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin")))
REAL_AUTH = Path(_cfg("C3_AUTH", str(Path.home() / ".codex" / "auth.json")))
CANARY = Path(_cfg("C3_CANARY", str(Path.home() / ".a2d-canary.txt")))
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


def _read_fd(fd):
    """Read a regular file from an open descriptor (closes it)."""
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise PermissionError("not a regular file")
        with os.fdopen(fd, "rb") as f:
            fd = None
            return f.read()
    finally:
        if fd is not None:
            os.close(fd)


_DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
_FILE_FLAGS = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK          # a FIFO must not block


def safe_read_bytes(path, root=None):
    """Descriptor-relative read: every component below root is opened with O_NOFOLLOW
    relative to its parent's descriptor, so no component can be swapped for a symlink."""
    root = Path(os.path.abspath(root or HOLDOUT))
    path = Path(os.path.abspath(path))
    if path == root or root not in path.parents:
        raise PermissionError("outside root")
    parts = path.relative_to(root).parts
    dfd = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in parts[:-1]:
            nfd = os.open(part, _DIR_FLAGS, dir_fd=dfd)
            os.close(dfd)
            dfd = nfd
        return _read_fd(os.open(parts[-1], _FILE_FLAGS, dir_fd=dfd))
    except OSError as e:
        if e.errno in (errno.ELOOP, errno.ENOTDIR):
            raise PermissionError("symlink") from e
        raise
    finally:
        os.close(dfd)


def safe_read_text(path, root=None):
    return safe_read_bytes(path, root).decode("utf-8", errors="replace")


def safe_write_text(path, text, root=None):
    safe_write_bytes(path, text.encode("utf-8"), root)


def safe_write_bytes(path, data, root=None):
    """Atomic, durable create-or-replace that never follows a symlink: write a fresh temp
    file in the same dir (O_EXCL|O_NOFOLLOW), fsync, then rename over the target (rename
    replaces a symlink itself, never its target)."""
    _check_chain(path, root or HOLDOUT)
    p = Path(path)
    if os.path.lexists(p) and not stat.S_ISREG(os.lstat(p).st_mode) and not stat.S_ISLNK(os.lstat(p).st_mode):
        raise PermissionError("not a regular file")
    tmp = p.parent / f".{p.name}.tmp-{os.getpid()}-{time.time_ns()}"
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    try:
        with os.fdopen(fd, "wb") as f:
            f.write(data)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, p)
        dfd = os.open(p.parent, os.O_RDONLY)
        try:
            os.fsync(dfd)
        finally:
            os.close(dfd)
    except BaseException:
        try:
            os.unlink(tmp)
        except FileNotFoundError:
            pass
        raise


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


# Harvested 2026-10-18 with (allow mach-lookup (with report)) from a sandboxed compile+link
# +test run and a live codex call; only these are reachable from a job.
MACH_ALLOW = ("com.apple.logd", "com.apple.system.notification_center", "com.apple.system.opendirectoryd.libinfo",
              "com.apple.system.opendirectoryd.membership", "com.apple.bsd.dirhelper", "com.apple.trustd",
              "com.apple.trustd.agent", "com.apple.SystemConfiguration.configd",
              "com.apple.SecurityServer")   # codex loads TLS roots through it; keychain files stay unreadable


def profile(workdir, tag):
    """SBPL profile for one job. `tag` is an empty driver-owned file only this profile may
    read: it identifies the job's processes to the host (see job_members)."""
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
        # Mach services are an allowlist: copied broker binaries (open, launchctl, security,
        # mdfind, pbpaste, ...) bypass the exec denies above but need LaunchServices, the
        # keychain, Spotlight or the pasteboard, which stay unreachable (probed live).
        "(deny mach-lookup)",
        "(allow mach-lookup " + " ".join(f"(global-name {_q(n)})" for n in MACH_ALLOW) + ")",
        # Local TCP services (dev servers, browser debug ports, local MCP/HTTP brokers) are
        # closed; the remote API stays reachable.
        '(deny network-outbound (remote ip "localhost:*"))',
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
        f"(allow file-read* (literal {_q(_rp(REAL_AUTH))}) (literal {_q(_rp(tag))}))",
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
    """Run argv under a fresh tagged profile with a cleared environment, then reap every
    process left in that sandbox (fails closed if one survives)."""
    import threading
    rec = record_dir(record or "_probe")
    name = f"probe-{os.getpid()}-{threading.get_ident()}-{time.time_ns()}"
    tag = new_tag(name)
    prof = rec / f"{name}.sb"
    safe_write_text(prof, profile(workdir, tag))
    env = _env(workdir, Path(workdir) / "_codex_home")
    pipe = subprocess.PIPE if kw.get("capture_output") else None
    p = None
    try:
        with _Gate():                                 # registered like a job (no STOP check: probes only)
            p = _handshake_popen(prof, argv, cwd=workdir, env=env, stdout=pipe, stderr=pipe,
                                 text=kw.get("text", False))
        try:
            out, err = p.communicate(timeout=kw.get("timeout"))
        except subprocess.TimeoutExpired:
            _kill_group(p)
            p.wait()
            raise
        return subprocess.CompletedProcess(argv, p.returncode, out, err)
    finally:
        if p is not None:
            _kill_group(p)                            # the launcher's group, then its sandbox
        if not reap(tag):
            raise RuntimeError("probe not quiesced")
        retire_tag(tag)


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
        if r.returncode == 0 or secret in r.stdout + r.stderr or "Operation not permitted" not in r.stderr:
            return False
    # writes outside the workdir must fail too
    r = sandboxed(w, ["/usr/bin/touch", str(C3 / f".c3-write-probe-{os.getpid()}")],
                  capture_output=True, text=True, timeout=60)
    if r.returncode == 0 or "Operation not permitted" not in r.stderr:
        return False
    return _brokers_closed(w, secret)


BROKER_COPIES = ("/usr/bin/open", "/bin/launchctl", "/usr/bin/pbpaste", "/usr/bin/mdfind")


def _brokers_closed(w, secret):
    """Copies of broker binaries (which bypass the exec denies) must not reach their
    services, and a local TCP listener must be unreachable. Each case has its own
    expected-failure test (they do not all fail with EPERM)."""
    import socket
    w = Path(w)
    pdir = w / f".c3-brokers-{secret[-24:]}"
    pdir.mkdir()
    srv = socket.socket()
    srv.bind(("127.0.0.1", 0))
    srv.listen(1)
    srv.setblocking(False)
    try:
        x = {}
        for b in BROKER_COPIES:
            x[os.path.basename(b)] = pdir / f"x{os.path.basename(b)}"
            safe_copy(b, x[os.path.basename(b)], "/", w, mode=0o755)
        port = srv.getsockname()[1]
        cases = [
            ([x["open"], "-g", "-j", "/System/Applications/Calculator.app"], lambda r: r.returncode != 0),
            ([x["launchctl"], "list"], lambda r: r.returncode != 0 and not r.stdout.strip()),
            ([x["pbpaste"]], lambda r: r.returncode != 0),
            ([x["mdfind"], "-onlyin", "/Applications", "-name", "Safari"], lambda r: not r.stdout.strip()),
            (["/usr/bin/curl", "-s", "-m", "5", f"http://127.0.0.1:{port}/"], lambda r: r.returncode == 7),
        ]
        for argv, denied in cases:
            try:
                r = sandboxed(w, [str(a) for a in argv], capture_output=True, text=True, timeout=60)
            except subprocess.TimeoutExpired:
                return False
            if not denied(r):
                return False
        try:
            srv.accept()
            return False                     # something reached the listener
        except BlockingIOError:
            return True
    finally:
        srv.close()
        for f in pdir.iterdir():
            f.unlink()
        pdir.rmdir()


def disable_args():
    out = []
    for f in DISABLE:
        out += ["--disable", f]
    return out


# ------------------------------------------------------------ job quiescence
#
# A job's processes are identified by their sandbox, not by process group, session,
# environment or ancestry (all of which a job can shed): each profile alone may read its
# empty tag file under records/tags/, which every other profile denies (it is inside the
# holdout dir) and unsandboxed processes can read. sandbox_check(pid, "file-read-data",
# tag) == allowed, while records/tags/_none is denied, means the pid is in that sandbox.

TAGS = RECORDS / "tags"
_FILTER_PATH, _NO_REPORT = 1, 0x40000000
_lib = None


def _libsystem():
    global _lib
    if _lib is None:
        import ctypes
        lib = ctypes.CDLL("/usr/lib/libSystem.B.dylib", use_errno=True)
        lib.sandbox_check.restype = ctypes.c_int
        lib.sandbox_check.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int]  # path is variadic
        lib.proc_listallpids.restype = ctypes.c_int
        lib.proc_listallpids.argtypes = [ctypes.c_void_p, ctypes.c_int]
        lib.proc_pidinfo.restype = ctypes.c_int
        lib.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_int]
        _lib = lib
    return _lib


def _all_pids():
    import ctypes
    lib = _libsystem()
    n = lib.proc_listallpids(None, 0)
    buf = (ctypes.c_int * (n + 512))()
    n = lib.proc_listallpids(buf, ctypes.sizeof(buf))
    if n <= 0:
        raise OSError("proc_listallpids")
    return [buf[i] for i in range(n) if buf[i] > 1 and buf[i] != os.getpid()]


def _proc_start(pid):
    """Process start time in microseconds (libproc PROC_PIDTBSDINFO), or None if gone."""
    import ctypes

    class BsdInfo(ctypes.Structure):
        _fields_ = [("u32", ctypes.c_uint32 * 12), ("comm", ctypes.c_char * 16), ("name", ctypes.c_char * 32),
                    ("u32b", ctypes.c_uint32 * 5), ("nice", ctypes.c_int32), ("start_sec", ctypes.c_uint64),
                    ("start_usec", ctypes.c_uint64)]
    lib = _libsystem()
    info = BsdInfo()
    n = lib.proc_pidinfo(pid, 3, ctypes.c_uint64(0), ctypes.byref(info), ctypes.sizeof(info))
    if n != ctypes.sizeof(info):
        return None
    return info.start_sec * 1_000_000 + info.start_usec


def _recorded_groups():
    """(pgid, start) of every job launched by a driver since the last clean recovery."""
    try:
        lines = safe_read_text(RECORDS / "pids.txt").splitlines()
    except FileNotFoundError:
        return []
    out = []
    for line in lines:
        if line.strip():
            pgid, start = line.split()
            out.append((int(pgid), int(start)))
    return out


def _group_live(pgid, start):
    """True iff the recorded job group may still have members. A live leader with another
    start time means the id was reused: our group is gone (never signal it)."""
    leader = _proc_start(pgid)
    if leader is not None and leader != start:
        return False
    try:
        os.killpg(pgid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def kill_recorded_groups(deadline=30):
    """Kill every recorded launcher group still alive; True once none is left."""
    end = time.time() + deadline
    while True:
        live = [(g, s) for g, s in _recorded_groups() if _group_live(g, s)]
        if not live:
            return True
        if time.time() > end:
            return False
        for pgid, _ in live:
            try:
                os.killpg(pgid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
        time.sleep(0.2)                     # killed members are reaped by launchd


def _may_read(pid, path):
    import ctypes
    return _libsystem().sandbox_check(pid, b"file-read-data", _FILTER_PATH | _NO_REPORT,
                                      ctypes.c_char_p(str(path).encode())) == 0


def _member(pid, tag):
    return _libsystem().sandbox_check(pid, None, 0) == 1 and _may_read(pid, tag) \
        and not _may_read(pid, TAGS / "_none")


def new_tag(name):
    TAGS.mkdir(parents=True, exist_ok=True)
    if not (TAGS / "_none").exists():
        safe_write_text(TAGS / "_none", "")
    tag = TAGS / f"{name}.tag"
    safe_write_text(tag, "")
    return tag


def retire_tag(tag):
    try:
        os.unlink(tag)
    except FileNotFoundError:
        pass


def job_members(tag):
    if not os.path.exists(tag):
        raise FileNotFoundError("tag")
    return [p for p in _all_pids() if _member(p, tag)]


def reap(tag, deadline=60):
    """Stop every member of the tag's sandbox to a fixpoint (stopped processes cannot
    fork), kill them, repeat until none is left. A stopped pid that no longer belongs to
    the sandbox (pid reuse) is continued, never killed. True iff nothing survives."""
    end = time.time() + deadline
    while time.time() < end:
        stopped = set()
        while True:
            new = [p for p in job_members(tag) if p not in stopped]
            if not new:
                break
            for p in new:
                try:
                    os.kill(p, signal.SIGSTOP)
                except ProcessLookupError:
                    pass
                stopped.add(p)
        if not stopped:
            return True
        for p in stopped:
            try:
                os.kill(p, signal.SIGKILL if _member(p, tag) else signal.SIGCONT)
            except ProcessLookupError:
                pass
        time.sleep(0.2)
    return not job_members(tag)


def reap_all():
    """Driver-death recovery: reap every job whose tag was never retired."""
    if not TAGS.exists():
        return True
    return all(reap(t) for t in sorted(TAGS.glob("*.tag")))


# ------------------------------------------------------------------ job runner

def _kill_group(p):
    try:
        os.killpg(p.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass


class StopRequested(Exception):
    """STOP existed when the launch gate was taken; nothing was launched."""


class _Gate:
    """One exclusive lock (records/launch.lock) serialises STOP creation and every job
    launch, so no job starts after STOP exists (closes the check-to-launch race)."""

    def __enter__(self):
        import fcntl
        RECORDS.mkdir(parents=True, exist_ok=True)
        self.fd = os.open(RECORDS / "launch.lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o644)
        fcntl.flock(self.fd, fcntl.LOCK_EX)
        return self

    def __exit__(self, *exc):
        import fcntl
        fcntl.flock(self.fd, fcntl.LOCK_UN)
        os.close(self.fd)
        return False


def request_stop():
    with _Gate():
        safe_write_text(C3 / "STOP", "stop\n")


# The launcher reads "go" from its stdin (a driver pipe) before it execs sandbox-exec with
# stdin from /dev/null, so it never runs anything before the driver has durably recorded its
# pid and start time. If the driver dies first, the pipe hits EOF and the launcher exits
# without running anything.
_HANDSHAKE = 'IFS= read -r go && [ "$go" = go ] && exec /usr/bin/sandbox-exec -f "$0" "$@" </dev/null; exit 125'


def _handshake_popen(prof, argv, **kw):
    """Every launcher (jobs and probes alike) runs in its own session (pgid == pid) and is
    released only after its pid and start time are fsync'd to pids.txt. Callers hold _Gate."""
    kw.pop("stdin", None)
    r, w = os.pipe()
    try:
        p = subprocess.Popen(["/bin/sh", "-c", _HANDSHAKE, str(prof), *[str(a) for a in argv]], stdin=r,
                             start_new_session=True, **kw)
        os.close(r)
        r = None
        start = _proc_start(p.pid)
        if start is None:
            raise RuntimeError("launcher vanished")
        with open(RECORDS / "pids.txt", "a") as f:
            f.write(f"{p.pid} {start}\n")
            f.flush()
            os.fsync(f.fileno())
        os.write(w, b"go\n")
        return p
    finally:
        if r is not None:
            os.close(r)
        os.close(w)


def launch(prof, argv, **kw):
    """Start one job through the launch gate (refused once STOP exists) and the handshake."""
    with _Gate():
        if stop_requested():
            raise StopRequested()
        return _handshake_popen(prof, argv, **kw)


def run_codex(workdir, record, model="gpt-6.1-sol", effort="high", timeout=7200):
    """Run one pointer-prompted codex job in workdir under the outer sandbox.

    `record` names the driver-owned records dir (outside the workdir) that receives the
    profile, transcript, stderr and exit code. Returns (exit_code, other_event_count);
    the driver never reads model text, only event types.
    """
    w = Path(workdir).resolve()
    rec = record_dir(record)
    home = job_codex_home(w)
    tag = new_tag(record)
    prof = rec / "job.sb"
    safe_write_text(prof, profile(w, tag))
    argv = [CODEX, "exec", "-m", model, "-c", f'model_reasoning_effort="{effort}"',
            "-c", 'web_search="disabled"', "-c", 'approval_policy="never"',
            "--sandbox", "danger-full-access", "--skip-git-repo-check", "--json",
            *disable_args(), "Read INSTRUCTIONS.md in the current directory and follow it."]
    code = None
    with open(rec / "codex.jsonl", "wb") as out, open(rec / "codex.err", "wb") as err:
        p = launch(prof, argv, cwd=w, env=_env(w, home), stdin=subprocess.DEVNULL, stdout=out, stderr=err)
        try:
            code = p.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            code = 124
        finally:
            _kill_group(p)
            p.wait()
            # setsid/double-forked descendants survive killpg: every process still in this
            # job's sandbox is stopped and killed before the driver touches any artifact.
            if not reap(tag):
                raise RuntimeError("job not quiesced")
    retire_tag(tag)
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


PIPELINE_PATHS = ("tools/holdout", "docs/plans/2026-10-18-confirmation-3.md")


def prereqs_ok():
    """§0.6 gates checked before any selection/authoring job:
    (a) confirmation-2 replicate 2's count-only web audit is committed at HEAD and is a
        complete replicate-2 record (its generation and audit finished);
    (b) the pipeline and the confirmation-3 plan have no modified or untracked files;
    records the commit and a launch manifest (sha256 of every pipeline file at HEAD)."""
    if TESTING:
        return True
    r = _git("show", "HEAD:a2d-autopoietic-autocatalysis-deutero/runs/c2-confirm2/web-audit-r2.json")
    if r.returncode != 0:
        return False
    try:
        audit = json.loads(r.stdout)
    except ValueError:
        return False
    if audit.get("rep") != 2 or set(audit.get("arms", {})) != {"B", "D", "X-frozen"}:
        return False
    if _git("status", "--porcelain", "--", *PIPELINE_PATHS).stdout.strip():
        return False
    head = _git("rev-parse", "HEAD").stdout.strip()
    files = _git("ls-files", "--", *PIPELINE_PATHS).stdout.split()
    manifest = {f: hashlib.sha256((REPO / f).read_bytes()).hexdigest() for f in files}
    save_json(RECORDS / "pipeline.json", {"commit": head, "files": manifest,
                                          "checked_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})
    return bool(head) and bool(manifest)


PROVIDER_COMMIT = "bc6922f7"   # c2-web-provider, reviewed head (early reviews 2026-10-19): isolated CODEX_HOME,
#                               tools off, --json allowlist, failed-call transcripts, A2D_CODEX_HOME_ROOT, controls


def provider_merged():
    """The audit runs on the fixed provider only (plan §0.6 step 3 merge, then re-freeze)."""
    if TESTING:
        return True
    return _git("merge-base", "--is-ancestor", PROVIDER_COMMIT, "HEAD").returncode == 0


class DriverLock:
    """One driver at a time (fcntl.flock held for the driver's lifetime); its pid is
    recorded so drained() can see it."""

    def __enter__(self):
        import fcntl
        RECORDS.mkdir(parents=True, exist_ok=True)
        self.fd = os.open(RECORDS / "driver.lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o644)
        try:
            fcntl.flock(self.fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            os.close(self.fd)
            raise RuntimeError("driver already running")
        safe_write_text(RECORDS / "driver.pid", f"{os.getpid()}\n")
        # Recovery from a driver that died with live jobs: first every recorded launcher
        # group (which covers a launcher not yet inside its sandbox), then every member of
        # an unretired job sandbox. Only then may anything touch an artifact.
        if not kill_recorded_groups() or not reap_all():
            self.__exit__(None, None, None)
            raise RuntimeError("unquiesced job")
        safe_write_text(RECORDS / "pids.txt", "")
        return self

    def __exit__(self, *exc):
        import fcntl
        try:
            os.unlink(RECORDS / "driver.pid")
        except FileNotFoundError:
            pass
        fcntl.flock(self.fd, fcntl.LOCK_UN)
        os.close(self.fd)
        return False


def stop_requested():
    return os.path.lexists(C3 / "STOP")


def relocation_evidenced():
    """§0.10's positive check: every family that used an authoring attempt has session files
    in its relocated CODEX_HOME (inside the holdout dir), so the relocation took effect.
    ~/.codex is never read (the A² session also writes there)."""
    for fid in (load_json(RECORDS / "active.json", {}).get("active") or []):
        if load_json(RECORDS / f"author-{fid}" / "status.json", {"attempts": 0})["attempts"] < 1:
            continue
        sessions = C3 / fid / "_codex_home" / "sessions"
        if not sessions.is_dir() or not any(p.is_file() for p in sessions.rglob("*")):
            return False
    return True


def drained():
    """§0.10 drain: STOP requested, no live driver, no live job process group, and the
    relocated CODEX_HOMEs hold the authoring sessions."""
    if not stop_requested():
        return False
    try:
        pid = int(safe_read_text(RECORDS / "driver.pid").strip())
        os.kill(pid, 0)
        return False
    except (FileNotFoundError, ValueError, ProcessLookupError):
        pass
    if not (RECORDS / "pids.txt").exists():
        return False
    try:
        if any(_group_live(g, s) for g, s in _recorded_groups()):
            return False
    except ValueError:
        return False
    try:
        if TAGS.exists() and any(job_members(t) for t in TAGS.glob("*.tag")):
            return False
    except OSError:
        return False
    return relocation_evidenced()


def _raise(e):
    raise e


def task_files(root, exclude=()):
    """Every task.json under root, following symlinked dirs without cycles. Any traversal
    error (unreadable dir, vanished entry) raises: an inventory must never be partial."""
    seen_dirs, seen_files, out = set(), set(), []
    for dirpath, dirs, files in os.walk(root, followlinks=True, onerror=_raise):
        real = os.path.realpath(dirpath)
        if real in seen_dirs:
            dirs[:] = []
            continue
        dirs[:] = [d for d in dirs if not any(x in os.path.join(dirpath, d) + "/" for x in exclude)]
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
            ("holdout", HOLDOUT, ("/confirmation-3", "/_records", "/.confirmation-3.", "/runs/", "/audit/"))]


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


# --------------------------------------------------- artifacts, hashes, command scan

def tree_hashes(root):
    """sha256 of every regular file under root (relative paths), walked descriptor-relative
    (os.fwalk, no symlink following); symlinks, other non-regular entries or any traversal
    error make it fail."""
    out = {}
    for dirpath, dirs, files, dfd in os.fwalk(root, follow_symlinks=False, onerror=_raise):
        for name in dirs + files:
            mode = os.lstat(name, dir_fd=dfd).st_mode
            if stat.S_ISLNK(mode) or not (stat.S_ISREG(mode) or stat.S_ISDIR(mode)):
                raise PermissionError("non-regular entry")
        for name in files:
            rel = os.path.relpath(os.path.join(dirpath, name), root)
            out[rel] = hashlib.sha256(_read_fd(os.open(name, _FILE_FLAGS, dir_fd=dfd))).hexdigest()
    return out


def hashes_digest(hashes):
    """One sha256 over a family's verified file hashes (author status.json)."""
    return hashlib.sha256(json.dumps(hashes, sort_keys=True).encode()).hexdigest()


def suite_digest():
    """One sha256 over the assembled suite's files (relative paths and contents)."""
    h = hashlib.sha256()
    for rel, digest in sorted(tree_hashes(SUITE).items()):
        h.update(f"{rel}\0{digest}\n".encode())
    return h.hexdigest()


def prior_manifest():
    """Frozen at selection: one sha256 over every prior task file (sorted), and the count."""
    h, n = hashlib.sha256(), 0
    for name, root, excl in prior_roots():
        for tj in task_files(root, excl):
            h.update(hashlib.sha256(Path(tj).read_bytes()).digest())
            n += 1
    return {"sha256": h.hexdigest(), "count": n}


# Not "at"/"batch": common prose words (a heredoc or multi-line command line starting with
# "at" cost 2 of the first 7 authoring attempts, 2026-10-19), and atrun is not loaded on macOS,
# so a queued job never runs.
SUSPICIOUS_TOOLS = {"launchctl", "osascript", "open", "curl", "wget", "nc", "ncat", "ssh", "scp", "sftp", "git",
                    "pip", "pip3", "npm", "brew", "sudo", "su", "security", "defaults", "pbcopy", "pbpaste",
                    "crontab", "ping", "dig", "nslookup", "telnet", "ftp", "rsync"}
OUTSIDE_PATH = re.compile(r"/Users/|/Volumes/|/private/(?:tmp|var)|~/|\$HOME|\.a2d-")
SYSTEM_TOOL_PATH = re.compile(r"/(?:usr/)?s?bin/(" + "|".join(sorted(SUSPICIOUS_TOOLS)) + r")\b")


HEREDOC = re.compile(r"<<(-?)\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\2")
DATA_SINKS = {"cat", "tee"}


def _strip_data_heredocs(inner):
    """Drop the bodies of heredocs fed to cat or tee: they are file content, not commands
    (a body line starting with "at" or "open" is prose). A heredoc fed to anything else
    (sh, bash, python, ...) is kept and scanned line by line. File writes through patches
    are not scanned at all, so this is no weaker; the seatbelt profile is the boundary."""
    lines, out, i = inner.split("\n"), [], 0
    while i < len(lines):
        line = lines[i]
        out.append(line)
        i += 1
        m = HEREDOC.search(line)
        if not m:
            continue
        words = re.split(r"[;&|]+|\$\(|`", line[:m.start()])[-1].split()
        if not words or os.path.basename(words[0].strip("\"'(")) not in DATA_SINKS:
            continue
        end, tabs = m.group(3), m.group(1) == "-"
        while i < len(lines) and (lines[i].lstrip("\t") if tabs else lines[i]) != end:
            i += 1
        if i < len(lines):
            out.append(lines[i])                       # the terminator
            i += 1
    return "\n".join(out)


MUTANT_NAMES = tuple(f"m{i}.rs" for i in (1, 2, 3, 4))


def mutants_ok(d):
    """A task's mutants/ holds m1.rs..m4.rs as plain files, and no other source file or
    directory (which would make the mutant set ambiguous). Other files, such as an author's
    notes, are ignored: assembly copies only m1..m4 (2026-10-19, plan §0.13)."""
    d = Path(d)
    if d.is_symlink() or not d.is_dir():
        return False
    entries = list(d.iterdir())
    names = {p.name for p in entries}
    if not set(MUTANT_NAMES) <= names:
        return False
    for p in entries:
        if p.is_symlink() or p.is_dir() or (p.name.endswith(".rs") and p.name not in MUTANT_NAMES):
            return False
    return all((d / m).is_file() for m in MUTANT_NAMES)


def _segments(inner):
    """First word of every shell segment (split on ; & | newlines, $( and backticks)."""
    words = []
    for seg in re.split(r"[;&|\n]+|\$\(|`", inner):
        toks = [t for t in seg.strip().split() if "=" not in t.split("/")[0] or t.startswith("/")]
        if toks:
            words.append(os.path.basename(toks[0].strip("\"'(")))
    return words


def suspicious_commands(record, workdir):
    """Count-only scan of a job's command strings: broker/network tools or paths outside
    the workdir and toolchain. Content is never returned. Missing transcript counts 1."""
    try:
        lines = safe_read_text(RECORDS / record / "codex.jsonl").splitlines()
    except OSError:
        return 1
    w, tc = str(Path(workdir).resolve()), str(TOOLCHAIN.resolve())
    home = str(Path.home())
    # the toolchain may also be spelled through $HOME or ~ (e.g. "$HOME/.rustup/.../rustc")
    tcs = [tc] + ([tc.replace(home, h, 1) for h in ("${HOME}", "$HOME", "~")] if tc.startswith(home + "/") else [])
    n = 0
    for line in lines:
        try:
            e = json.loads(line)
        except ValueError:
            continue
        item = e.get("item") if isinstance(e, dict) else None
        if e.get("type") == "item.started" and isinstance(item, dict) and item.get("type") == "command_execution":
            cmd = str(item.get("command", ""))
            m = re.match(r"^/bin/(?:zsh|bash|sh) -lc (['\"])(.*)\1$", cmd, re.S)
            inner = _strip_data_heredocs((m.group(2) if m else cmd).replace(w, ""))
            for t in tcs:
                inner = inner.replace(t, "")
            if OUTSIDE_PATH.search(inner) or SYSTEM_TOOL_PATH.search(inner) \
                    or any(x in SUSPICIOUS_TOOLS for x in _segments(inner)):
                n += 1
    return n
