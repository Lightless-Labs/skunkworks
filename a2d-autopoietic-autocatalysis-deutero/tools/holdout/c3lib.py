"""Shared pieces of the confirmation-3 blind authoring pipeline.

Plan: docs/plans/2026-10-18-confirmation-3.md (§0.4, §0.6, §0.10). Rules this module
enforces mechanically:

- Output gateway: scripts print ONLY `emit(...)` lines whose keys are allowlisted and
  whose values are numbers, booleans or short opaque tokens. Exceptions print only
  their class name. No task content, file names, family names or model output ever
  reaches the developing agent through these scripts.
- Every codex job runs inside an outer macOS seatbelt profile (`sandbox-exec`) that
  denies reads and writes under $HOME and the temp roots except its own workdir, the
  dedicated CODEX_HOME (auth.json only) and the Rust toolchain. Codex's own sandbox
  is off inside it (macOS cannot nest seatbelt profiles); the outer profile is the
  enforcement. Network stays open (codex needs the API); disclosed.
- Launch isolation is checked before every job (no .codex/AGENTS.md/skills in the
  workdir's ancestry, CODEX_HOME inside the holdout dir and holding only auth.json,
  pinned codex version, canary read denied). A failed check means the job does not run.
- Prompts are passed as a pointer ("read INSTRUCTIONS.md"), never in argv.
- Every job's pid is recorded; `drained()` checks with kill -0, never pgrep -f.
"""
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

