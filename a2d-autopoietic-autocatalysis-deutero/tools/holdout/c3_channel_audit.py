"""Counts-only audit of compile-time file-read channels (Sol registration review, 10-21).

The validator's API-surface check compiled model-written Rust outside the sandbox, and rustc
can read files at compile time (include_str!, include_bytes!, include!, env!, option_env!).
This scans every readable file of the certified suite, the authoring tree and the step-5 audit
runs for those macros, and every readable file for sealed-path strings (the confirmation-2
decision). It prints counts only and writes runs/c2-confirm3/channel-audit.json, which the
registration generator's certification requires to be all zero. No file content is printed.

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

MACRO = re.compile(r"\b(include_str|include_bytes|include|env|option_env)\s*!\s*[\(\[\{]")
SEALED = re.compile(r"c2-confirm2-decision|pooled-summary|decision\.json|sealed-sha256|\.a2d-holdout/runs/c2-confirm2")
OUT = REPO / "runs" / "c2-confirm3" / "channel-audit.json"


def scan(root):
    c = {"files": 0, "text_files": 0, "unreadable": 0, "macros": collections.Counter(), "files_with_macro": 0,
         "sealed_path_mentions": 0}
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
        found = [m.group(1) for m in MACRO.finditer(text)]
        c["macros"].update(found)
        c["files_with_macro"] += bool(found)
        c["sealed_path_mentions"] += len(SEALED.findall(text))
    c["macros"] = dict(c["macros"])
    return c


def main():
    roots = {"suite": SUITE, "authoring": C3}
    roots.update({p.name: p for p in sorted((HOLDOUT / "runs").glob("c3-audit-*")) if p.is_dir()})
    result = {name: scan(root) for name, root in roots.items()}
    total = {"macros": sum(sum(r["macros"].values()) for r in result.values()),
             "sealed_path_mentions": sum(r["sealed_path_mentions"] for r in result.values())}
    record = {"roots": result, "total": total, "clean": total["macros"] == 0 and total["sealed_path_mentions"] == 0,
              "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    OUT.write_text(json.dumps(record, indent=1) + "\n")
    print(json.dumps({"clean": record["clean"], "total": total,
                      "files": {k: v["files"] for k, v in result.items()}}))


if __name__ == "__main__":
    main()
