#!/usr/bin/env python3
"""Prepare an isolated researcher workspace and validate proposed genomes."""

from __future__ import annotations

import argparse

import hashlib
import json
import os
import re
import shutil
import sys
from pathlib import Path

E1_DIR = Path(__file__).resolve().parents[1] / "e1"
if str(E1_DIR) not in sys.path:
    sys.path.insert(0, str(E1_DIR))

from bench.e1 import grading, run as e1run, tasks
from bench.evolve import codex_homes, episode

REPO_ROOT = Path(__file__).resolve().parents[2]
PROGRAM = Path(__file__).with_name("program.md")
LOG_FIELDS = ("exp_id", "parent", "hypothesis", "decision", "reason", "delta_A", "delta_B",
              "p_B", "full44_delta", "tokens")


def build_workspace(exp_id: str, champion_genome: Path, log_rows: list[dict],
                    digests: list[dict], dest: Path) -> Path:
    dest = Path(dest)
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    shutil.copytree(champion_genome, dest / "genome", ignore=shutil.ignore_patterns(".git"))
    (dest / "genome" / "HYPOTHESIS.txt").unlink(missing_ok=True)
    shutil.copy2(PROGRAM, dest / "program.md")
    with (dest / "log.jsonl").open("w") as output:
        for row in log_rows:
            output.write(json.dumps({key: row[key] for key in LOG_FIELDS if key in row}, sort_keys=True) + "\n")
    digest_dir = dest / "digests"
    digest_dir.mkdir()
    for digest in digests:
        key = str(digest["key"])
        if not re.fullmatch(r"[0-9a-f]{10}", key):
            raise ValueError("digest key must be a 10-character lowercase hex string")
        (digest_dir / (key + ".json")).write_text(json.dumps(digest, sort_keys=True) + "\n")
    (dest / "README.md").write_text("Research workspace. Read program.md and follow its instructions exactly.\n")
    grading.init_history_free_repo(dest)
    return dest


def _safe_diff_stat(value):
    """Keep aggregate diff counts while preventing file paths from entering a digest."""
    if isinstance(value, dict):
        return {key: item for key, item in value.items()
                if key in ("files", "insertions", "deletions", "changed") and isinstance(item, (int, float))}
    if isinstance(value, (int, float)):
        return value
    if isinstance(value, str):
        numbers = re.findall(r"\d+", value)
        return {"counts": [int(number) for number in numbers]} if numbers else None
    if isinstance(value, list):
        return {"files": len(value)}
    return None


def make_digests(rows: list[dict], manifests: dict[str, dict], night_salt: str) -> list[dict]:
    """Produce one redacted digest per task, taking the last supplied episode row."""
    latest = {}
    for row in rows:
        opaque = row.get("opaque_id")
        if opaque is not None:
            latest[opaque] = row
    results = []
    for opaque, row in latest.items():
        key = hashlib.sha256((night_salt + str(opaque)).encode("utf-8")).hexdigest()[:10]
        grade = row.get("grade") or {}
        digest = {"key": key,
                  "problem_statement": (manifests.get(opaque) or {}).get("problem_statement", ""),
                  "resolved": bool(grade.get("resolved", row.get("resolved", False))),
                  "agent_calls": int(row.get("agent_calls") or 0)}
        if isinstance(row.get("verify_summaries"), list):
            digest["verify_summaries"] = [
                {"passed": bool(summary.get("passed")),
                 "failing": list(summary.get("failing") or [])}
                for summary in row["verify_summaries"] if isinstance(summary, dict)]
        if "diff_stat" in row:
            stat = _safe_diff_stat(row["diff_stat"])
            if stat is not None:
                digest["diff_stat"] = stat
        results.append(digest)
    return results


def _word_ngrams(text: str, n: int) -> set[str]:
    words = re.findall(r"\w+", text.lower(), flags=re.UNICODE)
    return {" ".join(words[index:index + n]) for index in range(max(0, len(words) - n + 1))}