HOLDOUT = Path(os.environ.get("A2D_HOLDOUT_DIR", str(Path.home() / ".a2d-holdout"))).resolve()
C3 = HOLDOUT / "authoring" / "confirmation-3"
SUITE = HOLDOUT / "confirmation-3"
CODEX = os.environ.get("C3_CODEX", "/opt/homebrew/bin/codex")
PINNED_CODEX = os.environ.get("C3_PINNED_CODEX", "codex-cli 0.162.0")
TOOLCHAIN = Path(os.environ.get("C3_TOOLCHAIN", str(Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin")))
REAL_AUTH = Path(os.environ.get("C3_AUTH", str(Path.home() / ".codex" / "auth.json")))
CANARY = Path(os.environ.get("C3_CANARY", str(Path.home() / ".a2d-canary.txt")))
AUTHORING_FILES = Path(__file__).resolve().parent / "authoring"

# Features that give a model tools beyond shell/file edits in its own dir. Authoring
# needs shell (to run check.sh) and file edits; everything else is off.
DISABLE = [
    "web_search_request", "standalone_web_search", "browser_use", "browser_use_external",
    "browser_use_full_cdp_access", "browser_annotation_api", "computer_use", "apps", "plugins",
    "remote_plugin", "multi_agent", "image_generation", "in_app_browser", "skill_search",
    "skill_mcp_dependency_install", "tool_suggest", "js_repl", "goals", "hooks",
]
# --json item types allowed for authoring jobs; anything else is counted as "other".
ALLOWED_ITEMS = {"agent_message", "reasoning", "command_execution", "file_change", "todo_list"}
ALLOWED_TOP = {"thread.started", "turn.started", "turn.completed", "item.started", "item.updated",
               "item.completed"}

_KEYS = {"stage", "id", "exit", "count", "ok", "n", "max_sim", "conflicts", "other_events",
         "status", "error", "pending", "done", "skipped", "missing", "families", "tasks", "round"}
_TOKEN = re.compile(r"^[a-z0-9_.:=-]{0,48}$")


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
        elif isinstance(v, str) and _TOKEN.match(v):
            out[k] = v
        else:
            out[k] = "redacted"
    print(json.dumps(out), flush=True)


def gateway(stage):
    """Decorator: run main(), turning any exception into a content-free error line."""
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


ID_RE = re.compile(r"^c3-f\d{2}$")


def codex_home():
    """Dedicated CODEX_HOME inside the holdout dir: exactly one entry, auth.json -> real auth."""
    home = C3 / "_codex_home"
    home.mkdir(parents=True, exist_ok=True)
    link = home / "auth.json"
    if not link.exists():
        link.symlink_to(REAL_AUTH)
    return home


def _rp(p):
    return str(Path(p).resolve())


def profile(workdir):
    """SBPL profile for one job: deny $HOME and temp roots, re-allow the minimum."""
    home = str(Path.home())
    tmp = _rp(os.environ.get("TMPDIR", "/private/tmp"))
    w, c = _rp(workdir), _rp(codex_home())
    codex_dir = _rp(Path(CODEX).resolve().parent)
    lines = [
        "(version 1)",
        "(allow default)",
        "(deny file-read* file-write*"
        f' (subpath "{home}") (subpath "/System/Volumes/Data{home}") (subpath "{tmp}")'
        ' (subpath "/private/tmp") (subpath "/private/var/folders") (subpath "/private/var/tmp"))',
        f'(allow file-read* file-write* (subpath "{w}") (subpath "{c}"))',
        f'(allow file-read* file-write* (literal "{_rp(REAL_AUTH)}"))',
        f'(allow file-read* (subpath "{_rp(TOOLCHAIN)}") (subpath "{codex_dir}"))',
        "(allow file-read-metadata)",
    ]
    return "\n".join(lines) + "\n"


def write_profile(workdir):
    p = Path(workdir) / ".c3.sb"
    p.write_text(profile(workdir))
    return p


def sandboxed(workdir, argv, **kw):
    """Run argv under the job's profile with a cleared environment."""
    env = {"HOME": str(Path.home()), "PATH": "/usr/bin:/bin:/opt/homebrew/bin",
           "CODEX_HOME": str(codex_home()), "TMPDIR": _rp(workdir), "LANG": "en_US.UTF-8",
           "RUSTC": str(TOOLCHAIN / "bin" / "rustc")}
    return subprocess.run(["/usr/bin/sandbox-exec", "-f", str(write_profile(workdir)), *argv],
                          cwd=workdir, env=env, stdin=subprocess.DEVNULL, **kw)


def isolation_check(workdir):
    """True iff the job may launch. Never prints; callers emit the boolean."""
    w = Path(workdir).resolve()
    # Ancestors from the workdir up to the holdout root must carry no project-level
    # codex/agent context. Above the holdout root everything under $HOME is unreadable
    # inside the profile anyway (only the workdir, CODEX_HOME and auth.json are allowed).
    if HOLDOUT != w and HOLDOUT not in w.parents:
        return False
    chain = [w] + [p for p in w.parents if p == HOLDOUT or HOLDOUT in p.parents]
    for d in chain:
        for bad in (".codex", "AGENTS.md", "AGENTS.override.md", ".agents", "skills"):
            if (d / bad).exists():
                return False
    ch = codex_home().resolve()
    if HOLDOUT not in ch.parents:
        return False
    # CODEX_HOME may gain codex's own runtime files (sessions, logs, caches), but never
    # anything that configures the model's context or tools.
    forbidden = {"config.toml", "plugins", "skills", "memories", "AGENTS.md", "AGENTS.override.md",
                 "hooks.json", "prompts", "rules", ".agents", "packages"}
    if any((ch / f).exists() for f in forbidden):
        return False
    if not (ch / "auth.json").exists():
        return False
    try:
        v = subprocess.run([CODEX, "--version"], capture_output=True, text=True, timeout=30).stdout.strip()
    except (OSError, subprocess.TimeoutExpired):
        return False
    if v != PINNED_CODEX:
        return False
    return canary_denied(w)


def canary_denied(workdir):
    secret = f"C3-CANARY-{os.getpid()}-{time.time_ns()}"
    CANARY.write_text(secret + "\n")
    r = sandboxed(workdir, ["/bin/cat", str(CANARY)], capture_output=True, text=True, timeout=30)
    return r.returncode != 0 and secret not in (r.stdout + r.stderr)


def disable_args():
    out = []
    for f in DISABLE:
        out += ["--disable", f]
    return out


def run_codex(workdir, model="gpt-6.1-sol", effort="high", timeout=7200, pidfile=None):
    """Run one pointer-prompted codex job in workdir under the outer sandbox.

    Returns (exit_code, other_event_count). The driver never reads model text; it
    only counts event types in codex.jsonl.
    """
    w = Path(workdir).resolve()
    argv = [CODEX, "exec", "-m", model, "-c", f'model_reasoning_effort="{effort}"',
            "-c", 'web_search="disabled"', "-c", 'approval_policy="never"',
            "--sandbox", "danger-full-access", "--skip-git-repo-check", "--json",
            *disable_args(), "Read INSTRUCTIONS.md in the current directory and follow it."]
    env = {"HOME": str(Path.home()), "PATH": "/usr/bin:/bin:/opt/homebrew/bin",
           "CODEX_HOME": str(codex_home()), "TMPDIR": str(w), "LANG": "en_US.UTF-8",
           "RUSTC": str(TOOLCHAIN / "bin" / "rustc")}
    env.update({k: v for k, v in os.environ.items() if k.startswith("C3_TEST_")})  # test hooks only
    with open(w / "codex.jsonl", "wb") as out, open(w / "codex.err", "wb") as err:
        p = subprocess.Popen(["/usr/bin/sandbox-exec", "-f", str(write_profile(w)), *argv],
                             cwd=w, env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=err)
        if pidfile:
            with open(pidfile, "a") as f:
                f.write(f"{p.pid}\n")
        try:
            code = p.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            p.kill()
            p.wait()
            code = 124
    (w / "exit.txt").write_text(f"{code}\n")
    return code, count_other_events(w / "codex.jsonl")


def count_other_events(jsonl):
    """Count events outside the allowlist (unknown top-level types, disallowed item
    types, error items, unparsable lines). Content is never returned."""
    other = 0
    try:
        lines = Path(jsonl).read_text(errors="replace").splitlines()
    except OSError:
        return -1
    for line in lines:
        if not line.strip():
            continue
        try:
            e = json.loads(line)
        except ValueError:
            other += 1
            continue
        t = e.get("type")
        if t not in ALLOWED_TOP:
            other += 1
            continue
        item = e.get("item")
        if isinstance(item, dict) and item.get("type") not in ALLOWED_ITEMS:
            other += 1
    return other


def drained(pidfile):
    """True iff no pid recorded in pidfile is alive (kill -0)."""
    try:
        pids = [int(x) for x in Path(pidfile).read_text().split()]
    except OSError:
        return True
    for pid in pids:
        try:
            os.kill(pid, 0)
            return False
        except ProcessLookupError:
            continue
        except PermissionError:
            return False
    return True


def read_ids(path):
    ids = [x.strip() for x in Path(path).read_text().splitlines() if x.strip()]
    if not all(ID_RE.match(i) for i in ids):
        raise ValueError("bad id")
    return ids
