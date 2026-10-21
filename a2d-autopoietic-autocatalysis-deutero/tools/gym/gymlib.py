"""Shared pieces of the gym v3 authoring pipeline (plan docs/plans/2026-10-21-gym-v3.md).

A copy of tools/holdout/c3lib.py, adapted for the gym's isolation from confirmation-3:

- Own root: GYM_ROOT (default ~/.a2d-gym). The gym never reads or exports A2D_HOLDOUT_DIR and
  never imports nano/confirm3.py (pure functions are vendored in gymvendor.py). Test hooks
  (GYM_TEST_*) are honoured only when TESTING: a non-default GYM_ROOT and GYM_TEST_SKIP_PREREQS;
  TESTING is confined to GYM_TEST_ROOT with a fake codex and fixture auth (refused at import
  otherwise), so it cannot make a real model call or write real state.
- Own STOP file, locks, records, tags, codex homes (A2D_CODEX_HOME_ROOT under the gym root),
  canary (~/.a2d-gym-canary.txt) and a separate CARGO_TARGET_DIR (<root>/target), so the repo's
  target/debug/a2d is never rebuilt by gym tooling.
- Launch gate (every driver and every job launch; fails closed, content-free):
  (a) runs/c2-confirm3/decision.json exists, or replicate 2's ledger (first attempt or its
      -relaunch1) holds a non-aborted `generation_complete` and no runs/c2-confirm3/manifest-r*.json
      exists yet (confirmation-3's timed scoring is due once they do); and
  (b) `git diff --quiet fb5110e7 -- <GATE_PATHS>` passes (and nothing is untracked there).
      (b) is never bypassed, not even in tests.
- Job profile: as c3lib's, plus explicit denials of ~/.a2d-holdout (and the gym root, the repo),
  placed after the allow rules. Must-exist denial probes: ~/.a2d-holdout/confirmation-3,
  .../authoring/confirmation-3/_records and .../runs/c2-confirm2-decision; a missing target
  fails the isolation check.
- Every compile of model-written Rust outside a codex job runs under `compile_profile` (modelled
  on crates/a2d-core/src/sandbox.rs): HOME, TMPDIR and the temp roots denied except the step dir;
  the rustc sysroot and xcrun's cache under TMPDIR allowed; env PATH/HOME/LANG/TMPDIR/
  DEVELOPER_DIR/SDKROOT only. Test binaries run under `run_profile` (no fork, exec, Mach, AE).
- Prior corpus: a read-only interface (PriorCorpus) over the repo's tasks/ and the holdout sets,
  disjoint from the gym's write roots; confirmation-3 joins it only after its decision.json.
- Output gateway, symlink-refusing host I/O, job quiescence by sandbox tag, launch handshake,
  driver lock and drain: as c3lib (see tools/holdout/README.md).
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

sys.path.insert(0, str(Path(__file__).resolve().parent))
from gymvendor import generation_complete_in  # noqa: E402

DEFAULT_ROOT = (Path.home() / ".a2d-gym").resolve()
ROOT = Path(os.environ.get("GYM_ROOT", str(DEFAULT_ROOT))).resolve()
# Test hooks (GYM_TEST_* overrides of the codex binary, pins, auth, canary, the holdout view and
# the repo output dir) are honoured only against a non-default gym root. TESTING is confined
# (assert_testing_confined, at import): every overridden path lies inside GYM_TEST_ROOT, the codex
# is a non-Mach-O fake, the auth file a fixture; so test mode cannot make a real model call or
# write real state.
TESTING = ROOT != DEFAULT_ROOT and bool(os.environ.get("GYM_TEST_SKIP_PREREQS"))
TESTS_DIR = Path(__file__).resolve().parent / "tests"
TEST_ROOT = Path(os.environ["GYM_TEST_ROOT"]).resolve() if TESTING and os.environ.get("GYM_TEST_ROOT") else None


def _cfg(name, default):
    return os.environ.get(name, default) if TESTING else default


REPO = Path(__file__).resolve().parents[2]
# The confirmation sets' dir, as a literal: never taken from A2D_HOLDOUT_DIR.
HOLDOUT_REAL = Path.home() / ".a2d-holdout"
REAL_AUTH_DEFAULT = Path.home() / ".codex" / "auth.json"
PIN_ROOT = Path.home() / ".a2d-codex-pin"
MACHO_MAGIC = (b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xfe\xed\xfa\xce",
               b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca")       # Mach-O 32/64 either endianness, universal
# What the gym treats as "the holdout dir" for the launch gate, the denial probes and the prior
# corpus. Production: the real dir. Tests: a fake under the test root (never the real one).
HOLDOUT_VIEW = Path(os.environ.get("GYM_TEST_HOLDOUT", str(ROOT / "fake-holdout"))).resolve() if TESTING \
    else HOLDOUT_REAL
# Where repo outputs go (split, band, export, pilot ledgers). Tests: a scratch git repo.
OUT_REPO = Path(os.environ.get("GYM_TEST_OUT_REPO", str(ROOT / "out-repo"))).resolve() if TESTING else REPO

GYM = ROOT / "authoring" / "gym-v3"
RECORDS = GYM / "_records"
SUITE = ROOT / "gym-v3"
RUNS = ROOT / "runs"
CODEX_HOMES = ROOT / "codex-homes"           # A2D_CODEX_HOME_ROOT for every gym `a2d lift run`
PROVIDER_CANARIES = ROOT / "canary"
TARGET = ROOT / "target"                     # CARGO_TARGET_DIR: never the repo's target/
BINARY = TARGET / "debug" / "a2d"
# The registered 0.162.0, restored byte-identical under ~/.a2d-codex-pin (Homebrew moved to 0.163.0 on 10-21; LOG).
CODEX = _cfg("GYM_CODEX", str(Path.home() / ".a2d-codex-pin" / "0.162.0" / "bin" / "codex"))
# Copied from c3lib; the installed codex was 0.163.0 on 2026-10-21. Re-pin (version and sha256
# prefix) before the first gym model call; until then the isolation check fails closed.
PINNED_CODEX = _cfg("GYM_PINNED_CODEX", "codex-cli 0.162.0")
PINNED_SHA = _cfg("GYM_PINNED_SHA", "8238273076041e55")
TOOLCHAIN = Path(_cfg("GYM_TOOLCHAIN", str(Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin")))
REAL_AUTH = Path(_cfg("GYM_AUTH", str(Path.home() / ".codex" / "auth.json")))
CANARY = Path(_cfg("GYM_CANARY", str(Path.home() / ".a2d-gym-canary.txt")))
AUTHORING_FILES = Path(__file__).resolve().parent / "authoring"

GATE_COMMIT = "fb5110e7285b37b8125ca6fb17c717fb871f3809"
# The plan's no-edit list plus confirmation-3's bound files (confirm3.BOUND).
GATE_PATHS = ("tools/holdout", "nano/confirm3.py", "nano/eval.py", "crates", "Cargo.toml", "Cargo.lock",
              "runs/c2-confirm3/registration.json", "runs/c2-confirm3/frozen.py")
# Replicate 2's ledgers: the first attempt and its one registered relaunch (confirm3.out_dir(2, relaunch)).
C3_R2_LEDGERS = (HOLDOUT_VIEW / "runs" / "c2-confirm3-r2" / "ledger.jsonl",
                 HOLDOUT_VIEW / "runs" / "c2-confirm3-r2-relaunch1" / "ledger.jsonl")
C3_DIR = OUT_REPO / "runs" / "c2-confirm3"
C3_DECISION = C3_DIR / "decision.json"
PIPELINE_PATHS = ("tools/gym", "docs/plans/2026-10-21-gym-v3.md")

DISABLE = [
    "browser_use", "browser_use_external",
    "browser_use_full_cdp_access", "browser_annotation_api", "computer_use", "apps", "plugins",
    "remote_plugin", "multi_agent", "image_generation", "in_app_browser", "skill_search",
    "skill_mcp_dependency_install", "tool_suggest", "js_repl", "goals", "hooks",
]
ALLOWED_ITEMS = {"agent_message", "reasoning", "command_execution", "file_change", "todo_list"}
ALLOWED_TOP = {"thread.started", "turn.started", "turn.completed", "item.started", "item.updated",
               "item.completed"}

ID_RE = re.compile(r"^g3-f\d{3}$")
TASK_RE = re.compile(r"^g3-f\d{3}-l[1-4]$")
# Rubric classes: the frozen rubric's difficulty sources (authoring/SPEC.md), a neutral taxonomy
# whose origin in the C2-FA2 failure analysis is disclosed there. Equal quotas, fixed before any brief.
CLASSES = ("depth", "state", "scale", "surface", "edges")
QUOTA_PER_CLASS = int(_cfg("GYM_TEST_QUOTA", "18"))     # 5 x 18 = 90 families (plan: ~85-90)
SPARES_PER_CLASS = int(_cfg("GYM_TEST_SPARES", "2"))
_KEYS = {"stage", "id", "exit", "count", "ok", "n", "max_sim", "conflicts", "other_events",
         "status", "error", "pending", "done", "skipped", "missing", "families", "tasks", "round",
         "max_contain", "unparsed", "set", "wave", "decision", "rate", "kept", "dropped", "repilot",
         "regression", "in_band", "voids", "agree", "rubric", "classes", "invalidated"}
_TOKEN = re.compile(r"^[a-z0-9][a-z0-9_.:=-]{0,31}$")


def _within(p, root):
    p, root = Path(os.path.abspath(p)), Path(os.path.abspath(root))
    return p == root or root in p.parents


def assert_disjoint():
    """The gym's write root never overlaps the holdout dir or the repo (either way round), and
    the test view of the holdout is never the real one."""
    real = Path(os.path.realpath(HOLDOUT_REAL))
    for other in (HOLDOUT_REAL, real, HOLDOUT_VIEW, REPO):
        if _within(ROOT, other) or _within(other, ROOT):
            raise RuntimeError("gym root overlaps a protected root")
    if TESTING and (_within(HOLDOUT_VIEW, HOLDOUT_REAL) or _within(HOLDOUT_VIEW, real)):
        raise RuntimeError("test holdout view is the real holdout")


def is_macho(path):
    try:
        with open(path, "rb") as f:
            return f.read(4) in MACHO_MAGIC
    except OSError:
        return False


def _real(p):
    return Path(os.path.realpath(os.path.expanduser(str(p))))


def assert_testing_confined():
    """TESTING must be incapable of a real model call or a real-state write (fails at import):
    - GYM_TEST_ROOT is set, is not HOME or an ancestor of it, and is disjoint (either way round)
      from the real holdout dir, the repo, the production gym root, ~/.codex, the codex pin and
      /opt/homebrew; GYM_ROOT lies inside it;
    - GYM_CODEX is set, resolves inside tools/gym/tests/ or the test root, and is not Mach-O (a
      script fake; never the pin, Homebrew's or any native codex);
    - GYM_AUTH is set, resolves inside the test root (a fixture) and is not ~/.codex/auth.json;
    - every other GYM_* value that is a path resolves inside the test root."""
    if not TESTING:
        return
    if TEST_ROOT is None or not TEST_ROOT.is_dir():
        raise RuntimeError("testing mode needs GYM_TEST_ROOT")
    t = _real(TEST_ROOT)
    if _within(Path.home(), t) or _within(_real(Path.home()), t):
        raise RuntimeError("test root contains HOME")
    for p in (HOLDOUT_REAL, _real(HOLDOUT_REAL), REPO, DEFAULT_ROOT, Path.home() / ".codex", PIN_ROOT,
              Path("/opt/homebrew")):
        if _within(t, p) or _within(p, t):
            raise RuntimeError("test root overlaps a protected root")
    if not _within(ROOT, t):
        raise RuntimeError("gym root outside the test root")
    codex, auth = os.environ.get("GYM_CODEX"), os.environ.get("GYM_AUTH")
    if not codex or not auth:
        raise RuntimeError("testing mode needs GYM_CODEX and GYM_AUTH")
    c = _real(codex)
    if not (_within(c, TESTS_DIR) or _within(c, t)) or not c.is_file() or is_macho(c):
        raise RuntimeError("test codex is not a fake")
    a = _real(auth)
    if not _within(a, t) or a == _real(REAL_AUTH_DEFAULT):
        raise RuntimeError("test auth is not a fixture")
    for k, v in os.environ.items():
        if k.startswith("GYM_") and k not in ("GYM_CODEX", "GYM_TEST_ROOT") and v.strip().startswith(("/", "~")):
            if not _within(_real(v.strip()), t):
                raise RuntimeError("a test path override outside the test root")


assert_disjoint()
assert_testing_confined()


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
    return f"g3-f{i:03d}"


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
    root = Path(os.path.abspath(root or ROOT))
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
    replaces a symlink itself, never its target). Writes are confined to the gym root unless a
    caller names another root explicitly (job workdirs are inside it)."""
    _check_chain(path, root or ROOT)
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
    _check_chain(dst, dst_root or ROOT)
    Path(dst).parent.mkdir(parents=True, exist_ok=True)
    safe_write_bytes(dst, data, dst_root or ROOT)
    if mode is not None:
        os.chmod(dst, mode)


# ------------------------------------------------------------- sandbox profiles

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


def _rx(s):
    return re.sub(r'([\\.^$|?*+()\[\]{}"])', r"\\\1", s.rstrip("/"))


def protected_roots():
    """Roots every gym profile denies explicitly, after its allow rules: the confirmation sets'
    dir (literal and resolved) and, in tests, the fake view."""
    out = []
    for p in (HOLDOUT_REAL, Path(os.path.realpath(HOLDOUT_REAL)), HOLDOUT_VIEW):
        s = str(Path(os.path.abspath(p)))
        if s not in out:
            out.append(s)
    return out


def _protect_rules():
    return "(deny file-read* file-write* " + " ".join(f"(subpath {_q(p)})" for p in protected_roots()) + ")"


MACH_ALLOW = ("com.apple.logd", "com.apple.system.notification_center", "com.apple.system.opendirectoryd.libinfo",
              "com.apple.system.opendirectoryd.membership", "com.apple.bsd.dirhelper", "com.apple.trustd",
              "com.apple.trustd.agent", "com.apple.SystemConfiguration.configd",
              "com.apple.SecurityServer")   # codex loads TLS roots through it; keychain files stay unreadable


def codex_bin_dir():
    return str(Path(CODEX).resolve().parent)


def codex_package_root():
    """The codex package directory (bin/, codex-path/, codex-resources/) when CODEX sits in a package
    layout, else the binary's own directory."""
    bin_dir = Path(CODEX).resolve().parent
    return bin_dir.parent if bin_dir.name == "bin" and (bin_dir.parent / "codex-package.json").exists() else bin_dir


def profile(workdir, tag):
    """SBPL profile for one codex job (c3lib's, plus the explicit protected-root denials last).
    `tag` is an empty driver-owned file only this profile may read (see job_members)."""
    home = str(Path.home())
    w = _rp(workdir)
    codex_dir = _rp(codex_package_root())
    brokers = ["/bin/launchctl", "/usr/bin/open", "/usr/bin/osascript", "/usr/bin/crontab", "/usr/bin/at",
               "/usr/bin/batch", "/usr/bin/pbcopy", "/usr/bin/pbpaste", "/usr/bin/ssh", "/usr/bin/ssh-add",
               "/usr/bin/scp", "/usr/bin/sftp", "/usr/bin/sudo", "/usr/bin/su", "/usr/bin/security",
               "/usr/bin/defaults", "/usr/bin/screen", "/usr/bin/tmux"]
    lines = [
        "(version 1)",
        "(allow default)",
        "(deny appleevent-send)",
        "(deny signal)",
        "(allow signal (target same-sandbox))",
        "(deny process-info*)",
        "(allow process-info* (target same-sandbox))",
        "(deny process-exec " + " ".join(f"(literal {_q(b)})" for b in brokers) + ")",
        "(deny mach-lookup)",
        "(allow mach-lookup " + " ".join(f"(global-name {_q(n)})" for n in MACH_ALLOW) + ")",
        '(deny network-outbound (remote ip "localhost:*"))',
        "(deny network-outbound (remote unix-socket"
        f" (subpath {_q(home)}) (subpath \"/Users\") (subpath \"/private/tmp\") (subpath \"/private/var/folders\")))",
        f"(allow network-outbound (remote unix-socket (subpath {_q(w)})))",
        "(deny file-write*)",
        "(deny file-read*"
        f" (subpath {_q(home)}) (subpath {_q('/System/Volumes/Data' + home)}) (subpath \"/Users\")"
        f" (subpath \"/System/Volumes/Data/Users\") (subpath \"/Volumes\")"
        f" (subpath {_q(str(ROOT))}) (subpath {_q(str(REPO))})"
        f" (subpath {_q(_rp(os.environ.get('TMPDIR', '/private/tmp')))})"
        ' (subpath "/private/tmp") (subpath "/private/var/folders") (subpath "/private/var/tmp"))',
        f"(allow file-read* file-write* (subpath {_q(w)}))",
        '(allow file-write* (literal "/dev/null") (literal "/dev/zero"))',
        f"(allow file-read* (literal {_q(_rp(REAL_AUTH))}) (literal {_q(_rp(tag))}))",
        f"(allow file-read* (subpath {_q(_rp(TOOLCHAIN))}) (subpath {_q(codex_dir)}))",
        "(allow file-read-metadata)",
        '(deny file-read* file-write* (regex #"^/dev/tty"))',
    ]
    user_tmp = _user_temp_dir()
    if user_tmp:
        lines.append(f'(allow file-read* file-write* (regex #"^{_rx(user_tmp)}/xcrun_db(-[A-Za-z0-9]+)?$"))')
    lines.append(_protect_rules())          # last: no allow rule above can reopen the holdout dir
    return "\n".join(lines) + "\n"


def _compile_denied():
    roots = ["/private/tmp", "/private/var/folders", "/private/var/tmp"]
    roots += [os.path.realpath(os.environ[n]) for n in ("HOME", "TMPDIR") if os.environ.get(n)]
    return " ".join(f"(subpath {_q(r)})" for r in roots)


def _xcrun_cache_rule():
    """The linker's xcrun shim caches its toolchain lookup in $TMPDIR/xcrun_db (as the oracle allows)."""
    tmp = os.environ.get("TMPDIR")
    if not tmp:
        return ""
    return f'(allow file-read* file-write* (regex #"^{_rx(os.path.realpath(tmp))}/xcrun_db(-[A-Za-z0-9]+)?$"))'


def compile_profile(step_dir, sysroot):
    """rustc and its linker on model-written Rust (sandbox.rs `Step::Compile`): no network; HOME,
    TMPDIR and the temp roots denied except the step dir; the sysroot readable; xcrun's cache."""
    return (f"(version 1)(allow default)(deny network*)(deny file-read* file-write* {_compile_denied()})"
            f"(allow file-read* (subpath {_q(sysroot)})){_xcrun_cache_rule()}"
            f"(allow file-read* file-write* (subpath {_q(os.path.realpath(step_dir))}))"
            f"{_protect_rules()}")


def run_profile(binary, run_dir):
    """A test binary (sandbox.rs `Step::Run`): no network, no fork/exec/Mach/Apple Events; it
    reads only itself and its own run dir."""
    binary = os.path.realpath(binary)
    return (f"(version 1)(allow default)(deny network*)(deny file-read* file-write* {_compile_denied()})"
            f"(deny process-fork process-exec* mach-lookup appleevent-send)"
            f"(allow process-exec (literal {_q(binary)}))(allow file-read* (literal {_q(binary)}))"
            f"(allow file-read* file-write* (subpath {_q(os.path.realpath(run_dir))}))"
            f"{_protect_rules()}")


COMPILE_ENV = ("PATH", "HOME", "LANG", "TMPDIR", "DEVELOPER_DIR", "SDKROOT")


def compile_env():
    return {k: os.environ[k] for k in COMPILE_ENV if k in os.environ}


def rustc():
    return str(TOOLCHAIN / "bin" / "rustc")


_SYSROOT = None


def sysroot():
    global _SYSROOT
    if _SYSROOT is None:
        out = subprocess.run([rustc(), "--print", "sysroot"], capture_output=True, text=True, timeout=60,
                             env=compile_env(), cwd="/").stdout.strip()
        if not out:
            raise RuntimeError("rustc sysroot")
        _SYSROOT = os.path.realpath(out)
    return _SYSROOT


def sandboxed_rustc(args, step_dir, timeout=180):
    """One rustc invocation under compile_profile, cwd = step_dir. Returns CompletedProcess or
    raises TimeoutExpired. The only way gym tooling compiles model-written Rust."""
    return subprocess.run(["/usr/bin/sandbox-exec", "-p", compile_profile(step_dir, sysroot()), rustc(), *args],
                          capture_output=True, text=True, cwd=step_dir, env=compile_env(), timeout=timeout,
                          stdin=subprocess.DEVNULL)


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


TEST_PATH_TAIL = "/usr/bin:/bin"        # TESTING: no Homebrew, no pin, no native codex on any PATH


def _env(workdir, home):
    tail = TEST_PATH_TAIL if TESTING else "/usr/bin:/bin:/opt/homebrew/bin"
    env = {"HOME": str(Path.home()), "PATH": codex_bin_dir() + ":" + tail,
           "CODEX_HOME": str(home), "TMPDIR": _rp(workdir), "LANG": "en_US.UTF-8",
           "RUSTC": str(TOOLCHAIN / "bin" / "rustc")}
    if TESTING:
        env.update({k: v for k, v in os.environ.items() if k.startswith("GYM_TEST_")})  # test hooks only
    return env


SCRUBBED = ("A2D_HOLDOUT_DIR",)


def child_env(**extra):
    """Environment for gym subprocesses that are not codex jobs (`a2d lift run`, cargo): the
    parent's, minus A2D_HOLDOUT_DIR and every C3_* / GYM_TEST_* hook (kept only in tests). PATH
    starts with the pinned codex's dir (the provider spawns `codex` by name); in tests it is that
    (fake) dir plus /usr/bin:/bin only, so no real codex is ever reachable by name."""
    env = {k: v for k, v in os.environ.items()
           if k not in SCRUBBED and not k.startswith("C3_") and (TESTING or not k.startswith("GYM_TEST_"))}
    tail = TEST_PATH_TAIL if TESTING else env.get("PATH", "/usr/bin:/bin")
    env.update({"A2D_CODEX_HOME_ROOT": str(CODEX_HOMES), "CARGO_TARGET_DIR": str(TARGET),
                "PATH": codex_bin_dir() + ":" + tail})
    env.update(extra)
    return env


def binary_allowed(binary, default):
    """An `a2d` override (--binary) is accepted only in tests, and only a non-Mach-O fake inside
    tools/gym/tests/ or the test root."""
    if Path(binary) == Path(default):
        return True
    if not TESTING:
        return False
    b = _real(binary)
    return (_within(b, TESTS_DIR) or _within(b, _real(TEST_ROOT))) and b.is_file() and not is_macho(b)


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
            _kill_group(p)
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


def must_deny():
    """Denial targets that must EXIST and be unreadable from every job (a missing one fails)."""
    return [HOLDOUT_VIEW / "confirmation-3", HOLDOUT_VIEW / "authoring" / "confirmation-3" / "_records",
            HOLDOUT_VIEW / "runs" / "c2-confirm2-decision"]


def isolation_check(workdir):
    """True iff the job may launch. Never prints; callers emit the boolean."""
    w = Path(workdir).resolve()
    if GYM not in w.parents:
        return False
    home = Path.home().resolve()
    under_home = []
    for d in [w, *w.parents]:
        for bad in AMBIENT:
            p = d / bad
            if os.path.lexists(p):
                if home in p.parents or p.parent == home:
                    under_home.append(p)
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
    secret = f"GYM-CANARY-{tag}"
    canary = CANARY.parent / f"{CANARY.name}.{tag}"
    safe_write_text(canary, secret + "\n", CANARY.parent)
    try:
        return _probes(w, canary, secret, extra_denied)
    finally:
        try:
            os.unlink(canary)
        except FileNotFoundError:
            pass


def _denied(w, target, secret):
    argv = ["/bin/ls", str(target)] if Path(target).is_dir() else ["/bin/cat", str(target)]
    r = sandboxed(w, argv, capture_output=True, text=True, timeout=60)
    return r.returncode != 0 and secret not in r.stdout + r.stderr and "Operation not permitted" in r.stderr


def _probes(w, canary, secret, extra_denied):
    ctl = w / f".gym-control-{secret[-24:]}"
    safe_write_text(ctl, "CONTROL-OK\n", w)
    r = sandboxed(w, ["/bin/cat", str(ctl)], capture_output=True, text=True, timeout=60)
    if r.returncode != 0 or "CONTROL-OK" not in r.stdout:
        return False
    ctl.unlink()
    required = [canary, RECORDS, REPO / "CLAUDE.md", *must_deny()]
    optional = [HOLDOUT_VIEW / "confirmation-2", HOLDOUT_VIEW / "dev-holdout",
                Path.home() / ".codex" / "config.toml", *extra_denied]
    for target in required:
        if not os.path.lexists(target) or not _denied(w, target, secret):
            return False
    for target in optional:
        if os.path.lexists(target) and not _denied(w, target, secret):
            return False
    r = sandboxed(w, ["/usr/bin/touch", str(GYM / f".gym-write-probe-{os.getpid()}")],
                  capture_output=True, text=True, timeout=60)
    if r.returncode == 0 or "Operation not permitted" not in r.stderr:
        return False
    return _brokers_closed(w, secret)


BROKER_COPIES = ("/usr/bin/open", "/bin/launchctl", "/usr/bin/pbpaste", "/usr/bin/mdfind")


def _brokers_closed(w, secret):
    """Copies of broker binaries (which bypass the exec denies) must not reach their
    services, and a local TCP listener must be unreachable."""
    import socket
    w = Path(w)
    pdir = w / f".gym-brokers-{secret[-24:]}"
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
            return False
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

TAGS = RECORDS / "tags"
_FILTER_PATH, _NO_REPORT = 1, 0x40000000
_lib = None


def _libsystem():
    global _lib
    if _lib is None:
        import ctypes
        lib = ctypes.CDLL("/usr/lib/libSystem.B.dylib", use_errno=True)
        lib.sandbox_check.restype = ctypes.c_int
        lib.sandbox_check.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int]
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
        time.sleep(0.2)


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


