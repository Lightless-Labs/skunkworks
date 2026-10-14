#!/usr/bin/env python3
"""Fail unless Cargo targets and parity-tagged Bazel targets match exactly."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import subprocess
import sys
import unittest
import xml.etree.ElementTree as ET


@dataclass(frozen=True)
class TargetRecord:
    label: str
    rule_class: str
    package: str
    semantic: str


def _run(command: list[str], cwd: Path, timeout: int) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            "command failed with return code {}: {}\nstdout:\n{}\nstderr:\n{}".format(
                completed.returncode,
                command,
                completed.stdout,
                completed.stderr,
            )
        )
    return completed.stdout


def cargo_metadata(workspace: Path) -> dict:
    return json.loads(
        _run(
            ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
            workspace,
            timeout=120,
        )
    )


def bazel_query_xml(workspace: Path, output_user_root: str | None) -> str:
    command = ["bazel", "--batch"]
    if output_user_root:
        command.append("--output_user_root={}".format(output_user_root))
    command.extend(
        [
            "query",
            'attr(tags, "cargo-.*", //crates/...)',
            "--output=xml",
        ]
    )
    return _run(command, workspace, timeout=300)


def _package_path(workspace: Path, manifest_path: str) -> str:
    manifest = Path(manifest_path).resolve()
    try:
        return manifest.parent.relative_to(workspace.resolve()).as_posix()
    except ValueError as error:
        raise ValueError(
            "Cargo package manifest is outside the workspace: {}".format(manifest)
        ) from error


def expected_targets(metadata: dict, workspace: Path) -> dict[str, TargetRecord]:
    expected: dict[str, TargetRecord] = {}
    for package in metadata["packages"]:
        package_name = package["name"]
        package_path = _package_path(workspace, package["manifest_path"])
        targets = package["targets"]
        has_library = any("lib" in target["kind"] for target in targets)
        for target in targets:
            kinds = set(target["kind"])
            cargo_name = target["name"]
            supported_kinds = {"lib", "bin", "test"}
            if len(kinds) != 1 or not kinds <= supported_kinds:
                raise ValueError(
                    "unsupported Cargo target kind(s) for {}:{}: {}; add an explicit Bazel mapping before parity may pass"
                    .format(package_name, cargo_name, sorted(kinds))
                )
            if "lib" in kinds:
                bazel_name = cargo_name
                _add_expected(
                    expected,
                    TargetRecord(
                        label="//{}:{}".format(package_path, bazel_name),
                        rule_class="rust_library",
                        package=package_name,
                        semantic="cargo-target=lib:{}".format(cargo_name),
                    ),
                )
                if target.get("test", True):
                    _add_expected(
                        expected,
                        TargetRecord(
                            label="//{}:{}_test".format(package_path, bazel_name),
                            rule_class="rust_test",
                            package=package_name,
                            semantic="cargo-test=unit:lib:{}".format(cargo_name),
                        ),
                    )
            elif "bin" in kinds:
                bazel_name = "{}_bin".format(cargo_name) if has_library else cargo_name
                _add_expected(
                    expected,
                    TargetRecord(
                        label="//{}:{}".format(package_path, bazel_name),
                        rule_class="rust_binary",
                        package=package_name,
                        semantic="cargo-target=bin:{}".format(cargo_name),
                    ),
                )
                if target.get("test", True):
                    _add_expected(
                        expected,
                        TargetRecord(
                            label="//{}:{}_test".format(package_path, bazel_name),
                            rule_class="rust_test",
                            package=package_name,
                            semantic="cargo-test=unit:bin:{}".format(cargo_name),
                        ),
                    )
            elif "test" in kinds:
                _add_expected(
                    expected,
                    TargetRecord(
                        label="//{}:{}_test".format(package_path, cargo_name),
                        rule_class="rust_test",
                        package=package_name,
                        semantic="cargo-test=integration:{}".format(cargo_name),
                    ),
                )
    return expected


def _add_expected(expected: dict[str, TargetRecord], record: TargetRecord) -> None:
    if record.label in expected:
        raise ValueError("duplicate expected Bazel label: {}".format(record.label))
    expected[record.label] = record


def observed_targets(xml_text: str) -> dict[str, TargetRecord]:
    root = ET.fromstring(xml_text)
    observed: dict[str, TargetRecord] = {}
    for rule in root.findall(".//rule"):
        tags = {
            item.attrib["value"]
            for item in rule.findall("./list[@name='tags']/string")
            if "value" in item.attrib
        }
        package_tags = sorted(tag for tag in tags if tag.startswith("cargo-package="))
        semantic_tags = sorted(
            tag
            for tag in tags
            if tag.startswith("cargo-target=") or tag.startswith("cargo-test=")
        )
        if not package_tags and not semantic_tags:
            continue
        label = rule.attrib["name"]
        if len(package_tags) != 1 or len(semantic_tags) != 1:
            raise ValueError(
                "{} must have exactly one cargo-package tag and one cargo-target/cargo-test tag; found {}"
                .format(label, sorted(tags))
            )
        observed[label] = TargetRecord(
            label=label,
            rule_class=rule.attrib["class"],
            package=package_tags[0].split("=", 1)[1],
            semantic=semantic_tags[0],
        )
    return observed


def compare_targets(
    expected: dict[str, TargetRecord], observed: dict[str, TargetRecord]
) -> list[str]:
    errors: list[str] = []
    expected_labels = set(expected)
    observed_labels = set(observed)
    for label in sorted(expected_labels - observed_labels):
        errors.append("missing Bazel target: {} ({})".format(label, expected[label].semantic))
    for label in sorted(observed_labels - expected_labels):
        errors.append("extra parity-tagged Bazel target: {} ({})".format(label, observed[label].semantic))
    for label in sorted(expected_labels & observed_labels):
        wanted = expected[label]
        found = observed[label]
        if wanted != found:
            errors.append(
                "target mismatch for {}: expected {}, observed {}".format(
                    label,
                    wanted,
                    found,
                )
            )
    return errors


def render_result(
    expected: dict[str, TargetRecord], observed: dict[str, TargetRecord], errors: list[str]
) -> dict:
    build_targets = sum(
        record.semantic.startswith("cargo-target=") for record in expected.values()
    )
    test_targets = len(expected) - build_targets
    return {
        "schema": "a2.cargo-bazel-target-parity.v1",
        "passed": not errors,
        "cargo_build_targets": build_targets,
        "cargo_test_targets": test_targets,
        "expected_target_count": len(expected),
        "observed_target_count": len(observed),
        "errors": errors,
        "targets": [record.__dict__ for record in sorted(expected.values(), key=lambda x: x.label)],
    }


class ParityTests(unittest.TestCase):
    def sample_metadata(self) -> dict:
        return {
            "packages": [
                {
                    "name": "demo",
                    "manifest_path": "/workspace/crates/demo/Cargo.toml",
                    "targets": [
                        {"name": "demo", "kind": ["lib"]},
                        {"name": "demo", "kind": ["bin"]},
                        {"name": "integration", "kind": ["test"]},
                    ],
                }
            ]
        }

    def sample_xml(self) -> str:
        records = expected_targets(self.sample_metadata(), Path("/workspace"))
        rules = []
        for record in records.values():
            rules.append(
                '<rule class="{}" name="{}"><list name="tags">'
                '<string value="cargo-package={}"/><string value="{}"/>'
                "</list></rule>".format(
                    record.rule_class,
                    record.label,
                    record.package,
                    record.semantic,
                )
            )
        return "<query>{}</query>".format("".join(rules))

    def test_matching_inventory_passes(self) -> None:
        expected = expected_targets(self.sample_metadata(), Path("/workspace"))
        observed = observed_targets(self.sample_xml())
        self.assertEqual(compare_targets(expected, observed), [])
        self.assertEqual(len(expected), 5)

    def test_missing_target_fails(self) -> None:
        expected = expected_targets(self.sample_metadata(), Path("/workspace"))
        observed = observed_targets(self.sample_xml())
        observed.pop("//crates/demo:demo_bin_test")
        self.assertIn("missing Bazel target", compare_targets(expected, observed)[0])

    def test_extra_target_fails(self) -> None:
        expected = expected_targets(self.sample_metadata(), Path("/workspace"))
        observed = observed_targets(self.sample_xml())
        observed["//crates/demo:extra"] = TargetRecord(
            "//crates/demo:extra",
            "rust_test",
            "demo",
            "cargo-test=unit:bin:extra",
        )
        self.assertIn("extra parity-tagged", compare_targets(expected, observed)[0])

    def test_kind_or_semantic_mismatch_fails(self) -> None:
        expected = expected_targets(self.sample_metadata(), Path("/workspace"))
        observed = observed_targets(self.sample_xml())
        record = observed["//crates/demo:demo"]
        observed[record.label] = TargetRecord(
            record.label,
            "rust_binary",
            record.package,
            "cargo-target=bin:demo",
        )
        self.assertIn("target mismatch", compare_targets(expected, observed)[0])

    def test_unsupported_cargo_target_kind_fails_closed(self) -> None:
        metadata = self.sample_metadata()
        metadata["packages"][0]["targets"].append(
            {"name": "demo_example", "kind": ["example"]}
        )
        with self.assertRaisesRegex(ValueError, "unsupported Cargo target kind"):
            expected_targets(metadata, Path("/workspace"))

    def test_test_false_does_not_invent_unit_test_target(self) -> None:
        metadata = self.sample_metadata()
        metadata["packages"][0]["targets"][1]["test"] = False
        expected = expected_targets(metadata, Path("/workspace"))
        self.assertNotIn("//crates/demo:demo_bin_test", expected)
        self.assertEqual(len(expected), 4)


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--workspace", type=Path, default=Path.cwd())
    parser.add_argument("--bazel-output-user-root")
    parser.add_argument("--json", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(ParityTests)
        return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1

    workspace = args.workspace.resolve()
    expected = expected_targets(cargo_metadata(workspace), workspace)
    observed = observed_targets(
        bazel_query_xml(workspace, args.bazel_output_user_root)
    )
    errors = compare_targets(expected, observed)
    result = render_result(expected, observed, errors)
    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    elif errors:
        print("Cargo/Bazel target parity: FAIL", file=sys.stderr)
        for error in errors:
            print("- {}".format(error), file=sys.stderr)
    else:
        print(
            "Cargo/Bazel target parity: PASS ({} build targets, {} test targets)".format(
                result["cargo_build_targets"],
                result["cargo_test_targets"],
            )
        )
    return 0 if not errors else 1


if __name__ == "__main__":
    raise SystemExit(main())
