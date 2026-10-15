"""Isolated Codex homes for each evolution role."""

from __future__ import annotations

import shutil
from pathlib import Path

from bench.e1 import run as e1run, tasks

ROLES = {"agent", "confirm", "researcher"}
CONFIG_TEMPLATE = Path(__file__).with_name("codex-home.config.toml")


def ensure_role_home(role: str) -> Path:
    if role not in ROLES:
        raise ValueError("role must be agent, confirm, or researcher")
    home = tasks.e1_home() / f"codex-home-{role}"
    home.mkdir(parents=True, exist_ok=True)
    unexpected = [p.name for p in home.iterdir() if not p.name.startswith(e1run.CODEX_HOME_RUNTIME_PREFIXES)]
    skills = home / "skills"
    if skills.is_dir() and {p.name for p in skills.iterdir()} <= {".system"}:
        unexpected.remove("skills")
    if unexpected:
        raise RuntimeError(f"isolated CODEX_HOME contains ambient files: {sorted(unexpected)}")
    auth = home / "auth.json"
    user_auth = Path.home() / ".codex" / "auth.json"
    if not auth.is_symlink() or auth.resolve() != user_auth.resolve():
        auth.unlink(missing_ok=True)
        auth.symlink_to(user_auth)
    config = home / "config.toml"
    template = CONFIG_TEMPLATE.read_text()
    if not config.exists() or config.read_text() != template:
        config.write_text(template)
    return home


def purge_sessions(home: str | Path) -> None:
    root = Path(home)
    shutil.rmtree(root / "sessions", ignore_errors=True)
    for path in root.glob("history*"):
        if path.is_dir():
            shutil.rmtree(path, ignore_errors=True)
        else:
            path.unlink(missing_ok=True)