class LaunchGateClosed(Exception):
    """The confirmation-3 isolation gate refused; nothing was launched."""


class _Gate:
    """One exclusive lock (records/launch.lock) serialises STOP creation and every job launch."""

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
        GYM.mkdir(parents=True, exist_ok=True)
        safe_write_text(GYM / "STOP", "stop\n")


_HANDSHAKE = 'IFS= read -r go && [ "$go" = go ] && exec /usr/bin/sandbox-exec -f "$0" "$@" </dev/null; exit 125'


def _handshake_popen(prof, argv, **kw):
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
    """Start one job through the launch gate (refused once STOP exists, and refused while the
    confirmation-3 isolation gate is closed) and the handshake."""
    if not launch_gate():
        raise LaunchGateClosed()
    with _Gate():
        if stop_requested():
            raise StopRequested()
        return _handshake_popen(prof, argv, **kw)


def run_codex(workdir, record, model="gpt-6.1-sol", effort="high", timeout=7200):
    """Run one pointer-prompted codex job in workdir under the outer sandbox. Returns
    (exit_code, other_event_count); the driver never reads model text, only event types."""
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
        try:
            p = launch(prof, argv, cwd=w, env=_env(w, home), stdin=subprocess.DEVNULL, stdout=out, stderr=err)
        except (StopRequested, LaunchGateClosed):
            retire_tag(tag)
            raise
        try:
            code = p.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            code = 124
        finally:
            _kill_group(p)
            p.wait()
            if not reap(tag):
                raise RuntimeError("job not quiesced")
    retire_tag(tag)
    safe_write_text(rec / "exit.txt", f"{code}\n")
    return code, count_other_events(rec / "codex.jsonl")


