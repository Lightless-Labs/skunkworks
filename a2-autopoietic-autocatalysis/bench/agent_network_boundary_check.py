#!/usr/bin/env python3
"""Audit child-agent network-boundary prerequisites for benchmark integrity.

This check is not benchmark evidence and does not prove public internet/GitHub
reachability. It makes the current child-agent launch boundary reproducible:
where Pi subagent/Foundry child processes are spawned, whether the example Pi
sandbox hook exists, whether the sandbox runtime package is available on this
host, and whether actual child-agent launch functions pass their spawned command
through a sandbox wrapper or sandbox-exec. Use --require-sandbox-runtime when a
future benchmark path must fail closed unless both runtime availability and
launch-path enforcement are visible.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any

PI_PACKAGE = Path("/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent")
PI_SUBAGENT = PI_PACKAGE / "examples/extensions/subagent/index.ts"
PI_SANDBOX = PI_PACKAGE / "examples/extensions/sandbox/index.ts"
FOUNDRY_REPO = Path("/Users/thomas/.pi/agent/git/github.com/Lightless-Labs/foundry")
FOUNDRY_TEAM = FOUNDRY_REPO / "extensions/pi-foundry-team/index.ts"
REPO_ROOT = Path(__file__).resolve().parent.parent
A2_WORKTREE_CATALYST = REPO_ROOT / "crates/a2_workcell/src/worktree_catalyst.rs"
A2_GENERALIST_CATALYST = REPO_ROOT / "crates/a2_workcell/src/catalyst.rs"
A2_BROKER = REPO_ROOT / "crates/a2_broker/src/broker.rs"
HANDOFF_DOC = REPO_ROOT / "docs/HANDOFF.md"
SELF_CORRECTION_TODO = REPO_ROOT / "todos/self-correction-loop.md"
SANDBOX_RUNTIME_PACKAGE = "@anthropic-ai/sandbox-runtime"


def run(command: list[str], cwd: Path | None = None) -> dict[str, Any]:
    process = subprocess.run(command, cwd=cwd, text=True, capture_output=True, timeout=30)
    return {
        "command": command,
        "cwd": str(cwd) if cwd else None,
        "returncode": process.returncode,
        "stdout": process.stdout.strip(),
        "stderr": process.stderr.strip(),
    }


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def find_line(path: Path, needle: str) -> dict[str, Any]:
    if not path.exists():
        return {"path": str(path), "needle": needle, "found": False, "line": None, "text": None}
    for index, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if needle in line:
            return {
                "path": str(path),
                "needle": needle,
                "found": True,
                "line": index,
                "text": line.strip(),
            }
    return {"path": str(path), "needle": needle, "found": False, "line": None, "text": None}


def code_mask_preserving_offsets(text: str) -> str:
    output: list[str] = []
    index = 0
    in_block_comment = False
    in_line_comment = False
    in_string: str | None = None
    escaped = False

    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""

        if in_line_comment:
            if char == "\n":
                in_line_comment = False
                output.append(char)
            else:
                output.append(" ")
            index += 1
            continue
        if in_block_comment:
            if char == "*" and next_char == "/":
                in_block_comment = False
                output.extend("  ")
                index += 2
            else:
                output.append("\n" if char == "\n" else " ")
                index += 1
            continue
        if in_string is not None:
            output.append("\n" if char == "\n" else " ")
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == in_string:
                in_string = None
            index += 1
            continue

        if char in {"'", '"', "`"}:
            in_string = char
            output.append(" ")
            index += 1
            continue
        if char == "/" and next_char == "/":
            in_line_comment = True
            output.extend("  ")
            index += 2
            continue
        if char == "/" and next_char == "*":
            in_block_comment = True
            output.extend("  ")
            index += 2
            continue

        output.append(char)
        index += 1

    return "".join(output)


def advance_typescript_line_state(line: str, *, in_block_comment: bool, in_template: bool) -> tuple[bool, bool]:
    index = 0
    in_string: str | None = None
    escaped = False
    while index < len(line):
        char = line[index]
        next_char = line[index + 1] if index + 1 < len(line) else ""

        if in_block_comment:
            if char == "*" and next_char == "/":
                in_block_comment = False
                index += 2
            else:
                index += 1
            continue
        if in_template:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == "`":
                in_template = False
            index += 1
            continue
        if in_string is not None:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == in_string:
                in_string = None
            index += 1
            continue

        if char in {"'", '"'}:
            in_string = char
            index += 1
            continue
        if char == "`":
            in_template = True
            index += 1
            continue
        if char == "/" and next_char == "/":
            break
        if char == "/" and next_char == "*":
            in_block_comment = True
            index += 2
            continue
        if char == "/":
            index += 1
            regex_escaped = False
            while index < len(line):
                current = line[index]
                if regex_escaped:
                    regex_escaped = False
                elif current == "\\":
                    regex_escaped = True
                elif current == "/":
                    index += 1
                    while index < len(line) and line[index].isalpha():
                        index += 1
                    break
                index += 1
            continue
        index += 1
    return in_block_comment, in_template


def find_function_declaration_start(text: str, function_name: str) -> int | None:
    pattern = re.compile(rf"^\s*(?:export\s+)?(?:async\s+)?function\s+{re.escape(function_name)}\b")
    offset = 0
    in_block_comment = False
    in_template = False
    for line in text.splitlines(keepends=True):
        if not in_block_comment and not in_template and pattern.match(line):
            return offset
        in_block_comment, in_template = advance_typescript_line_state(
            line,
            in_block_comment=in_block_comment,
            in_template=in_template,
        )
        offset += len(line)
    return None


def extract_braced_body(text: str, start: int) -> str | None:
    brace_start = text.find("{", start)
    if brace_start < 0:
        return None

    depth = 0
    in_block_comment = False
    in_line_comment = False
    in_string: str | None = None
    escaped = False
    index = brace_start
    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""

        if in_line_comment:
            if char == "\n":
                in_line_comment = False
            index += 1
            continue
        if in_block_comment:
            if char == "*" and next_char == "/":
                in_block_comment = False
                index += 2
            else:
                index += 1
            continue
        if in_string is not None:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == in_string:
                in_string = None
            index += 1
            continue

        if char in {"'", '"', "`"}:
            in_string = char
            index += 1
            continue
        if char == "/" and next_char == "/":
            in_line_comment = True
            index += 2
            continue
        if char == "/" and next_char == "*":
            in_block_comment = True
            index += 2
            continue
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return text[brace_start + 1 : index]
        index += 1
    return None


def extract_function_body(text: str, function_name: str) -> str | None:
    start = find_function_declaration_start(text, function_name)
    if start is None:
        return None
    return extract_braced_body(text, start)


def mask_rust_non_code_preserving_offsets(text: str) -> str:
    output: list[str] = []
    index = 0
    block_comment_depth = 0
    in_line_comment = False
    in_string: str | None = None
    escaped = False
    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""

        if in_line_comment:
            if char == "\n":
                in_line_comment = False
                output.append(char)
            else:
                output.append(" ")
            index += 1
            continue
        if block_comment_depth > 0:
            if char == "/" and next_char == "*":
                block_comment_depth += 1
                output.extend("  ")
                index += 2
                continue
            if char == "*" and next_char == "/":
                block_comment_depth -= 1
                output.extend("  ")
                index += 2
                continue
            output.append("\n" if char == "\n" else " ")
            index += 1
            continue
        if in_string is not None:
            output.append("\n" if char == "\n" else " ")
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == in_string:
                in_string = None
            index += 1
            continue

        raw_end = rust_raw_string_end(text, index)
        if raw_end is not None:
            raw_text = text[index:raw_end]
            output.extend("\n" if current == "\n" else " " for current in raw_text)
            index = raw_end
            continue
        char_end = rust_char_literal_end(text, index)
        if char_end is not None:
            char_text = text[index:char_end]
            output.extend("\n" if current == "\n" else " " for current in char_text)
            index = char_end
            continue
        if char == '"':
            in_string = char
            output.append(" ")
            index += 1
            continue
        if char == "/" and next_char == "/":
            in_line_comment = True
            output.extend("  ")
            index += 2
            continue
        if char == "/" and next_char == "*":
            block_comment_depth = 1
            output.extend("  ")
            index += 2
            continue

        output.append(char)
        index += 1
    return "".join(output)


def find_rust_function_declaration_start(text: str, function_name: str) -> int | None:
    masked = mask_rust_non_code_preserving_offsets(text)
    pattern = re.compile(
        rf"^\s*(?:pub\s+)?(?:async\s+)?fn\s+{re.escape(function_name)}\b",
        re.MULTILINE,
    )
    match = pattern.search(masked)
    return match.start() if match else None


def extract_rust_braced_body(text: str, start: int) -> str | None:
    masked = mask_rust_non_code_preserving_offsets(text)
    brace_start = masked.find("{", start)
    if brace_start < 0:
        return None
    depth = 0
    for index in range(brace_start, len(masked)):
        char = masked[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return text[brace_start + 1 : index]
    return None


def extract_rust_function_body(text: str, function_name: str) -> str | None:
    start = find_rust_function_declaration_start(text, function_name)
    if start is None:
        return None
    return extract_rust_braced_body(text, start)


def strip_typescript_comments(text: str, *, strip_strings: bool = False) -> str:
    output: list[str] = []
    index = 0
    in_block_comment = False
    in_line_comment = False
    in_string: str | None = None
    escaped = False

    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""

        if in_line_comment:
            if char == "\n":
                in_line_comment = False
                output.append(char)
            index += 1
            continue

        if in_block_comment:
            if char == "*" and next_char == "/":
                in_block_comment = False
                index += 2
            else:
                if char == "\n":
                    output.append(char)
                index += 1
            continue

        if in_string is not None:
            if not strip_strings:
                output.append(char)
            elif char == "\n":
                output.append(char)
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == in_string:
                in_string = None
            index += 1
            continue

        if char in {"'", '"', "`"}:
            in_string = char
            if not strip_strings:
                output.append(char)
            index += 1
            continue
        if char == "/" and next_char == "/":
            in_line_comment = True
            index += 2
            continue
        if char == "/" and next_char == "*":
            in_block_comment = True
            index += 2
            continue

        output.append(char)
        index += 1

    return "".join(output)


def previous_significant_char(text: str, offset: int) -> str | None:
    index = offset - 1
    while index >= 0 and text[index].isspace():
        index -= 1
    return text[index] if index >= 0 else None


def regex_literal_may_start_after(char: str | None) -> bool:
    return char is None or char in "=({[,;:!&|?+-*~^<>"


def mask_typescript_regex_literals_preserving_offsets(text: str) -> str:
    output = list(text)
    index = 0
    in_string: str | None = None
    escaped = False
    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""
        if in_string is not None:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == in_string:
                in_string = None
            index += 1
            continue
        if char in {"'", '"', "`"}:
            in_string = char
            index += 1
            continue
        if char == "/" and next_char not in {"/", "*"} and regex_literal_may_start_after(previous_significant_char(text, index)):
            start = index
            index += 1
            regex_escaped = False
            in_character_class = False
            while index < len(text):
                current = text[index]
                if regex_escaped:
                    regex_escaped = False
                elif current == "\\":
                    regex_escaped = True
                elif current == "[":
                    in_character_class = True
                elif current == "]":
                    in_character_class = False
                elif current == "/" and not in_character_class:
                    index += 1
                    while index < len(text) and text[index].isalpha():
                        index += 1
                    for masked_index in range(start, index):
                        output[masked_index] = "\n" if text[masked_index] == "\n" else " "
                    break
                elif current == "\n":
                    break
                index += 1
            continue
        index += 1
    return "".join(output)


def wrapped_invocation_names(executable_body_without_strings: str) -> list[str]:
    assignments = re.finditer(
        r"(?:(?:const|let|var)\s+)?([A-Za-z_$][\w$]*)\s*=\s*(?:await\s+)?SandboxManager\.wrapWithSandbox\s*\(",
        executable_body_without_strings,
    )
    return [match.group(1) for match in assignments]


def string_literal_ranges(text: str) -> list[tuple[int, int]]:
    ranges: list[tuple[int, int]] = []
    index = 0
    while index < len(text):
        char = text[index]
        if char not in {"'", '"', "`"}:
            index += 1
            continue
        quote = char
        start = index
        index += 1
        escaped = False
        while index < len(text):
            current = text[index]
            if escaped:
                escaped = False
            elif current == "\\":
                escaped = True
            elif current == quote:
                index += 1
                break
            index += 1
        ranges.append((start, index))
    return ranges


def offset_inside_ranges(offset: int, ranges: list[tuple[int, int]]) -> bool:
    return any(start <= offset < end for start, end in ranges)


def extract_call_at(text: str, open_paren: int) -> str | None:
    depth = 0
    in_string: str | None = None
    escaped = False
    for index in range(open_paren, len(text)):
        char = text[index]
        if in_string is not None:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == in_string:
                in_string = None
            continue
        if char in {"'", '"', "`"}:
            in_string = char
            continue
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return text[open_paren : index + 1]
    return None


def spawn_calls(executable_body_with_strings: str) -> list[str]:
    ranges = string_literal_ranges(executable_body_with_strings)
    calls: list[str] = []
    for match in re.finditer(r"\bspawn\s*\(", executable_body_with_strings):
        if offset_inside_ranges(match.start(), ranges):
            continue
        open_paren = executable_body_with_strings.find("(", match.start())
        call = extract_call_at(executable_body_with_strings, open_paren)
        if call is not None:
            calls.append(call)
    return calls


def sandbox_exec_constant_names(executable_body_with_strings: str) -> list[str]:
    ranges = string_literal_ranges(executable_body_with_strings)
    names: list[str] = []
    for match in re.finditer(
        r"(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(['\"`])sandbox-exec\2",
        executable_body_with_strings,
    ):
        if not offset_inside_ranges(match.start(), ranges):
            names.append(match.group(1))
    return names


def spawn_call_uses_wrapped_invocation(spawn_call_without_strings: str, invocation_name: str) -> bool:
    escaped_name = re.escape(invocation_name)
    return (
        re.search(
            rf"^\(\s*{escaped_name}\.command\s*,\s*{escaped_name}\.args\b",
            spawn_call_without_strings,
        )
        is not None
    )


def spawn_call_uses_sandbox_exec(spawn_call_with_strings: str) -> bool:
    return re.search(r"^\(\s*(['\"`])sandbox-exec\1\s*,", spawn_call_with_strings) is not None


def spawn_call_uses_sandbox_exec_constant(spawn_call_without_strings: str, constant_names: list[str]) -> bool:
    return any(
        re.search(rf"^\(\s*{re.escape(name)}\s*,", spawn_call_without_strings) is not None
        for name in constant_names
    )


def sandbox_exec_argv_text(spawn_call_without_strings: str) -> str | None:
    first_comma = spawn_call_without_strings.find(",")
    if first_comma < 0:
        return None
    bracket_start = spawn_call_without_strings.find("[", first_comma)
    if bracket_start < 0:
        return None
    depth = 0
    for index in range(bracket_start, len(spawn_call_without_strings)):
        char = spawn_call_without_strings[index]
        if char == "[":
            depth += 1
        elif char == "]":
            depth -= 1
            if depth == 0:
                return spawn_call_without_strings[bracket_start + 1 : index]
    return None


def spawn_call_carries_child_invocation(spawn_call_without_strings: str, wrapped_names: list[str]) -> bool:
    argv_text = sandbox_exec_argv_text(spawn_call_without_strings)
    if argv_text is None:
        return False
    command_arg_pairs = ["invocation", *wrapped_names]
    for name in command_arg_pairs:
        command_index = argv_text.find(f"{name}.command")
        args_index = argv_text.find(f"{name}.args")
        if command_index >= 0 and command_index < args_index:
            return True
    return False


def classify_spawn_call(
    spawn_call_with_strings: str,
    wrapped_names: list[str],
    sandbox_exec_constants: list[str],
) -> str | None:
    spawn_call_without_strings = strip_typescript_comments(spawn_call_with_strings, strip_strings=True)
    for name in wrapped_names:
        if spawn_call_uses_wrapped_invocation(spawn_call_without_strings, name):
            return f"spawn({name}.command, {name}.args) from SandboxManager.wrapWithSandbox"
    if (
        spawn_call_uses_sandbox_exec(spawn_call_with_strings)
        or spawn_call_uses_sandbox_exec_constant(spawn_call_without_strings, sandbox_exec_constants)
    ) and spawn_call_carries_child_invocation(spawn_call_without_strings, wrapped_names):
        return "spawn(sandbox-exec, ... child command/args ...)"
    return None


def rust_raw_string_end(text: str, start: int) -> int | None:
    index = start
    if index < len(text) and text[index] == "b":
        index += 1
    if index >= len(text) or text[index] != "r":
        return None
    index += 1
    hashes = 0
    while index < len(text) and text[index] == "#":
        hashes += 1
        index += 1
    if index >= len(text) or text[index] != '"':
        return None
    index += 1
    terminator = '"' + ("#" * hashes)
    end = text.find(terminator, index)
    if end < 0:
        return len(text)
    return end + len(terminator)


def rust_char_literal_end(text: str, start: int) -> int | None:
    index = start
    if index < len(text) and text[index] == "b" and index + 1 < len(text) and text[index + 1] == "'":
        index += 1
    if index >= len(text) or text[index] != "'":
        return None
    content = index + 1
    if content >= len(text) or text[content] == "\n":
        return None
    if text[content].isalpha() or text[content] == "_":
        if content + 1 < len(text) and text[content + 1] == "'":
            return content + 2
        return None
    if text[content] == "\\":
        cursor = content + 1
        if cursor < len(text) and text[cursor] == "u" and cursor + 1 < len(text) and text[cursor + 1] == "{":
            end_brace = text.find("}", cursor + 2)
            if end_brace < 0:
                return None
            cursor = end_brace + 1
        else:
            cursor += 1
    else:
        cursor = content + 1
    if cursor < len(text) and text[cursor] == "'":
        return cursor + 1
    return None


def rust_string_literal_ranges(text: str) -> list[tuple[int, int]]:
    ranges: list[tuple[int, int]] = []
    index = 0
    while index < len(text):
        raw_end = rust_raw_string_end(text, index)
        if raw_end is not None:
            ranges.append((index, raw_end))
            index = raw_end
            continue
        char_end = rust_char_literal_end(text, index)
        if char_end is not None:
            ranges.append((index, char_end))
            index = char_end
            continue
        char = text[index]
        if char != '"':
            index += 1
            continue
        start = index
        index += 1
        escaped = False
        while index < len(text):
            current = text[index]
            if escaped:
                escaped = False
            elif current == "\\":
                escaped = True
            elif current == '"':
                index += 1
                break
            index += 1
        ranges.append((start, index))
    return ranges


def mask_rust_comments_preserving_offsets(text: str) -> str:
    output: list[str] = []
    index = 0
    block_comment_depth = 0
    in_line_comment = False
    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""

        if in_line_comment:
            if char == "\n":
                in_line_comment = False
                output.append(char)
            else:
                output.append(" ")
            index += 1
            continue
        if block_comment_depth > 0:
            if char == "/" and next_char == "*":
                block_comment_depth += 1
                output.extend("  ")
                index += 2
                continue
            if char == "*" and next_char == "/":
                block_comment_depth -= 1
                output.extend("  ")
                index += 2
                continue
            output.append("\n" if char == "\n" else " ")
            index += 1
            continue

        raw_end = rust_raw_string_end(text, index)
        if raw_end is not None:
            output.append(text[index:raw_end])
            index = raw_end
            continue
        char_end = rust_char_literal_end(text, index)
        if char_end is not None:
            output.append(text[index:char_end])
            index = char_end
            continue
        if char == '"':
            start = index
            index += 1
            escaped = False
            while index < len(text):
                current = text[index]
                if escaped:
                    escaped = False
                elif current == "\\":
                    escaped = True
                elif current == '"':
                    index += 1
                    break
                index += 1
            output.append(text[start:index])
            continue
        if char == "/" and next_char == "/":
            in_line_comment = True
            output.extend("  ")
            index += 2
            continue
        if char == "/" and next_char == "*":
            block_comment_depth = 1
            output.extend("  ")
            index += 2
            continue

        output.append(char)
        index += 1
    return "".join(output)


def strip_rust_comments(text: str, *, strip_strings: bool = False) -> str:
    output: list[str] = []
    index = 0
    block_comment_depth = 0
    in_line_comment = False
    in_string: str | None = None
    escaped = False
    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""

        if in_line_comment:
            if char == "\n":
                in_line_comment = False
                output.append(char)
            index += 1
            continue
        if block_comment_depth > 0:
            if char == "/" and next_char == "*":
                block_comment_depth += 1
                index += 2
                continue
            if char == "*" and next_char == "/":
                block_comment_depth -= 1
                index += 2
                continue
            if char == "\n":
                output.append(char)
            index += 1
            continue
        if in_string is not None:
            output.append("\n" if char == "\n" else (" " if strip_strings else char))
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == in_string:
                in_string = None
            index += 1
            continue

        raw_end = rust_raw_string_end(text, index)
        if raw_end is not None:
            raw_text = text[index:raw_end]
            if strip_strings:
                output.extend("\n" if current == "\n" else " " for current in raw_text)
            else:
                output.append(raw_text)
            index = raw_end
            continue
        char_end = rust_char_literal_end(text, index)
        if char_end is not None:
            char_text = text[index:char_end]
            if strip_strings:
                output.extend("\n" if current == "\n" else " " for current in char_text)
            else:
                output.append(char_text)
            index = char_end
            continue
        if char == '"':
            in_string = char
            output.append(" " if strip_strings else char)
            index += 1
            continue
        if char == "/" and next_char == "/":
            in_line_comment = True
            index += 2
            continue
        if char == "/" and next_char == "*":
            block_comment_depth = 1
            index += 2
            continue

        output.append(char)
        index += 1
    return "".join(output)


def rust_executable_body_without_strings(body: str) -> str:
    return strip_rust_comments(body, strip_strings=True)


def rust_command_new_offsets(body_without_strings: str) -> list[int]:
    return [match.start() for match in re.finditer(r"\bCommand::new\s*\(", body_without_strings)]


def code_regex_match_offsets(text: str, pattern: str, flags: int = 0) -> list[int]:
    ranges = string_literal_ranges(text)
    return [
        match.start()
        for match in re.finditer(pattern, text, flags)
        if not offset_inside_ranges(match.start(), ranges)
    ]


def first_code_regex_match_offset(text: str, pattern: str, flags: int = 0) -> int:
    offsets = code_regex_match_offsets(text, pattern, flags)
    return offsets[0] if offsets else -1


def rust_code_regex_match_offsets(text: str, pattern: str, flags: int = 0) -> list[int]:
    ranges = rust_string_literal_ranges(text)
    return [
        match.start()
        for match in re.finditer(pattern, text, flags)
        if not offset_inside_ranges(match.start(), ranges)
    ]


def first_rust_code_regex_match_offset(text: str, pattern: str, flags: int = 0) -> int:
    offsets = rust_code_regex_match_offsets(text, pattern, flags)
    return offsets[0] if offsets else -1


def rust_brace_depth_at(text: str, offset: int) -> int:
    masked = mask_rust_non_code_preserving_offsets(text)
    depth = 0
    for char in masked[:offset]:
        if char == "{":
            depth += 1
        elif char == "}":
            depth = max(0, depth - 1)
    return depth


def top_level_rust_code_regex_match_offsets(text: str, pattern: str, flags: int = 0) -> list[int]:
    comments_masked = mask_rust_comments_preserving_offsets(text)
    return [
        offset
        for offset in rust_code_regex_match_offsets(comments_masked, pattern, flags)
        if rust_brace_depth_at(text, offset) == 0
    ]


def first_top_level_rust_code_regex_match_offset(text: str, pattern: str, flags: int = 0) -> int:
    offsets = top_level_rust_code_regex_match_offsets(text, pattern, flags)
    return offsets[0] if offsets else -1


def extract_rust_braced_body_span(text: str, start: int) -> tuple[str, int, int] | None:
    masked = mask_rust_non_code_preserving_offsets(text)
    brace_start = masked.find("{", start)
    if brace_start < 0:
        return None
    depth = 0
    for index in range(brace_start, len(masked)):
        char = masked[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return (text[brace_start + 1 : index], brace_start, index)
    return None


def has_top_level_return_err(block: str) -> bool:
    return bool(top_level_rust_code_regex_match_offsets(block, r"\breturn\s+Err\b"))


def find_restricted_policy_return_err_span(executable: str) -> tuple[int, int] | None:
    policy_pattern = (
        r"NetworkPolicy::Isolated\s*\|\s*NetworkPolicy::AllowList\s*\(\s*_\s*\)"
    )
    patterns = [
        re.compile(
            r"\bif\s+matches!\s*\(\s*(?:&\s*)?(?:network_policy|task\.network_policy)\s*,\s*"
            rf"Some\s*\(\s*{policy_pattern}\s*\)\s*\)",
            re.MULTILINE | re.DOTALL,
        ),
        re.compile(
            rf"\bif\s+let\s+Some\s*\(\s*(?:\w+\s*@\s*)?\(\s*{policy_pattern}\s*\)\s*\)\s*=\s*(?:&\s*)?(?:network_policy|task\.network_policy)\b",
            re.MULTILINE | re.DOTALL,
        ),
    ]
    for pattern in patterns:
        for match in pattern.finditer(executable):
            span = extract_rust_braced_body_span(executable, match.start())
            if span is None:
                continue
            block, block_start, block_end = span
            return_offsets = top_level_rust_code_regex_match_offsets(block, r"\breturn\s+Err\b")
            if return_offsets:
                return (block_start + 1 + return_offsets[0], block_end)
    return None


def helper_rejects_policy_with_return(helper: str, condition_pattern: str) -> bool:
    helper_code = mask_rust_comments_preserving_offsets(helper)
    condition_offsets = rust_code_regex_match_offsets(helper_code, condition_pattern, re.MULTILINE | re.DOTALL)
    if not condition_offsets:
        return False
    for if_match in re.finditer(r"\bif\b", helper_code):
        span = extract_rust_braced_body_span(helper_code, if_match.start())
        if span is None:
            continue
        block, block_start, _block_end = span
        if has_top_level_return_err(block) and any(if_match.start() < condition < block_start for condition in condition_offsets):
            return True
    return False


def audit_a2_worktree_catalyst_launch_gate(path: Path = A2_WORKTREE_CATALYST) -> dict[str, Any]:
    result: dict[str, Any] = {
        "path": str(path),
        "run_agent_found": False,
        "restricted_policy_check_present": False,
        "restricted_policy_error_before_mock": False,
        "restricted_policy_error_before_provider_dispatch": False,
        "provider_command_launch_functions": [],
        "sandbox_exec_launch_wrapper_present": False,
        "fail_closed_before_provider_launch": False,
        "reason": None,
    }
    if not path.exists():
        result["reason"] = "file_not_found"
        return result
    text = path.read_text(encoding="utf-8")
    body = extract_rust_function_body(text, "run_agent")
    if body is None:
        result["reason"] = "run_agent_not_found"
        return result
    result["run_agent_found"] = True
    executable = rust_executable_body_without_strings(body)
    restricted_return_span = find_restricted_policy_return_err_span(executable)
    error_index = restricted_return_span[0] if restricted_return_span is not None else -1
    mock_index = executable.find("run_mock_agent")
    dispatch_index = executable.find("match provider_id")
    result["restricted_policy_check_present"] = restricted_return_span is not None
    result["restricted_policy_error_before_mock"] = (
        result["restricted_policy_check_present"] and mock_index >= 0 and error_index < mock_index
    )
    result["restricted_policy_error_before_provider_dispatch"] = (
        result["restricted_policy_check_present"] and dispatch_index >= 0 and error_index < dispatch_index
    )

    provider_functions = ["run_claude", "run_codex", "run_gemini", "run_opencode", "run_pi"]
    launches: list[dict[str, Any]] = []
    for function_name in provider_functions:
        provider_body = extract_rust_function_body(text, function_name)
        if provider_body is None:
            launches.append({"function": function_name, "found": False, "command_new_present": False})
            continue
        provider_executable = rust_executable_body_without_strings(provider_body)
        command_new_present = bool(rust_command_new_offsets(provider_executable))
        first_command_index = first_top_level_rust_code_regex_match_offset(provider_body, r"\bCommand::new\s*\(")
        sandbox_command_index = first_top_level_rust_code_regex_match_offset(
            provider_body,
            r"\bCommand::new\s*\(\s*\"sandbox-exec\"",
        )
        launches.append(
            {
                "function": function_name,
                "found": True,
                "command_new_present": command_new_present,
                "sandbox_exec_present": sandbox_command_index >= 0 and sandbox_command_index == first_command_index,
            }
        )
    result["provider_command_launch_functions"] = launches
    provider_launches_present = all(item.get("found") and item.get("command_new_present") for item in launches)
    result["sandbox_exec_launch_wrapper_present"] = provider_launches_present and all(
        item.get("sandbox_exec_present") for item in launches
    )
    result["fail_closed_before_provider_launch"] = (
        result["restricted_policy_error_before_mock"]
        and result["restricted_policy_error_before_provider_dispatch"]
        and provider_launches_present
    )
    if not result["fail_closed_before_provider_launch"]:
        result["reason"] = "restricted policies are not visibly refused before every provider launch path"
    return result


def extract_rust_trait_impl_body(text: str, trait_name: str, impl_name: str) -> str | None:
    masked = mask_rust_non_code_preserving_offsets(text)
    pattern = re.compile(
        rf"^\s*impl\s*(?:<[^>{{}}]*>\s*)?{re.escape(trait_name)}\s+for\s+{re.escape(impl_name)}\b",
        re.MULTILINE,
    )
    match = pattern.search(masked)
    if match is None:
        return None
    return extract_rust_braced_body(text, match.start())


def extract_rust_impl_body(text: str, impl_name: str) -> str | None:
    return extract_rust_trait_impl_body(text, "ModelProvider", impl_name)


def audit_a2_generalist_catalyst_launch_gate(path: Path = A2_GENERALIST_CATALYST) -> dict[str, Any]:
    result: dict[str, Any] = {
        "path": str(path),
        "execute_found": False,
        "restricted_policy_check_present": False,
        "restricted_policy_error_before_provider_call": False,
        "provider_generate_call_present": False,
        "sandbox_exec_launch_wrapper_present": False,
        "fail_closed_before_provider_launch": False,
        "reason": None,
    }
    if not path.exists():
        result["reason"] = "file_not_found"
        return result
    text = path.read_text(encoding="utf-8")
    impl_body = extract_rust_trait_impl_body(text, "Catalyst", "GeneralistCatalyst")
    if impl_body is None:
        result["reason"] = "generalist_catalyst_impl_not_found"
        return result
    body = extract_rust_function_body(impl_body, "execute")
    if body is None:
        result["reason"] = "execute_not_found"
        return result
    result["execute_found"] = True
    executable = rust_executable_body_without_strings(body)
    restricted_return_span = find_restricted_policy_return_err_span(executable)
    error_index = restricted_return_span[0] if restricted_return_span is not None else -1
    provider_call_index = first_top_level_rust_code_regex_match_offset(
        body,
        r"\bmodel\s*\.\s*generate\s*\(",
    )
    result["restricted_policy_check_present"] = restricted_return_span is not None
    result["provider_generate_call_present"] = provider_call_index >= 0
    result["restricted_policy_error_before_provider_call"] = (
        result["restricted_policy_check_present"]
        and result["provider_generate_call_present"]
        and error_index < provider_call_index
    )
    result["fail_closed_before_provider_launch"] = result["restricted_policy_error_before_provider_call"]
    if not result["fail_closed_before_provider_launch"]:
        result["reason"] = "restricted policies are not visibly refused before model.generate"
    return result


def audit_a2_broker_launch_gate(path: Path = A2_BROKER) -> dict[str, Any]:
    result: dict[str, Any] = {
        "path": str(path),
        "policy_helper_found": False,
        "policy_helper_rejects_isolated": False,
        "policy_helper_rejects_allowlist": False,
        "provider_generate_guards": [],
        "sandbox_exec_launch_wrapper_present": False,
        "fail_closed_before_provider_launch": False,
        "reason": None,
    }
    if not path.exists():
        result["reason"] = "file_not_found"
        return result
    text = path.read_text(encoding="utf-8")
    helper = extract_rust_function_body(text, "fail_if_provider_network_restricted_for_policy")
    if helper is None:
        result["reason"] = "policy_helper_not_found"
        return result
    result["policy_helper_found"] = True
    result["policy_helper_rejects_isolated"] = helper_rejects_policy_with_return(
        helper,
        r"\b[a-zA-Z_][\w]*\s*==\s*\"isolated\"",
    )
    result["policy_helper_rejects_allowlist"] = helper_rejects_policy_with_return(
        helper,
        r"\b[a-zA-Z_][\w]*\.starts_with\s*\(\s*\"allowlist\"\s*\)",
    )

    providers = [
        ("claude", "ClaudeProvider"),
        ("gemini", "GeminiProvider"),
        ("codex", "CodexProvider"),
        ("pi", "PiProvider"),
        ("opencode", "OpenCodeProvider"),
    ]
    guards: list[dict[str, Any]] = []
    for provider, impl_name in providers:
        impl_body = extract_rust_impl_body(text, impl_name)
        generate_body = extract_rust_function_body(impl_body, "generate") if impl_body is not None else None
        if generate_body is None:
            guards.append(
                {
                    "provider": provider,
                    "impl": impl_name,
                    "generate_found": False,
                    "guard_found": False,
                    "command_new_after_guard": False,
                    "guard_before_command_new": False,
                }
            )
            continue
        guard_statement = re.escape(f'fail_if_provider_network_restricted("{provider}")?;')
        guard_pattern = rf"^\s*{guard_statement}"
        guard_index = first_top_level_rust_code_regex_match_offset(generate_body, guard_pattern, re.MULTILINE)
        command_index = first_top_level_rust_code_regex_match_offset(generate_body, r"\bCommand::new\s*\(")
        sandbox_command_index = first_top_level_rust_code_regex_match_offset(
            generate_body,
            r"\bCommand::new\s*\(\s*\"sandbox-exec\"",
        )
        sandbox_exec_present = sandbox_command_index >= 0 and sandbox_command_index == command_index
        guards.append(
            {
                "provider": provider,
                "impl": impl_name,
                "generate_found": True,
                "guard_found": guard_index >= 0,
                "command_new_after_guard": command_index >= 0,
                "guard_before_command_new": guard_index >= 0 and command_index >= 0 and guard_index < command_index,
                "sandbox_exec_present": sandbox_exec_present,
            }
        )
    result["provider_generate_guards"] = guards
    result["sandbox_exec_launch_wrapper_present"] = bool(guards) and all(
        item.get("sandbox_exec_present") for item in guards
    )
    result["fail_closed_before_provider_launch"] = (
        result["policy_helper_rejects_isolated"]
        and result["policy_helper_rejects_allowlist"]
        and all(item["guard_before_command_new"] for item in guards)
    )
    if not result["fail_closed_before_provider_launch"]:
        result["reason"] = "provider wrappers do not visibly reject restricted network policy before Command::new"
    return result


def a2_owned_sandbox_enforced_for_all_provider_surfaces(
    worktree: dict[str, Any],
    generalist: dict[str, Any],
    broker: dict[str, Any],
) -> bool:
    return bool(
        worktree["fail_closed_before_provider_launch"]
        and generalist["fail_closed_before_provider_launch"]
        and broker["fail_closed_before_provider_launch"]
        and worktree["sandbox_exec_launch_wrapper_present"]
        and broker["sandbox_exec_launch_wrapper_present"]
    )


def audit_a2_owned_provider_launch_boundaries() -> dict[str, Any]:
    worktree = audit_a2_worktree_catalyst_launch_gate()
    generalist = audit_a2_generalist_catalyst_launch_gate()
    broker = audit_a2_broker_launch_gate()
    fail_closed = bool(
        worktree["fail_closed_before_provider_launch"]
        and generalist["fail_closed_before_provider_launch"]
        and broker["fail_closed_before_provider_launch"]
    )
    sandbox_enforced = a2_owned_sandbox_enforced_for_all_provider_surfaces(worktree, generalist, broker)
    return {
        "worktree_catalyst": worktree,
        "generalist_catalyst": generalist,
        "broker": broker,
        "fail_closed_restricted_policies": fail_closed,
        "sandbox_enforced_for_restricted_policies": sandbox_enforced,
        "interpretation": (
            "A2-owned provider launch paths visibly fail closed before provider launch for restricted policies, but do not show a sandbox/provider allowlist wrapper at every provider command boundary yet."
            if fail_closed and not sandbox_enforced
            else "A2-owned provider launch path policy enforcement needs review before restricted-policy benchmark evidence is trusted."
        ),
    }


def find_function_sandbox_enforcement(path: Path, function_name: str) -> dict[str, Any]:
    result: dict[str, Any] = {
        "path": str(path),
        "function": function_name,
        "found": False,
        "spawn_present": False,
        "sandbox_markers": [],
        "unaccounted_spawns": [],
        "reason": None,
    }
    if not path.exists():
        result["reason"] = "file_not_found"
        return result

    body = extract_function_body(path.read_text(encoding="utf-8"), function_name)
    if body is None:
        result["reason"] = "function_not_found"
        return result

    executable_body_with_strings = mask_typescript_regex_literals_preserving_offsets(
        strip_typescript_comments(body, strip_strings=False)
    )
    executable_body_without_strings = mask_typescript_regex_literals_preserving_offsets(
        strip_typescript_comments(body, strip_strings=True)
    )
    wrapped_names = wrapped_invocation_names(executable_body_without_strings)
    sandbox_exec_constants = sandbox_exec_constant_names(executable_body_with_strings)
    calls = spawn_calls(executable_body_with_strings)
    marker_hits: list[str] = []
    unaccounted: list[str] = []
    for call in calls:
        marker = classify_spawn_call(call, wrapped_names, sandbox_exec_constants)
        if marker is None:
            unaccounted.append(call.strip())
        else:
            marker_hits.append(marker)

    result["spawn_present"] = bool(calls)
    result["sandbox_markers"] = marker_hits
    result["unaccounted_spawns"] = unaccounted
    if not calls:
        result["reason"] = "spawn_not_found_in_function"
    elif unaccounted:
        result["reason"] = "unaccounted_spawn_not_connected_to_sandbox"
    elif not marker_hits:
        result["reason"] = "sandbox_wrapper_not_connected_to_spawn"
    else:
        result["found"] = True
    return result


def npm_global_package_path(package: str) -> dict[str, Any]:
    root = run(["npm", "root", "-g"])
    if root["returncode"] != 0 or not root["stdout"]:
        return {"available": False, "npm_root": root, "path": None, "package_json": None}
    package_path = Path(root["stdout"]) / package
    package_json = package_path / "package.json"
    if package_json.exists():
        version = read_json(package_json).get("version")
        return {
            "available": True,
            "npm_root": root["stdout"],
            "path": str(package_path),
            "package_json": str(package_json),
            "version": version,
        }
    return {
        "available": False,
        "npm_root": root["stdout"],
        "path": str(package_path),
        "package_json": str(package_json),
    }


def _claim_is_negated(text: str, start: int, end: int) -> bool:
    clause_start = max(text.rfind(separator, 0, start) for separator in "\n.;:") + 1
    match_text = text[start:end].lower()
    prefix = text[clause_start:start].lower()
    explicit_match_negation_patterns = [
        r"\bnot\s+sandbox[- ]enforced\b",
        r"\bnot\s+(?:usable|implemented)\b",
        r"\bdoes\s+not\s+(?:enforce|protect|cover)\b",
        r"\bdo\s+not\s+(?:enforce|protect|cover)\b",
        r"\b(?:cannot|can't|can\s+not)\s+(?:enforce|protect|cover)\b",
    ]
    if any(re.search(pattern, match_text) for pattern in explicit_match_negation_patterns):
        return True
    prefix_negates_claim_patterns = [
        r"\b(?:cannot|can't|can\s+not|do\s+not|don't|does\s+not|doesn't)\s+[^\n.;:]{0,40}\bclaim\s+(?:that\s+)?$",
        r"\bnot\s+(?:proof|evidence)\s+that\s+$",
        r"\bwithout\s+claiming\s+(?:that\s+)?$",
    ]
    return any(re.search(pattern, prefix) for pattern in prefix_negates_claim_patterns)


def audit_boundary_docs(
    handoff_path: Path = HANDOFF_DOC,
    todo_path: Path = SELF_CORRECTION_TODO,
) -> dict[str, Any]:
    required_snippet_groups = [
        ["sandbox_enforced_for_restricted_policies=false"],
        ["fail-closed"],
        [
            "not usable runtime sandbox enforcement",
            "not proof that runtime sandbox enforcement is usable",
            "not sandbox enforcement",
        ],
        ["not fresh provider-backed loop evidence"],
    ]
    forbidden_unconditional_patterns = [r"sandbox_enforced_for_restricted_policies\s*=\s*true"]
    forbidden_positive_claim_patterns = [
        r"\brestricted policies\b[^.\n]{0,120}\bsandbox[- ]enforced\b",
        r"\bsandbox[- ]enforced\b[^.\n]{0,120}\brestricted policies\b",
        r"\brestricted policies\b[^.\n]{0,120}\benforced\b[^.\n]{0,80}\bsandbox\b",
        r"\bsandbox enforcement\b[^.\n]{0,120}\b(?:protects|enforces)\b[^.\n]{0,80}\brestricted policies\b",
        r"\bruntime sandbox\b[^.\n]{0,120}\benforces\b[^.\n]{0,80}\brestricted policies\b",
        r"\bruntime sandbox enforcement\b[^.\n]{0,120}\brestricted policies\b",
        r"\bruntime sandbox enforcement is\s+(?:now\s+)?(?:not\s+only\s+|not\s+just\s+)?usable\b",
        r"\bruntime sandbox enforcement (?:is|was)\s+(?:now\s+)?implemented\b",
        r"\busable runtime sandbox enforcement is implemented\b",
    ]
    documents: list[dict[str, Any]] = []
    missing: list[str] = []
    forbidden: list[dict[str, str]] = []
    for path in [handoff_path, todo_path]:
        if not path.exists():
            missing.append(f"{path}: file missing")
            documents.append({"path": str(path), "exists": False})
            continue
        text = path.read_text(encoding="utf-8")
        doc_missing_groups = [
            group for group in required_snippet_groups if not any(snippet in text for snippet in group)
        ]
        for group in doc_missing_groups:
            missing.append(f"{path}: missing one of {group!r}")
        for pattern in forbidden_unconditional_patterns:
            match = re.search(pattern, text, flags=re.IGNORECASE)
            if match:
                forbidden.append({"path": str(path), "pattern": pattern, "match": match.group(0)})
        for pattern in forbidden_positive_claim_patterns:
            for match in re.finditer(pattern, text, flags=re.IGNORECASE):
                if not _claim_is_negated(text, match.start(), match.end()):
                    forbidden.append({"path": str(path), "pattern": pattern, "match": match.group(0)})
                    break
        documents.append(
            {
                "path": str(path),
                "exists": True,
                "required_snippet_groups_present": not doc_missing_groups,
                "missing_required_snippet_groups": doc_missing_groups,
            }
        )
    return {
        "complete": not missing and not forbidden,
        "documents": documents,
        "required_snippet_groups": required_snippet_groups,
        "missing": missing,
        "forbidden": forbidden,
        "interpretation": "docs preserve fail-closed-vs-enforced distinction and do not claim sandbox enforcement while boundary JSON reports sandbox_enforced_for_restricted_policies=false",
    }


def audit() -> dict[str, Any]:
    pi_package_json = PI_PACKAGE / "package.json"
    pi_version = read_json(pi_package_json).get("version") if pi_package_json.exists() else None
    foundry_head = run(["git", "rev-parse", "HEAD"], cwd=FOUNDRY_REPO) if FOUNDRY_REPO.exists() else None
    foundry_status = run(["git", "status", "--short", "--branch"], cwd=FOUNDRY_REPO) if FOUNDRY_REPO.exists() else None
    sandbox_runtime = npm_global_package_path(SANDBOX_RUNTIME_PACKAGE)

    subagent_checks = [
        find_line(PI_SUBAGENT, "async function runSingleAgent"),
        find_line(PI_SUBAGENT, "function getPiInvocation"),
        find_line(PI_SUBAGENT, "spawn(invocation.command, invocation.args"),
    ]
    foundry_checks = [
        find_line(FOUNDRY_TEAM, "async function runDispatch"),
        find_line(FOUNDRY_TEAM, "function piInvocation"),
        find_line(FOUNDRY_TEAM, "spawn(invocation.command, invocation.args"),
    ]
    sandbox_checks = [
        find_line(PI_SANDBOX, "SandboxManager.wrapWithSandbox"),
        find_line(PI_SANDBOX, "SandboxManager.initialize"),
        find_line(PI_SANDBOX, "network:"),
    ]

    launch_boundaries_found = all(item["found"] for item in subagent_checks + foundry_checks)
    sandbox_example_found = all(item["found"] for item in sandbox_checks)
    actual_launch_sandbox_enforcement = {
        "subagent": find_function_sandbox_enforcement(PI_SUBAGENT, "runSingleAgent"),
        "foundry_team": find_function_sandbox_enforcement(FOUNDRY_TEAM, "runDispatch"),
        "required_in_actual_launch_code_not_examples": True,
    }
    actual_launch_boundaries_found = all(
        item["spawn_present"]
        for key, item in actual_launch_sandbox_enforcement.items()
        if key != "required_in_actual_launch_code_not_examples"
    )
    launch_sandbox_enforced = all(
        item["found"]
        for key, item in actual_launch_sandbox_enforcement.items()
        if key != "required_in_actual_launch_code_not_examples"
    )
    a2_owned_provider_launch_boundary = audit_a2_owned_provider_launch_boundaries()
    boundary_docs = audit_boundary_docs()

    a2_owned_fail_closed = a2_owned_provider_launch_boundary["fail_closed_restricted_policies"]

    return {
        "schema": "a2.agent-network-boundary-audit.v1",
        "not_benchmark_evidence": True,
        "complete": launch_boundaries_found
        and sandbox_example_found
        and actual_launch_boundaries_found
        and a2_owned_fail_closed
        and boundary_docs["complete"],
        "actual_launch_boundaries_found": actual_launch_boundaries_found,
        "launch_sandbox_enforced": launch_sandbox_enforced,
        "pi_package": {
            "path": str(PI_PACKAGE),
            "version": pi_version,
            "subagent_extension": str(PI_SUBAGENT),
            "sandbox_extension": str(PI_SANDBOX),
        },
        "foundry": {
            "repo": str(FOUNDRY_REPO),
            "head": foundry_head["stdout"] if foundry_head else None,
            "status": foundry_status["stdout"].splitlines() if foundry_status else None,
            "team_extension": str(FOUNDRY_TEAM),
        },
        "subagent_launch_boundary": subagent_checks,
        "foundry_team_launch_boundary": foundry_checks,
        "sandbox_example": sandbox_checks,
        "sandbox_runtime": sandbox_runtime,
        "actual_launch_sandbox_enforcement": actual_launch_sandbox_enforcement,
        "a2_owned_provider_launch_boundary": a2_owned_provider_launch_boundary,
        "boundary_docs": boundary_docs,
        "conclusion": (
            "A2-owned provider launch paths visibly fail closed for restricted policies, and child pi launch boundaries are identifiable, but sandbox runtime is not available globally and actual child-agent launch functions do not show sandbox enforcement; "
            "benchmark child-agent network isolation remains unenforced until a sandbox/provider allowlist is wired at these spawn points."
            if a2_owned_fail_closed and (not sandbox_runtime["available"] or not launch_sandbox_enforced)
            else "Child pi launch boundaries, a global sandbox runtime, and sandbox-wrapped spawn paths are present; next step is to run an end-to-end enforcement probe."
            if a2_owned_fail_closed
            else "A2-owned provider launch paths do not visibly fail closed before provider launch; fix the A2 launch gate before treating restricted-policy benchmark evidence as uncontaminated."
        ),
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="print full audit JSON")
    parser.add_argument("--self-test", action="store_true", help="run audit and print PASS/FAIL")
    parser.add_argument("--unit-test", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument(
        "--require-sandbox-runtime",
        action="store_true",
        help=(
            "fail unless @anthropic-ai/sandbox-runtime is available globally and actual child-agent "
            "launch functions connect sandbox wrapping to their spawned command"
        ),
    )
    return parser.parse_args(argv)


def missing_child_agent_sandbox_surfaces(result: dict[str, Any]) -> list[str]:
    enforcement = result.get("actual_launch_sandbox_enforcement")
    if not isinstance(enforcement, dict):
        return []
    missing = []
    for key, item in enforcement.items():
        if key == "required_in_actual_launch_code_not_examples" or not isinstance(item, dict):
            continue
        if item.get("spawn_present") and not item.get("found"):
            missing.append(key)
    return missing


def missing_a2_owned_sandbox_surfaces(a2_boundary: dict[str, Any]) -> list[str]:
    missing: list[str] = []
    worktree = a2_boundary.get("worktree_catalyst")
    if isinstance(worktree, dict):
        for item in worktree.get("provider_command_launch_functions", []):
            if isinstance(item, dict) and item.get("found") and item.get("command_new_present") and not item.get("sandbox_exec_present"):
                missing.append(f"worktree_catalyst.{item.get('function')}")
    broker = a2_boundary.get("broker")
    if isinstance(broker, dict):
        for item in broker.get("provider_generate_guards", []):
            if isinstance(item, dict) and item.get("generate_found") and item.get("command_new_after_guard") and not item.get("sandbox_exec_present"):
                missing.append(f"broker.{item.get('provider')}")
    return missing


def require_sandbox_runtime_failures(result: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    a2_boundary = result.get("a2_owned_provider_launch_boundary")
    if not isinstance(a2_boundary, dict) or not a2_boundary.get("fail_closed_restricted_policies"):
        failures.append("A2-owned provider launch paths do not visibly fail closed for restricted policies")
    if (
        isinstance(a2_boundary, dict)
        and a2_boundary.get("fail_closed_restricted_policies")
        and not a2_boundary.get("sandbox_enforced_for_restricted_policies")
    ):
        missing = missing_a2_owned_sandbox_surfaces(a2_boundary)
        suffix = f": {', '.join(missing)}" if missing else ""
        failures.append(
            "A2-owned provider launch paths do not show sandbox-exec at every provider command boundary"
            f"{suffix}"
        )
    if not result["sandbox_runtime"]["available"]:
        failures.append("@anthropic-ai/sandbox-runtime not installed globally")
    if not result["launch_sandbox_enforced"]:
        missing = missing_child_agent_sandbox_surfaces(result)
        suffix = f": {', '.join(missing)}" if missing else ""
        failures.append(f"actual child-agent launch functions do not show sandbox enforcement{suffix}")
    return failures


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.unit_test:
        return 0 if unittest.main(argv=[sys.argv[0]], exit=False).result.wasSuccessful() else 1

    result = audit()
    if args.require_sandbox_runtime:
        failures = require_sandbox_runtime_failures(result)
        if failures:
            if args.json:
                print(json.dumps(result, indent=2, sort_keys=True))
            else:
                print(f"FAIL sandbox runtime/enforcement: {'; '.join(failures)}")
            return 1
    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    elif args.self_test:
        if result["complete"]:
            runtime = "available" if result["sandbox_runtime"]["available"] else "missing"
            enforcement = "present" if result["launch_sandbox_enforced"] else "missing"
            print(
                "PASS agent network boundary audit: "
                f"launch points found; sandbox runtime {runtime}; launch sandbox enforcement {enforcement}"
            )
        else:
            print("FAIL agent network boundary audit")
            print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result["complete"] else 1


class AgentNetworkBoundaryCheckTests(unittest.TestCase):
    def test_extract_function_body_returns_named_function_only(self) -> None:
        text = "function unrelated() { sandbox-exec }\nasync function target() { const x = { y: true }; spawn(cmd); }"
        body = extract_function_body(text, "target")
        self.assertIsNotNone(body)
        assert body is not None
        self.assertIn("spawn(cmd)", body)
        self.assertNotIn("sandbox-exec", body)

    def test_extract_function_body_ignores_fake_function_in_comment_or_string(self) -> None:
        text = (
            "// function runDispatch() { spawn('sandbox-exec', ['-f', profile, invocation.command, ...invocation.args]); }\n"
            "const fixture = \"function runDispatch() { spawn('sandbox-exec', ['-f', profile, invocation.command, ...invocation.args]); }\";\n"
            "const template = `\nfunction runDispatch() {\n  spawn('sandbox-exec', ['-f', profile, invocation.command, ...invocation.args]);\n}\n`;\n"
            "async function runDispatch() {\n"
            "  spawn(invocation.command, invocation.args);\n"
            "}\n"
        )
        body = extract_function_body(text, "runDispatch")
        self.assertIsNotNone(body)
        assert body is not None
        self.assertIn("spawn(invocation.command, invocation.args)", body)
        self.assertNotIn("sandbox-exec", body)

    def test_extract_function_body_survives_urls_and_regex_literals_before_function(self) -> None:
        text = (
            "const url = 'https://senior-swe-bench.snorkel.ai/tasks';\n"
            "const quotedPath = \"/$bunfs/root/\";\n"
            "const regex = /['`]|https?:\\/\\//g;\n"
            "async function runDispatch() {\n"
            "  spawn(invocation.command, invocation.args);\n"
            "}\n"
        )
        body = extract_function_body(text, "runDispatch")
        self.assertIsNotNone(body)
        assert body is not None
        self.assertIn("spawn(invocation.command, invocation.args)", body)

    def test_actual_launch_enforcement_rejects_fake_spawn_inside_template_interpolation(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "extension.ts"
            path.write_text(
                "const fixture = `${(() => { function runDispatch() { spawn('sandbox-exec', ['-f', profile, invocation.command, ...invocation.args]); } })()}`;\n"
                "async function runDispatch() {\n"
                "  spawn(invocation.command, invocation.args);\n"
                "}\n",
                encoding="utf-8",
            )
            result = find_function_sandbox_enforcement(path, "runDispatch")
        self.assertFalse(result["found"])
        self.assertEqual(result["reason"], "unaccounted_spawn_not_connected_to_sandbox")

    def test_actual_launch_enforcement_rejects_fake_spawn_inside_regex_literal(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "extension.ts"
            path.write_text(
                "async function runDispatch() {\n"
                "  const inert = /spawn('sandbox-exec', ['-f', profile, invocation.command, ...invocation.args, /brace{1,2}/])/g;\n"
                "  const ratio = total / count;\n"
                "  spawn(invocation.command, invocation.args);\n"
                "}\n",
                encoding="utf-8",
            )
            result = find_function_sandbox_enforcement(path, "runDispatch")
        self.assertFalse(result["found"])
        self.assertEqual(result["reason"], "unaccounted_spawn_not_connected_to_sandbox")
        self.assertEqual(result["sandbox_markers"], [])
        self.assertEqual(result["unaccounted_spawns"], ["(invocation.command, invocation.args)"])

    def test_actual_launch_enforcement_accepts_spawn_after_division_expression(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "extension.ts"
            path.write_text(
                "async function runDispatch() {\n"
                "  const ratio = total / count;\n"
                "  const invocation = SandboxManager.wrapWithSandbox(baseInvocation);\n"
                "  spawn(invocation.command, invocation.args);\n"
                "}\n",
                encoding="utf-8",
            )
            result = find_function_sandbox_enforcement(path, "runDispatch")
        self.assertTrue(result["found"])
        self.assertEqual(
            result["sandbox_markers"],
            ["spawn(invocation.command, invocation.args) from SandboxManager.wrapWithSandbox"],
        )

    def test_actual_launch_enforcement_rejects_fake_function_in_comment_or_string(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "extension.ts"
            path.write_text(
                "// function runDispatch() { spawn('sandbox-exec', ['-f', profile, invocation.command, ...invocation.args]); }\n"
                "const fixture = \"function runDispatch() { spawn('sandbox-exec', ['-f', profile, invocation.command, ...invocation.args]); }\";\n"
                "const template = `\nfunction runDispatch() {\n  spawn('sandbox-exec', ['-f', profile, invocation.command, ...invocation.args]);\n}\n`;\n"
                "async function runDispatch() {\n"
                "  spawn(invocation.command, invocation.args);\n"
                "}\n",
                encoding="utf-8",
            )
            result = find_function_sandbox_enforcement(path, "runDispatch")
        self.assertFalse(result["found"])
        self.assertEqual(result["reason"], "unaccounted_spawn_not_connected_to_sandbox")
        self.assertEqual(result["sandbox_markers"], [])

    def test_actual_launch_enforcement_requires_marker_in_launch_function(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "extension.ts"
            path.write_text(
                "import { SandboxManager } from '@anthropic-ai/sandbox-runtime';\n"
                "// SandboxManager.wrapWithSandbox in a comment outside launch code\n"
                "async function runDispatch() {\n"
                "  // SandboxManager.wrapWithSandbox in a comment inside launch code\n"
                "  /* sandbox-exec in a block comment inside launch code */\n"
                "  const inert = 'SandboxManager.wrapWithSandbox and sandbox-exec in a string';\n"
                "  spawn(invocation.command, invocation.args);\n"
                "}\n",
                encoding="utf-8",
            )
            result = find_function_sandbox_enforcement(path, "runDispatch")
        self.assertFalse(result["found"])
        self.assertTrue(result["spawn_present"])
        self.assertEqual(result["sandbox_markers"], [])

    def test_actual_launch_enforcement_rejects_executable_but_inert_marker(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "extension.ts"
            path.write_text(
                "async function runDispatch() {\n"
                "  await import('@anthropic-ai/sandbox-runtime');\n"
                "  SandboxManager.wrapWithSandbox;\n"
                "  const wrapped = SandboxManager.wrapWithSandbox(baseInvocation);\n"
                "  spawn(invocation.command, invocation.args);\n"
                "}\n",
                encoding="utf-8",
            )
            result = find_function_sandbox_enforcement(path, "runDispatch")
        self.assertFalse(result["found"])
        self.assertTrue(result["spawn_present"])
        self.assertEqual(result["sandbox_markers"], [])

    def test_actual_launch_enforcement_accepts_wrapper_connected_to_spawn(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "extension.ts"
            path.write_text(
                "async function runDispatch() {\n"
                "  const invocation = SandboxManager.wrapWithSandbox(baseInvocation);\n"
                "  spawn(invocation.command, invocation.args);\n"
                "}\n",
                encoding="utf-8",
            )
            result = find_function_sandbox_enforcement(path, "runDispatch")
        self.assertTrue(result["found"])
        self.assertTrue(result["spawn_present"])
        self.assertEqual(
            result["sandbox_markers"],
            ["spawn(invocation.command, invocation.args) from SandboxManager.wrapWithSandbox"],
        )

    def test_actual_launch_enforcement_accepts_direct_sandbox_exec_spawn(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "extension.ts"
            path.write_text(
                "async function runDispatch() {\n"
                "  spawn('sandbox-exec', ['-f', profile, invocation.command, ...invocation.args]);\n"
                "}\n",
                encoding="utf-8",
            )
            result = find_function_sandbox_enforcement(path, "runDispatch")
        self.assertTrue(result["found"])
        self.assertTrue(result["spawn_present"])
        self.assertEqual(
            result["sandbox_markers"],
            ["spawn(sandbox-exec, ... child command/args ...)"],
        )

    def test_actual_launch_enforcement_accepts_sandbox_exec_constant_spawn(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "extension.ts"
            path.write_text(
                "async function runDispatch() {\n"
                "  const sandboxCommand = 'sandbox-exec';\n"
                "  spawn(sandboxCommand, ['-f', profile, invocation.command, ...invocation.args]);\n"
                "}\n",
                encoding="utf-8",
            )
            result = find_function_sandbox_enforcement(path, "runDispatch")
        self.assertTrue(result["found"])
        self.assertTrue(result["spawn_present"])
        self.assertEqual(
            result["sandbox_markers"],
            ["spawn(sandbox-exec, ... child command/args ...)"],
        )

    def test_actual_launch_enforcement_rejects_wrapped_spawn_plus_unwrapped_spawn(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "extension.ts"
            path.write_text(
                "async function runDispatch() {\n"
                "  const wrapped = SandboxManager.wrapWithSandbox(baseInvocation);\n"
                "  spawn(wrapped.command, wrapped.args);\n"
                "  spawn(invocation.command, invocation.args);\n"
                "}\n",
                encoding="utf-8",
            )
            result = find_function_sandbox_enforcement(path, "runDispatch")
        self.assertFalse(result["found"])
        self.assertEqual(result["reason"], "unaccounted_spawn_not_connected_to_sandbox")
        self.assertEqual(result["unaccounted_spawns"], ["(invocation.command, invocation.args)"])

    def test_actual_launch_enforcement_rejects_unrelated_sandbox_exec_spawn(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "extension.ts"
            path.write_text(
                "async function runDispatch() {\n"
                "  spawn('sandbox-exec', ['-f', profile, 'echo']);\n"
                "  spawn(invocation.command, invocation.args);\n"
                "}\n",
                encoding="utf-8",
            )
            result = find_function_sandbox_enforcement(path, "runDispatch")
        self.assertFalse(result["found"])
        self.assertEqual(result["reason"], "unaccounted_spawn_not_connected_to_sandbox")
        self.assertEqual(
            result["unaccounted_spawns"],
            [
                "('sandbox-exec', ['-f', profile, 'echo'])",
                "(invocation.command, invocation.args)",
            ],
        )

    def test_extract_function_body_ignores_braces_inside_strings_and_comments(self) -> None:
        text = (
            "async function runDispatch() {\n"
            "  const text = '} not the end';\n"
            "  // } not the end either\n"
            "  spawn(invocation.command, invocation.args);\n"
            "}\n"
        )
        body = extract_function_body(text, "runDispatch")
        self.assertIsNotNone(body)
        assert body is not None
        self.assertIn("spawn(invocation.command, invocation.args)", body)

    def test_actual_launch_enforcement_rejects_fake_spawn_inside_string(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "extension.ts"
            path.write_text(
                "async function runDispatch() {\n"
                "  const inert = \"spawn('sandbox-exec', ['-f', profile])\";\n"
                "  spawn(invocation.command, invocation.args);\n"
                "}\n",
                encoding="utf-8",
            )
            result = find_function_sandbox_enforcement(path, "runDispatch")
        self.assertFalse(result["found"])
        self.assertTrue(result["spawn_present"])
        self.assertEqual(result["sandbox_markers"], [])

    def test_rust_masking_ignores_raw_strings_and_nested_block_comments(self) -> None:
        text = (
            'fn demo() {\n'
            '  let inert = r##"Command::new(\"sandbox-exec\")"##;\n'
            '  /* outer Command::new("sandbox-exec") /* nested Command::new("sandbox-exec") */ still comment */\n'
            '  let real = Command::new("opencode");\n'
            '}\n'
        )
        without_comments = strip_rust_comments(text, strip_strings=False)
        without_strings = strip_rust_comments(text, strip_strings=True)

        self.assertEqual(
            rust_code_regex_match_offsets(without_comments, r"\bCommand::new\s*\(\s*\"sandbox-exec\""),
            [],
        )
        self.assertEqual(len(rust_command_new_offsets(without_strings)), 1)

    def test_extract_rust_impl_body_handles_lifetimes_attributes_and_char_braces(self) -> None:
        text = r'''
const INERT: &str = r##"
impl ModelProvider for PiProvider {
    async fn generate(&self) { Command::new("pi"); }
}
"##;

#[async_trait]
impl<'a> ModelProvider for PiProvider {
    async fn generate(&self) {
        let open = '{';
        let close = '}';
        let quote = '\'';
        fail_if_provider_network_restricted("pi")?;
        Command::new("pi");
    }
}
'''
        impl_body = extract_rust_impl_body(text, "PiProvider")
        self.assertIsNotNone(impl_body)
        assert impl_body is not None
        self.assertIn("let open = '{';", impl_body)
        generate_body = extract_rust_function_body(impl_body, "generate")
        self.assertIsNotNone(generate_body)
        assert generate_body is not None
        self.assertIn('fail_if_provider_network_restricted("pi")?;', generate_body)
        self.assertIn('Command::new("pi")', generate_body)

    def test_a2_broker_launch_gate_rejects_guard_in_inner_closure_before_command_new(self) -> None:
        provider_impls = "\n".join(
            f'''impl ModelProvider for {impl_name} {{
    async fn generate(&self) {{
        let _inert_guard = || {{
            fail_if_provider_network_restricted("{provider}")?;
            Ok::<(), A2Error>(())
        }};
        Command::new("{provider}");
    }}
}}'''
            for provider, impl_name in [
                ("claude", "ClaudeProvider"),
                ("gemini", "GeminiProvider"),
                ("codex", "CodexProvider"),
                ("pi", "PiProvider"),
                ("opencode", "OpenCodeProvider"),
            ]
        )
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "broker.rs"
            path.write_text(
                'fn fail_if_provider_network_restricted_for_policy() {\n'
                '  if normalized == "isolated" || normalized.starts_with("allowlist") { return Err(error); }\n'
                '}\n'
                + provider_impls,
                encoding="utf-8",
            )
            result = audit_a2_broker_launch_gate(path)

        self.assertTrue(result["policy_helper_rejects_isolated"])
        self.assertTrue(result["policy_helper_rejects_allowlist"])
        self.assertFalse(result["fail_closed_before_provider_launch"])
        self.assertTrue(all(item["command_new_after_guard"] for item in result["provider_generate_guards"]))
        self.assertTrue(all(not item["guard_before_command_new"] for item in result["provider_generate_guards"]))

    def test_a2_worktree_launch_gate_rejects_policy_markers_only_in_rust_comments_or_strings(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "worktree_catalyst.rs"
            path.write_text(
                'async fn run_agent() {\n'
                '  let inert = r##"NetworkPolicy::Isolated NetworkPolicy::AllowList return Err"##;\n'
                '  /* NetworkPolicy::Isolated /* NetworkPolicy::AllowList return Err */ still comment */\n'
                '  if provider_id == "mock" { return self.run_mock_agent(worktree_path).await; }\n'
                '  match provider_id { _ => self.run_claude(model_id, prompt, worktree_path).await }\n'
                '}\n'
                'async fn run_claude() { Command::new("claude"); }\n'
                'async fn run_codex() { Command::new("codex"); }\n'
                'async fn run_gemini() { Command::new("gemini"); }\n'
                'async fn run_opencode() { Command::new("opencode"); }\n'
                'async fn run_pi() { Command::new("pi"); }\n',
                encoding="utf-8",
            )
            result = audit_a2_worktree_catalyst_launch_gate(path)

        self.assertTrue(result["run_agent_found"])
        self.assertFalse(result["restricted_policy_check_present"])
        self.assertFalse(result["fail_closed_before_provider_launch"])

    def test_a2_worktree_launch_gate_rejects_disconnected_policy_markers_and_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "worktree_catalyst.rs"
            path.write_text(
                'async fn run_agent() {\n'
                '  let _policy_seen = NetworkPolicy::Isolated;\n'
                '  let _allowlist_seen = NetworkPolicy::AllowList(vec![]);\n'
                '  if provider_id == "invalid" { return Err(error); }\n'
                '  if provider_id == "mock" { return self.run_mock_agent(worktree_path).await; }\n'
                '  match provider_id { _ => self.run_claude(model_id, prompt, worktree_path).await }\n'
                '}\n'
                'async fn run_claude() { Command::new("claude"); }\n'
                'async fn run_codex() { Command::new("codex"); }\n'
                'async fn run_gemini() { Command::new("gemini"); }\n'
                'async fn run_opencode() { Command::new("opencode"); }\n'
                'async fn run_pi() { Command::new("pi"); }\n',
                encoding="utf-8",
            )
            result = audit_a2_worktree_catalyst_launch_gate(path)

        self.assertTrue(result["run_agent_found"])
        self.assertFalse(result["restricted_policy_check_present"])
        self.assertFalse(result["fail_closed_before_provider_launch"])

    def test_a2_worktree_launch_gate_rejects_return_err_only_inside_nested_closure(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "worktree_catalyst.rs"
            path.write_text(
                'async fn run_agent() {\n'
                '  if matches!(network_policy, Some(NetworkPolicy::Isolated | NetworkPolicy::AllowList(_))) {\n'
                '    let _not_called = || { return Err(error); };\n'
                '  }\n'
                '  if provider_id == "mock" { return self.run_mock_agent(worktree_path).await; }\n'
                '  match provider_id { _ => self.run_claude(model_id, prompt, worktree_path).await }\n'
                '}\n'
                'async fn run_claude() { Command::new("claude"); }\n'
                'async fn run_codex() { Command::new("codex"); }\n'
                'async fn run_gemini() { Command::new("gemini"); }\n'
                'async fn run_opencode() { Command::new("opencode"); }\n'
                'async fn run_pi() { Command::new("pi"); }\n',
                encoding="utf-8",
            )
            result = audit_a2_worktree_catalyst_launch_gate(path)

        self.assertTrue(result["run_agent_found"])
        self.assertFalse(result["restricted_policy_check_present"])
        self.assertFalse(result["fail_closed_before_provider_launch"])

    def test_a2_broker_launch_gate_rejects_allowlist_helper_marker_without_return_path(self) -> None:
        provider_impls = "\n".join(
            f'''impl ModelProvider for {impl_name} {{
    async fn generate(&self) {{
        fail_if_provider_network_restricted("{provider}")?;
        Command::new("{provider}");
    }}
}}'''
            for provider, impl_name in [
                ("claude", "ClaudeProvider"),
                ("gemini", "GeminiProvider"),
                ("codex", "CodexProvider"),
                ("pi", "PiProvider"),
                ("opencode", "OpenCodeProvider"),
            ]
        )
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "broker.rs"
            path.write_text(
                'fn fail_if_provider_network_restricted_for_policy() {\n'
                '  let inert = "allowlist";\n'
                '  if normalized == "isolated" { return Err(error); }\n'
                '  Ok(())\n'
                '}\n'
                + provider_impls,
                encoding="utf-8",
            )
            result = audit_a2_broker_launch_gate(path)

        self.assertTrue(result["policy_helper_rejects_isolated"])
        self.assertFalse(result["policy_helper_rejects_allowlist"])
        self.assertFalse(result["fail_closed_before_provider_launch"])

    def test_a2_broker_launch_gate_rejects_allowlist_predicate_assignment_before_unrelated_error(self) -> None:
        provider_impls = "\n".join(
            f'''impl ModelProvider for {impl_name} {{
    async fn generate(&self) {{
        fail_if_provider_network_restricted("{provider}")?;
        Command::new("{provider}");
    }}
}}'''
            for provider, impl_name in [
                ("claude", "ClaudeProvider"),
                ("gemini", "GeminiProvider"),
                ("codex", "CodexProvider"),
                ("pi", "PiProvider"),
                ("opencode", "OpenCodeProvider"),
            ]
        )
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "broker.rs"
            path.write_text(
                'fn fail_if_provider_network_restricted_for_policy() {\n'
                '  let shift = normalized.starts_with("allowlist");\n'
                '  if normalized == "isolated" { return Err(error); }\n'
                '  Ok(())\n'
                '}\n'
                + provider_impls,
                encoding="utf-8",
            )
            result = audit_a2_broker_launch_gate(path)

        self.assertTrue(result["policy_helper_rejects_isolated"])
        self.assertFalse(result["policy_helper_rejects_allowlist"])
        self.assertFalse(result["fail_closed_before_provider_launch"])

    def test_a2_broker_launch_gate_rejects_guards_only_in_rust_comments_or_raw_strings(self) -> None:
        provider_impls = "\n".join(
            f'''impl ModelProvider for {impl_name} {{
    async fn generate(&self) {{
        let inert = r##"fail_if_provider_network_restricted(\"{provider}\")?;"##;
        // fail_if_provider_network_restricted("{provider}")?;
        Command::new("{provider}");
    }}
}}'''
            for provider, impl_name in [
                ("claude", "ClaudeProvider"),
                ("gemini", "GeminiProvider"),
                ("codex", "CodexProvider"),
                ("pi", "PiProvider"),
                ("opencode", "OpenCodeProvider"),
            ]
        )
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "broker.rs"
            path.write_text(
                'fn fail_if_provider_network_restricted_for_policy() {\n'
                '  if normalized == "isolated" || normalized.starts_with("allowlist") { return Err(error); }\n'
                '}\n'
                + provider_impls,
                encoding="utf-8",
            )
            result = audit_a2_broker_launch_gate(path)

        self.assertTrue(result["policy_helper_rejects_isolated"])
        self.assertTrue(result["policy_helper_rejects_allowlist"])
        self.assertFalse(result["fail_closed_before_provider_launch"])
        self.assertTrue(all(not item["guard_before_command_new"] for item in result["provider_generate_guards"]))

    def test_a2_worktree_launch_gate_rejects_sandbox_exec_after_direct_provider_command(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "worktree_catalyst.rs"
            provider_functions = "\n".join(
                f'async fn {function_name}() {{ Command::new("{provider}"); Command::new("sandbox-exec"); }}'
                for function_name, provider in [
                    ("run_claude", "claude"),
                    ("run_codex", "codex"),
                    ("run_gemini", "gemini"),
                    ("run_opencode", "opencode"),
                    ("run_pi", "pi"),
                ]
            )
            path.write_text(
                'async fn run_agent() {\n'
                '  if matches!(network_policy, Some(NetworkPolicy::Isolated | NetworkPolicy::AllowList(_))) {\n'
                '    return Err(error);\n'
                '  }\n'
                '  if provider_id == "mock" { return self.run_mock_agent(worktree_path).await; }\n'
                '  match provider_id { _ => self.run_claude(model_id, prompt, worktree_path).await }\n'
                '}\n'
                + provider_functions,
                encoding="utf-8",
            )
            result = audit_a2_worktree_catalyst_launch_gate(path)

        self.assertTrue(result["fail_closed_before_provider_launch"])
        self.assertFalse(result["sandbox_exec_launch_wrapper_present"])
        self.assertTrue(all(not item["sandbox_exec_present"] for item in result["provider_command_launch_functions"]))

    def test_a2_broker_launch_gate_rejects_sandbox_exec_after_direct_provider_command(self) -> None:
        provider_impls = "\n".join(
            f'''impl ModelProvider for {impl_name} {{
    async fn generate(&self) {{
        fail_if_provider_network_restricted("{provider}")?;
        Command::new("{provider}");
        Command::new("sandbox-exec");
    }}
}}'''
            for provider, impl_name in [
                ("claude", "ClaudeProvider"),
                ("gemini", "GeminiProvider"),
                ("codex", "CodexProvider"),
                ("pi", "PiProvider"),
                ("opencode", "OpenCodeProvider"),
            ]
        )
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "broker.rs"
            path.write_text(
                'fn fail_if_provider_network_restricted_for_policy() {\n'
                '  if normalized == "isolated" || normalized.starts_with("allowlist") { return Err(error); }\n'
                '}\n'
                + provider_impls,
                encoding="utf-8",
            )
            result = audit_a2_broker_launch_gate(path)

        self.assertTrue(result["fail_closed_before_provider_launch"])
        self.assertFalse(result["sandbox_exec_launch_wrapper_present"])
        self.assertTrue(all(not item["sandbox_exec_present"] for item in result["provider_generate_guards"]))

    def test_a2_worktree_launch_gate_detects_legacy_matches_and_if_let_policy_forms(self) -> None:
        run_agent_forms = [
            (
                "legacy_matches",
                '  if matches!(network_policy, Some(NetworkPolicy::Isolated | NetworkPolicy::AllowList(_))) {\n'
                '    return Err(error);\n'
                '  }\n',
            ),
            (
                "if_let_policy_binding",
                '  if let Some(policy @ (NetworkPolicy::Isolated | NetworkPolicy::AllowList(_))) = network_policy {\n'
                '    return Err(error);\n'
                '  }\n',
            ),
        ]
        for name, policy_guard in run_agent_forms:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as tmpdir:
                path = Path(tmpdir) / "worktree_catalyst.rs"
                path.write_text(
                    'async fn run_agent() {\n'
                    + policy_guard
                    + '  if provider_id == "mock" { return self.run_mock_agent(worktree_path).await; }\n'
                    + '  match provider_id { _ => self.run_claude(model_id, prompt, worktree_path).await }\n'
                    + '}\n'
                    + 'async fn run_claude() { Command::new("claude"); }\n'
                    + 'async fn run_codex() { Command::new("codex"); }\n'
                    + 'async fn run_gemini() { Command::new("gemini"); }\n'
                    + 'async fn run_opencode() { Command::new("opencode"); }\n'
                    + 'async fn run_pi() { Command::new("pi"); }\n',
                    encoding="utf-8",
                )
                result = audit_a2_worktree_catalyst_launch_gate(path)

            self.assertTrue(result["restricted_policy_check_present"])
            self.assertTrue(result["restricted_policy_error_before_mock"])
            self.assertTrue(result["restricted_policy_error_before_provider_dispatch"])
            self.assertTrue(result["fail_closed_before_provider_launch"])

    def test_a2_worktree_launch_gate_detects_fail_closed_before_provider_dispatch(self) -> None:
        result = audit_a2_worktree_catalyst_launch_gate()

        self.assertTrue(result["run_agent_found"])
        self.assertTrue(result["restricted_policy_check_present"])
        self.assertTrue(result["restricted_policy_error_before_mock"])
        self.assertTrue(result["restricted_policy_error_before_provider_dispatch"])
        self.assertTrue(result["fail_closed_before_provider_launch"])
        self.assertFalse(result["sandbox_exec_launch_wrapper_present"])
        self.assertEqual(
            [item["function"] for item in result["provider_command_launch_functions"]],
            ["run_claude", "run_codex", "run_gemini", "run_opencode", "run_pi"],
        )

    def test_a2_broker_launch_gate_detects_all_provider_guards(self) -> None:
        result = audit_a2_broker_launch_gate()

        self.assertTrue(result["policy_helper_found"])
        self.assertTrue(result["policy_helper_rejects_isolated"])
        self.assertTrue(result["policy_helper_rejects_allowlist"])
        self.assertTrue(result["fail_closed_before_provider_launch"])
        self.assertFalse(result["sandbox_exec_launch_wrapper_present"])
        self.assertEqual(
            [item["provider"] for item in result["provider_generate_guards"]],
            ["claude", "gemini", "codex", "pi", "opencode"],
        )
        self.assertTrue(all(item["guard_before_command_new"] for item in result["provider_generate_guards"]))

    def test_a2_generalist_launch_gate_detects_fail_closed_before_model_generate(self) -> None:
        result = audit_a2_generalist_catalyst_launch_gate()

        self.assertTrue(result["execute_found"])
        self.assertTrue(result["restricted_policy_check_present"])
        self.assertTrue(result["provider_generate_call_present"])
        self.assertTrue(result["restricted_policy_error_before_provider_call"])
        self.assertTrue(result["fail_closed_before_provider_launch"])

    def test_a2_generalist_launch_gate_rejects_policy_check_after_model_generate(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "catalyst.rs"
            path.write_text(
                'impl Catalyst for GeneralistCatalyst {\n'
                '  async fn execute() {\n'
                '    model.generate(&prompt, Some(system)).await?;\n'
                '    if matches!(&task.network_policy, Some(NetworkPolicy::Isolated | NetworkPolicy::AllowList(_))) {\n'
                '      return Err(error);\n'
                '    }\n'
                '  }\n'
                '}\n',
                encoding="utf-8",
            )
            result = audit_a2_generalist_catalyst_launch_gate(path)

        self.assertTrue(result["execute_found"])
        self.assertTrue(result["restricted_policy_check_present"])
        self.assertTrue(result["provider_generate_call_present"])
        self.assertFalse(result["restricted_policy_error_before_provider_call"])
        self.assertFalse(result["fail_closed_before_provider_launch"])

    def test_a2_generalist_launch_gate_anchors_to_generalist_catalyst_impl(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "catalyst.rs"
            path.write_text(
                'async fn execute() {\n'
                '  if matches!(&task.network_policy, Some(NetworkPolicy::Isolated | NetworkPolicy::AllowList(_))) {\n'
                '    return Err(error);\n'
                '  }\n'
                '  model.generate(&prompt, Some(system)).await?;\n'
                '}\n'
                'impl Catalyst for OtherCatalyst {\n'
                '  async fn execute() {\n'
                '    if matches!(&task.network_policy, Some(NetworkPolicy::Isolated | NetworkPolicy::AllowList(_))) {\n'
                '      return Err(error);\n'
                '    }\n'
                '    model.generate(&prompt, Some(system)).await?;\n'
                '  }\n'
                '}\n'
                'impl Catalyst for GeneralistCatalyst {\n'
                '  async fn execute() {\n'
                '    model.generate(&prompt, Some(system)).await?;\n'
                '    if matches!(&task.network_policy, Some(NetworkPolicy::Isolated | NetworkPolicy::AllowList(_))) {\n'
                '      return Err(error);\n'
                '    }\n'
                '  }\n'
                '}\n',
                encoding="utf-8",
            )
            result = audit_a2_generalist_catalyst_launch_gate(path)

        self.assertTrue(result["execute_found"])
        self.assertTrue(result["restricted_policy_check_present"])
        self.assertTrue(result["provider_generate_call_present"])
        self.assertFalse(result["restricted_policy_error_before_provider_call"])
        self.assertFalse(result["fail_closed_before_provider_launch"])

    def test_a2_owned_sandbox_enforcement_requires_all_provider_surfaces(self) -> None:
        wrapped_surface = {
            "fail_closed_before_provider_launch": True,
            "sandbox_exec_launch_wrapper_present": True,
        }
        unwrapped_surface = {
            "fail_closed_before_provider_launch": True,
            "sandbox_exec_launch_wrapper_present": False,
        }
        wrapped_but_open_surface = {
            "fail_closed_before_provider_launch": False,
            "sandbox_exec_launch_wrapper_present": True,
        }
        closed_generalist = {"fail_closed_before_provider_launch": True}
        open_generalist = {"fail_closed_before_provider_launch": False}
        self.assertFalse(
            a2_owned_sandbox_enforced_for_all_provider_surfaces(
                wrapped_surface,
                closed_generalist,
                unwrapped_surface,
            )
        )
        self.assertFalse(
            a2_owned_sandbox_enforced_for_all_provider_surfaces(
                unwrapped_surface,
                closed_generalist,
                wrapped_surface,
            )
        )
        self.assertFalse(
            a2_owned_sandbox_enforced_for_all_provider_surfaces(
                wrapped_surface,
                open_generalist,
                wrapped_surface,
            )
        )
        self.assertFalse(
            a2_owned_sandbox_enforced_for_all_provider_surfaces(
                wrapped_but_open_surface,
                closed_generalist,
                wrapped_surface,
            )
        )
        self.assertFalse(
            a2_owned_sandbox_enforced_for_all_provider_surfaces(
                wrapped_surface,
                closed_generalist,
                wrapped_but_open_surface,
            )
        )
        self.assertTrue(
            a2_owned_sandbox_enforced_for_all_provider_surfaces(
                wrapped_surface,
                closed_generalist,
                wrapped_surface,
            )
        )

    def test_a2_owned_provider_launch_boundary_is_reported_in_full_audit(self) -> None:
        result = audit()
        boundary = result["a2_owned_provider_launch_boundary"]

        self.assertTrue(boundary["fail_closed_restricted_policies"])
        self.assertFalse(boundary["sandbox_enforced_for_restricted_policies"])
        self.assertIn("fail closed", boundary["interpretation"])
        self.assertTrue(result["boundary_docs"]["complete"])

    def test_boundary_docs_allow_negative_enforcement_wording(self) -> None:
        valid_claims = [
            "These checks cannot claim restricted policies are sandbox-enforced.\n",
            "restricted policies are not sandbox-enforced by this audit.\n",
            "runtime sandbox enforcement does not enforce restricted policies yet.\n",
            "sandbox enforcement does not protect restricted policies yet.\n",
        ]
        for valid_claim in valid_claims:
            with self.subTest(valid_claim=valid_claim):
                with tempfile.TemporaryDirectory() as tmpdir:
                    handoff = Path(tmpdir) / "HANDOFF.md"
                    todo = Path(tmpdir) / "self-correction-loop.md"
                    valid = (
                        "sandbox_enforced_for_restricted_policies=false\n"
                        "fail-closed\n"
                        "This is not proof that runtime sandbox enforcement is usable.\n"
                        f"{valid_claim}"
                        "not fresh provider-backed loop evidence\n"
                    )
                    handoff.write_text(valid, encoding="utf-8")
                    todo.write_text(valid, encoding="utf-8")

                    result = audit_boundary_docs(handoff, todo)

                self.assertTrue(result["complete"])
                self.assertFalse(result["forbidden"])

    def test_boundary_docs_reject_enforcement_claim_when_boundary_is_not_enforced(self) -> None:
        positive_claims = [
            "restricted policies are sandbox-enforced now\n",
            "restricted policies are now sandbox-enforced.\n",
            "restricted policies are sandbox-enforced by the worktree runtime.\n",
            "restricted policies are enforced by the sandbox.\n",
            "sandbox enforcement now protects restricted policies.\n",
            "the runtime sandbox enforces restricted policies.\n",
            "runtime sandbox enforcement covers restricted policies.\n",
            "runtime sandbox enforcement is usable now.\n",
            "runtime sandbox enforcement is not only usable but required.\n",
            "This is not proof. Restricted policies are sandbox-enforced by the runtime.\n",
            "It is not safe to proceed because restricted policies are enforced by the sandbox.\n",
        ]
        valid = (
            "sandbox_enforced_for_restricted_policies=false\n"
            "fail-closed\n"
            "not usable runtime sandbox enforcement\n"
            "not fresh provider-backed loop evidence\n"
        )
        for claim in positive_claims:
            with self.subTest(claim=claim):
                with tempfile.TemporaryDirectory() as tmpdir:
                    handoff = Path(tmpdir) / "HANDOFF.md"
                    todo = Path(tmpdir) / "self-correction-loop.md"
                    handoff.write_text(valid, encoding="utf-8")
                    todo.write_text(valid + claim, encoding="utf-8")

                    result = audit_boundary_docs(handoff, todo)

                self.assertFalse(result["complete"])
                self.assertTrue(result["forbidden"])

    def test_missing_a2_owned_sandbox_surfaces_names_each_unwrapped_provider_boundary(self) -> None:
        missing = missing_a2_owned_sandbox_surfaces(
            {
                "worktree_catalyst": {
                    "provider_command_launch_functions": [
                        {"function": "run_claude", "found": True, "command_new_present": True, "sandbox_exec_present": False},
                        {"function": "run_pi", "found": True, "command_new_present": True, "sandbox_exec_present": True},
                    ]
                },
                "broker": {
                    "provider_generate_guards": [
                        {"provider": "claude", "generate_found": True, "command_new_after_guard": True, "sandbox_exec_present": False},
                        {"provider": "pi", "generate_found": True, "command_new_after_guard": True, "sandbox_exec_present": True},
                    ]
                },
            }
        )

        self.assertEqual(missing, ["worktree_catalyst.run_claude", "broker.claude"])

    def test_require_sandbox_runtime_names_missing_a2_and_child_launch_surfaces(self) -> None:
        result = {
            "a2_owned_provider_launch_boundary": {
                "fail_closed_restricted_policies": True,
                "sandbox_enforced_for_restricted_policies": False,
                "worktree_catalyst": {
                    "provider_command_launch_functions": [
                        {"function": "run_pi", "found": True, "command_new_present": True, "sandbox_exec_present": False},
                    ]
                },
                "broker": {
                    "provider_generate_guards": [
                        {"provider": "pi", "generate_found": True, "command_new_after_guard": True, "sandbox_exec_present": False},
                    ]
                },
            },
            "actual_launch_sandbox_enforcement": {
                "subagent": {"spawn_present": True, "found": False},
                "foundry_team": {"spawn_present": True, "found": False},
                "required_in_actual_launch_code_not_examples": True,
            },
            "sandbox_runtime": {"available": True},
            "launch_sandbox_enforced": False,
        }

        self.assertEqual(
            require_sandbox_runtime_failures(result),
            [
                "A2-owned provider launch paths do not show sandbox-exec at every provider command boundary: worktree_catalyst.run_pi, broker.pi",
                "actual child-agent launch functions do not show sandbox enforcement: subagent, foundry_team",
            ],
        )

    def test_require_sandbox_runtime_fails_when_runtime_present_but_launch_unenforced(self) -> None:
        result = {
            "a2_owned_provider_launch_boundary": {
                "fail_closed_restricted_policies": True,
                "sandbox_enforced_for_restricted_policies": True,
            },
            "sandbox_runtime": {"available": True},
            "launch_sandbox_enforced": False,
        }
        self.assertEqual(
            require_sandbox_runtime_failures(result),
            ["actual child-agent launch functions do not show sandbox enforcement"],
        )

    def test_require_sandbox_runtime_includes_a2_owned_launch_gate_failure(self) -> None:
        result = {
            "a2_owned_provider_launch_boundary": {"fail_closed_restricted_policies": False},
            "sandbox_runtime": {"available": True},
            "launch_sandbox_enforced": True,
        }
        self.assertEqual(
            require_sandbox_runtime_failures(result),
            ["A2-owned provider launch paths do not visibly fail closed for restricted policies"],
        )


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
