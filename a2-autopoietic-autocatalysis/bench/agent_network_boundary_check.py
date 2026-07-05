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


def extract_function_body(text: str, function_name: str) -> str | None:
    start = find_function_declaration_start(text, function_name)
    if start is None:
        return None
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

    return {
        "schema": "a2.agent-network-boundary-audit.v1",
        "not_benchmark_evidence": True,
        "complete": launch_boundaries_found and sandbox_example_found and actual_launch_boundaries_found,
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
        "conclusion": (
            "Child pi launch boundaries are identifiable, but sandbox runtime is not available globally and actual child-agent launch functions do not show sandbox enforcement; "
            "benchmark child-agent network isolation remains unenforced until a sandbox/provider allowlist is wired at these spawn points."
            if not sandbox_runtime["available"] or not launch_sandbox_enforced
            else "Child pi launch boundaries, a global sandbox runtime, and sandbox-wrapped spawn paths are present; next step is to run an end-to-end enforcement probe."
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


def require_sandbox_runtime_failures(result: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    if not result["sandbox_runtime"]["available"]:
        failures.append("@anthropic-ai/sandbox-runtime not installed globally")
    if not result["launch_sandbox_enforced"]:
        failures.append("actual child-agent launch functions do not show sandbox enforcement")
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

    def test_require_sandbox_runtime_fails_when_runtime_present_but_launch_unenforced(self) -> None:
        result = {
            "sandbox_runtime": {"available": True},
            "launch_sandbox_enforced": False,
        }
        self.assertEqual(
            require_sandbox_runtime_failures(result),
            ["actual child-agent launch functions do not show sandbox enforcement"],
        )


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