def count_other_events(jsonl):
    """Count events outside the allowlist; a missing or empty transcript counts as 1."""
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
        return json.loads(safe_read_text(path, _root_of(path)))
    except FileNotFoundError:
        return default


def _root_of(path):
    for r in (ROOT, OUT_REPO):
        if _within(path, r) and Path(os.path.abspath(path)) != Path(os.path.abspath(r)):
            return r
    raise PermissionError("outside the gym's roots")


def repo_write_root(path):
    """The only repo locations gym tooling writes: runs/c2-gym3*/ (split, band, acceptance,
    export of requirements/public tests/hashes, pilot ledgers). Returns the root to write under."""
    p = Path(os.path.abspath(path))
    runs = Path(os.path.abspath(OUT_REPO / "runs"))
    if runs not in p.parents or not p.relative_to(runs).parts[0].startswith("c2-gym3"):
        raise PermissionError("repo write outside runs/c2-gym3*")
    return OUT_REPO


def save_json(path, obj):
    root = ROOT if _within(path, ROOT) else repo_write_root(path)
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    safe_write_text(path, json.dumps(obj, indent=1, sort_keys=True) + "\n", root)


def _git(*args, repo=None):
    return subprocess.run(["git", "-C", str(repo or REPO), *args], capture_output=True, text=True, timeout=600,
                          stdin=subprocess.DEVNULL)


