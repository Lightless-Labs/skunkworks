"""Counts-only audit of compile-time file-read channels (Sol registration review, 10-21).

The validator's API-surface check compiled model-written Rust outside the sandbox, and rustc
can read files at compile time (include_str!, include_bytes!, include!, env!, option_env!).
This scans every readable file of the certified suite, the authoring tree and the step-5 audit
runs for those macros and for the other compile-time file loaders (`#[path = ...]`, `mod x;`),
and every readable file for sealed-path strings (the confirmation-2 decision). Rust is lexed
string-aware with line and nested block comments removed, so `include_str /* c */ !(...)` is
found (Sol second re-check); every text file also gets a permissive raw-text pattern that allows
whitespace and comments before the `!`. Both over-count rather than miss. It prints counts only
and writes runs/c2-confirm3/channel-audit.json, which the registration generator's certification requires to be all zero. No file content is printed.

usage: python3 -I tools/holdout/c3_channel_audit.py
"""
import collections
import hashlib
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from c3lib import C3, HOLDOUT, REPO, SUITE  # noqa: E402

MACRO = re.compile(r"\b(include_str|include_bytes|include|env|option_env)\s*!")
RAW_MACRO = re.compile(r"\b(include_str|include_bytes|include|env|option_env)\b(?:\s|/\*.*?\*/|//[^\n]*\n)*!", re.S)
LOADER = re.compile(r"#!?\[\s*path\s*=|\bmod\s+[^\s;{(]+\s*;")
# Aliased imports of the reading macros (`use std::include_str as read;`) and any bare mention
# of the unambiguous names count too (Sol third re-check); over-counting is the safe side.
ALIAS = re.compile(r"\buse\s+[^;]*::[^;]*\b(include_str|include_bytes|include|option_env)\b[^;]*;"
                   r"|\buse\s+[^;]*::\s*env\s+as\b[^;]*;")     # a Rust `use` has a `::` path; prose does not
MENTION = re.compile(r"\b(include_str|include_bytes|option_env)\b")


def strip_comments(src):
    """Rust source with comments removed (string, raw-string and char literals respected)."""
    out, i, n = [], 0, len(src)
    while i < n:
        c, nxt = src[i], src[i + 1] if i + 1 < n else ""
        if c == "/" and nxt == "/":
            j = src.find("\n", i)
            i = n if j < 0 else j
        elif c == "/" and nxt == "*":
            depth, i = 1, i + 2
            while i < n and depth:
                if src.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif src.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
            out.append(" ")
        elif c == "r" and re.match(r'r#*"', src[i:]):
            hashes = len(re.match(r"r(#*)", src[i:]).group(1))
            end = src.find('"' + "#" * hashes, i + 2 + hashes)
            end = n if end < 0 else end + 1 + hashes
            out.append(src[i:end])
            i = end
        elif c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            out.append(src[i:j + 1])
            i = j + 1
        elif c == "'" and re.match(r"'(\\.|[^\\'])'", src[i:i + 4] if i + 4 <= n else src[i:]):
            m = re.match(r"'(\\.|[^\\'])'", src[i:])
            out.append(m.group(0))
            i += len(m.group(0))
        else:
            out.append(c)
            i += 1
    return "".join(out)


SEALED = re.compile(r"c2-confirm2-decision|pooled-summary|decision\.json|sealed-sha256|\.a2d-holdout/runs/c2-confirm2")
OUT = REPO / "runs" / "c2-confirm3" / "channel-audit.json"


def scan(root):
    c = {"files": 0, "text_files": 0, "unreadable": 0, "macros": collections.Counter(), "files_with_macro": 0,
         "loaders": 0, "sealed_path_mentions": 0}
    for p in sorted(root.rglob("*")) if root.exists() else []:
        if not p.is_file() or p.is_symlink():
            continue
        c["files"] += 1
        try:
            text = p.read_text(errors="strict")
        except (UnicodeDecodeError, OSError):
            c["unreadable"] += 1
            continue
        c["text_files"] += 1
        code = strip_comments(text) if p.suffix == ".rs" else text
        found = [m.group(1) for m in MACRO.finditer(code)] + [m.group(1) for m in RAW_MACRO.finditer(text)]
        found += ["alias"] * len(ALIAS.findall(code)) + ["mention:" + m for m in MENTION.findall(code)]
        c["macros"].update(found)
        c["files_with_macro"] += bool(found)
        if p.suffix == ".rs":
            c["loaders"] += len(LOADER.findall(code))
        c["sealed_path_mentions"] += len(SEALED.findall(text))
    c["macros"] = dict(c["macros"])
    return c


def main():
    roots = {"suite": SUITE, "authoring": C3}
    roots.update({p.name: p for p in sorted((HOLDOUT / "runs").glob("c3-audit-*")) if p.is_dir()})
    result = {name: scan(root) for name, root in roots.items()}
    total = {"macros": sum(sum(r["macros"].values()) for r in result.values()),
             "loaders": sum(r["loaders"] for r in result.values()),
             "sealed_path_mentions": sum(r["sealed_path_mentions"] for r in result.values())}
    record = {"roots": result, "total": total,
              "clean": total["macros"] == 0 and total["loaders"] == 0 and total["sealed_path_mentions"] == 0,
              "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    OUT.write_text(json.dumps(record, indent=1) + "\n")
    print(json.dumps({"clean": record["clean"], "total": total,
                      "files": {k: v["files"] for k, v in result.items()}}))


if __name__ == "__main__":
    main()