def ngram_violations(genome_dir: Path, texts: list[str], n: int = 8) -> list[str]:
    if n < 1:
        raise ValueError("n must be positive")
    genome_grams = set()
    for path in sorted(Path(genome_dir).rglob("*")):
        if path.is_file() and path.name != "HYPOTHESIS.txt":
            try:
                genome_grams.update(_word_ngrams(path.read_text(errors="ignore"), n))
            except OSError:
                continue
    shared = set()
    for text in texts:
        shared.update(genome_grams.intersection(_word_ngrams(str(text), n)))
    return sorted(shared)


RESEARCHER_PROMPT = "Read program.md and README.md in the repository root, then follow program.md exactly."


def run_researcher(cfg, workspace: Path, exp_id: str, run_dir: Path) -> dict:
    """One sandboxed a2ctl agent call on the researcher workspace; `cfg` (the loop's) is not modified."""
    run_dir = Path(run_dir)
    run_dir.mkdir(parents=True, exist_ok=True)
    cfg = argparse.Namespace(**vars(cfg))
    cfg.endpoint = getattr(cfg, "endpoint", None) or ["https://chatgpt.com", "https://auth.openai.com"]
    cfg.provider = getattr(cfg, "provider", "codex/gpt-6-luna")
    cfg.max_tokens = getattr(cfg, "max_tokens", 2000000)
    cfg.a2ctl = getattr(cfg, "a2ctl", None) or e1run.REPO_ROOT / "target" / "debug" / "a2ctl"
    cfg.timeout = 1500
    cfg.reasoning_effort = "xhigh"
    cfg.codex_home = codex_homes.ensure_role_home("researcher")
    cfg.proxy_port = None
    proxy = None
    emit, log = run_dir / "researcher.jsonl", run_dir / "researcher.log"
    emit.unlink(missing_ok=True)
    payload = e1run.task_payload("researcher", RESEARCHER_PROMPT, "researcher-" + exp_id, cfg.endpoint)
    payload["verification_commands"] = []
    sealed_root = tasks.e1_home() / "sealed"
    try:
        proxy = e1run.start_proxy(cfg, run_dir, "researcher-" + exp_id)
        returncode = episode._run_agent_call(cfg, "researcher-" + exp_id, Path(workspace), payload,
                                             emit, log, sealed_root)
        rows = e1run.read_rows(emit) if emit.exists() else []
        final = rows[-1] if rows else {}
        return {"patch_diff": final.get("patch_diff", ""),
                "tokens_in": sum(int(row.get("tokens_in") or 0) for row in rows),
                "tokens_out": sum(int(row.get("tokens_out") or 0) for row in rows),
                "returncode": int(returncode)}
    finally:
        if proxy is not None:
            proxy.terminate()
        codex_homes.purge_sessions(cfg.codex_home)


def _parse_diff(diff: str) -> list[tuple[str, list[str]]]:
    """Parse text unified diff files into (target path, patch lines) pairs."""
    files = []
    current_path, current_lines = None, []
    for line in diff.splitlines(keepends=True):
        if line.startswith("diff --git "):
            if current_path is not None:
                files.append((current_path, current_lines))
            match = re.match(r"diff --git a/(.+?) b/(.+?)\s*$", line.rstrip("\n"))
            if not match or match.group(1) != match.group(2):
                return [("<rename-or-invalid>", [])]
            current_path, current_lines = match.group(2), []
        elif current_path is not None:
            current_lines.append(line)
    if current_path is not None:
        files.append((current_path, current_lines))
    return files