def frozen_since(repo, commit, paths):
    """`git diff --quiet <commit> -- <paths>` passes and nothing is untracked under the paths."""
    try:
        if _git("diff", "--quiet", commit, "--", *paths, repo=repo).returncode != 0:
            return False
        r = _git("ls-files", "--others", "--exclude-standard", "--", *paths, repo=repo)
    except (OSError, subprocess.TimeoutExpired):
        return False
    return r.returncode == 0 and not r.stdout.strip()


def c3_frozen():
    """Gate (b): tools/holdout, nano/confirm3.py, crates/ and Cargo.* equal fb5110e7 (no diff and
    nothing untracked there). Run against the real repo, always; never bypassed."""
    return frozen_since(REPO, GATE_COMMIT, GATE_PATHS)


def c3_decided():
    return os.path.lexists(C3_DECISION)


def c3_scoring_started():
    """confirmation-3's manifests (written right before scoring) exist: its timed hidden runs are
    due, so the gym must be idle until decision.json (runbook: --stop and --drained first)."""
    try:
        return any(os.path.lexists(p) for p in C3_DIR.glob("manifest-r*.json"))
    except OSError:
        return True


def c3_window_open():
    """Gate (a): confirmation-3 is decided, or replicate 2 has generated (a non-aborted
    `generation_complete` in its first attempt's or its relaunch's ledger, confirm3.complete's
    rule) and confirmation-3's scoring has not started (no manifest-r*.json). In tests the
    paths point into the test root (fake holdout view, scratch repo)."""
    if c3_decided():
        return True
    return any(generation_complete_in(p) for p in C3_R2_LEDGERS) and not c3_scoring_started()