def _apply_file_patch(path: Path, lines: list[str]) -> None:
    if any(line.startswith("GIT binary patch") or line.startswith("Binary files ") for line in lines):
        raise ValueError("binary genome patches are not supported")
    new_file = any(line.startswith("new file mode ") for line in lines) or any(line.startswith("--- /dev/null") for line in lines)
    deleted = any(line.startswith("deleted file mode ") for line in lines) or any(line.startswith("+++ /dev/null") for line in lines)
    original = path.read_text().splitlines(keepends=True) if path.exists() else []
    output, cursor = [], 0
    hunks = [i for i, line in enumerate(lines) if line.startswith("@@ ")]
    if not hunks and not new_file and not deleted:
        raise ValueError("patch has no text hunks")
    for hunk_index, start in enumerate(hunks):
        end = hunks[hunk_index + 1] if hunk_index + 1 < len(hunks) else len(lines)
        match = re.match(r"@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@", lines[start])
        if not match:
            raise ValueError("malformed unified diff hunk")
        old_start = int(match.group(1))
        target = 0 if old_start == 0 else old_start - 1
        if target < cursor or target > len(original):
            raise ValueError("hunk offset is outside the candidate file")
        output.extend(original[cursor:target])
        cursor = target
        for patch_line in lines[start + 1:end]:
            if patch_line.startswith("\\ No newline"):
                continue
            marker, content = patch_line[:1], patch_line[1:]
            if marker == " ":
                if cursor >= len(original) or original[cursor].rstrip("\n") != content.rstrip("\n"):
                    raise ValueError("patch context does not match champion file")
                output.append(original[cursor]); cursor += 1
            elif marker == "-":
                if cursor >= len(original) or original[cursor].rstrip("\n") != content.rstrip("\n"):
                    raise ValueError("patch deletion does not match champion file")
                cursor += 1
            elif marker == "+":
                output.append(content)
            else:
                raise ValueError("unsupported unified diff line")
    output.extend(original[cursor:])
    if deleted:
        path.unlink(missing_ok=True)
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("".join(output))


def apply_candidate(champion_genome: Path, patch_diff: str, dest: Path) -> dict:
    champion_genome, dest = Path(champion_genome), Path(dest)
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(champion_genome, dest, ignore=shutil.ignore_patterns(".git"))
    (dest / "HYPOTHESIS.txt").unlink(missing_ok=True)
    files = _parse_diff(patch_diff)
    outside = [name for name, _lines in files if not name.startswith("genome/")]
    if outside:
        return {"ok": False, "hypothesis": "", "reason": "patch touches paths outside genome/: " + ", ".join(outside)}
    try:
        for name, lines in files:
            relative = name[len("genome/"):]
            relative_path = Path(relative)
            if (not relative or ".." in relative_path.parts or relative_path.is_absolute() or
                    (dest / relative_path).resolve().parent != dest.resolve() and
                    dest.resolve() not in (dest / relative_path).resolve().parents):
                raise ValueError("invalid genome patch path")
            _apply_file_patch(dest / relative_path, lines)
        hypothesis_path = dest / "HYPOTHESIS.txt"
        hypothesis = hypothesis_path.read_text().strip() if hypothesis_path.exists() else ""
        hypothesis_path.unlink(missing_ok=True)
        if not hypothesis or "\n" in hypothesis:
            return {"ok": False, "hypothesis": hypothesis, "reason": "HYPOTHESIS.txt must contain exactly one non-empty line"}
        return {"ok": True, "hypothesis": hypothesis, "reason": ""}
    except (OSError, ValueError) as exc:
        return {"ok": False, "hypothesis": "", "reason": str(exc)}


def _genome_digest(path: Path) -> str:
    digest = hashlib.sha256()
    for item in sorted(p for p in path.rglob("*") if p.is_file() and p.name != "HYPOTHESIS.txt"):
        digest.update(item.relative_to(path).as_posix().encode("utf-8"))
        digest.update(item.read_bytes())
    return digest.hexdigest()


def validate_candidate(candidate: Path, champion: Path, ngram_texts: list[str], replay_states: list[dict]) -> list[str]:
    reasons = []
    if _genome_digest(Path(candidate)) == _genome_digest(Path(champion)):
        reasons.append("candidate is unchanged from champion")
    violations = ngram_violations(Path(candidate), ngram_texts)
    if violations:
        reasons.append("n-gram scope violation: " + "; ".join(violations))
    for index, item in enumerate(replay_states):
        if isinstance(item, dict):
            state, snapshot = item.get("state", item), item.get("snapshot")
        else:
            state, snapshot = item
        if snapshot is None:
            reasons.append("replay {} has no base snapshot".format(index))
            continue
        stdout, stderr, code = episode.run_policy(Path(candidate), state, Path(snapshot))
        if code != 0:
            reasons.append("replay {} policy exited {}: {}".format(index, code, stderr[:300]))
            continue
        try:
            episode._parse_action(stdout)
        except (ValueError, TypeError, json.JSONDecodeError) as exc:
            reasons.append("replay {} returned invalid action: {}".format(index, exc))
    return reasons