def launch_gate():
    return c3_frozen() and c3_window_open()


def gate_or_exit(stage):
    """Every driver's first step: fail closed with a content-free line."""
    if not launch_gate():
        emit(stage=stage, status="launch-gate-closed")
        sys.exit(2)


def prereqs_ok():
    """The launch gate, then (production only) the gym pipeline and plan are unmodified at HEAD;
    records the commit and a manifest of every pipeline file."""
    if not launch_gate():
        return False
    if TESTING:
        return True
    if _git("status", "--porcelain", "--", *PIPELINE_PATHS).stdout.strip():
        return False
    head = _git("rev-parse", "HEAD").stdout.strip()
    files = _git("ls-files", "--", *PIPELINE_PATHS).stdout.split()
    manifest = {f: hashlib.sha256((REPO / f).read_bytes()).hexdigest() for f in files}
    save_json(RECORDS / "pipeline.json", {"commit": head, "files": manifest,
                                          "checked_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})
    return bool(head) and bool(manifest)


def build_a2d():
    """Build the controller into the gym's own CARGO_TARGET_DIR (never target/debug/a2d)."""
    TARGET.mkdir(parents=True, exist_ok=True)
    with open(RECORDS / "build.log", "ab") as log:
        code = subprocess.run(["cargo", "build", "--locked", "--bin", "a2d"], cwd=REPO, stdout=log,
                              stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL, env=child_env()).returncode
    return code == 0 and BINARY.is_file()


class DriverLock:
    """One driver at a time; its pid is recorded so drained() can see it."""

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
    return os.path.lexists(GYM / "STOP")


def relocation_evidenced():
    for fid in (load_json(RECORDS / "active.json", {}).get("active") or []):
        if load_json(RECORDS / f"author-{fid}" / "status.json", {"attempts": 0})["attempts"] < 1:
            continue
        sessions = GYM / fid / "_codex_home" / "sessions"
        if not sessions.is_dir() or not any(p.is_file() for p in sessions.rglob("*")):
            return False
    return True


def drained():
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
    error raises: an inventory must never be partial."""
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


# ------------------------------------------------------------- prior corpus (read-only)

EXPECTED_HOLDOUT_SETS = ("confirmation-1", "confirmation-2", "dev-holdout", "c1-validation")


class PriorCorpus:
    """Read-only view of every prior task set: the repo's tasks/ and the holdout dir's sets.
    It has no write method and its roots are disjoint from the gym's write root (checked).
    confirmation-3 (its suite and authoring history) joins only once its decision.json exists."""

    BASE_EXCLUDE = ("/_records", "/.confirmation-3.", "/runs/", "/audit/", "/codex-homes/", "/canary/",
                    "/confirmation-3")

    def __init__(self):
        self.base = [("repo-tasks", REPO / "tasks", ()), ("holdout", HOLDOUT_VIEW, self.BASE_EXCLUDE)]
        self.c3 = [("confirmation-3", HOLDOUT_VIEW / "confirmation-3", ()),
                   ("confirmation-3-authoring", HOLDOUT_VIEW / "authoring" / "confirmation-3",
                    ("/_records", "/runs/", "/audit/"))] if c3_decided() else []
        self.roots = self.base + self.c3
        for _, r, _ in self.roots:
            if _within(r, ROOT) or _within(ROOT, r):
                raise RuntimeError("prior corpus overlaps the gym root")

    def files(self, base_only=False):
        return [tj for _, root, excl in (self.base if base_only else self.roots) for tj in task_files(root, excl)]

    def tasks(self):
        """[(family, requirements)] and per-root counts; fails if a root or an expected set is
        empty or a record is malformed."""
        rows, counts = [], {}
        for name, root, excl in self.roots:
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
        if not TESTING:
            for s in EXPECTED_HOLDOUT_SETS:
                if not task_files(HOLDOUT_VIEW / s):
                    raise ValueError("missing prior set")
        if self.c3 and counts.get("confirmation-3", 0) == 0:
            raise ValueError("confirmation-3 decided but empty")
        return rows, counts

    def manifest(self):
        """Frozen at selection: one sha256 over every base prior task file (sorted) and the
        count. confirmation-3 is outside it, so its later arrival does not void the freeze."""
        h, n = hashlib.sha256(), 0
        for tj in self.files(base_only=True):
            h.update(hashlib.sha256(Path(tj).read_bytes()).digest())
            n += 1
        return {"sha256": h.hexdigest(), "count": n}


def replace_family(fid, reason):
    """An excluded family is replaced by the next unused spare of the SAME rubric class (in id
    order), which inherits its split slot; otherwise it is dropped. Recorded in active.json."""
    act = load_json(RECORDS / "active.json")
    if fid not in act["active"]:
        return None
    cls = act["classes"][fid]
    pool = act["spares"].get(cls, [])
    spare = pool.pop(0) if pool else None
    i = act["active"].index(fid)
    if spare:
        act["active"][i] = spare
    else:
        act["active"].pop(i)
    act["events"].append({"family": fid, "reason": reason, "replacement": spare})
    save_json(RECORDS / "active.json", act)
    return spare


def lineage(fid, act=None):
    """The family a spare replaced, recursively (the split slot it holds)."""
    act = act or load_json(RECORDS / "active.json", {})
    for ev in act.get("events", []):
        if ev.get("replacement") == fid:
            return lineage(ev["family"], act)
    return fid


# --------------------------------------------------- artifacts, hashes, command scan

def tree_hashes(root):
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
    return hashlib.sha256(json.dumps(hashes, sort_keys=True).encode()).hexdigest()


def dir_digest(root):
    h = hashlib.sha256()
    for rel, digest in sorted(tree_hashes(root).items()):
        h.update(f"{rel}\0{digest}\n".encode())
    return h.hexdigest()


def suite_digest():
    return dir_digest(SUITE)


def spec_sha():
    """The frozen rubric's hash (authoring/SPEC.md), bound into certification and every pilot."""
    return hashlib.sha256((AUTHORING_FILES / "SPEC.md").read_bytes()).hexdigest()


SUSPICIOUS_TOOLS = {"launchctl", "osascript", "open", "curl", "wget", "nc", "ncat", "ssh", "scp", "sftp", "git",
                    "pip", "pip3", "npm", "brew", "sudo", "su", "security", "defaults", "pbcopy", "pbpaste",
                    "crontab", "ping", "dig", "nslookup", "telnet", "ftp", "rsync"}
OUTSIDE_PATH = re.compile(r"/Users/|/Volumes/|/private/(?:tmp|var)|~/|\$HOME|\.a2d-")
SYSTEM_TOOL_PATH = re.compile(r"/(?:usr/)?s?bin/(" + "|".join(sorted(SUSPICIOUS_TOOLS)) + r")\b")
HEREDOC = re.compile(r"<<(-?)\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\2")
DATA_SINKS = {"cat", "tee"}


def _strip_data_heredocs(inner):
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
            out.append(lines[i])
            i += 1
    return "\n".join(out)


MUTANT_NAMES = tuple(f"m{i}.rs" for i in (1, 2, 3, 4))


def mutants_ok(d):
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
    words = []
    for seg in re.split(r"[;&|\n]+|\$\(|`", inner):
        toks = [t for t in seg.strip().split() if "=" not in t.split("/")[0] or t.startswith("/")]
        if toks:
            words.append(os.path.basename(toks[0].strip("\"'(")))
    return words


def suspicious_commands(record, workdir):
    """Count-only scan of a job's command strings. Missing transcript counts 1."""
    try:
        lines = safe_read_text(RECORDS / record / "codex.jsonl").splitlines()
    except OSError:
        return 1
    w, tc = str(Path(workdir).resolve()), str(TOOLCHAIN.resolve())
    home = str(Path.home())
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
