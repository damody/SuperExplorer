#!/usr/bin/env python3
"""Generate and validate the plugin-platform requirement/gate/task matrix."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

TASK = re.compile(r"- \[(?: |x|X|deferred)\]\s+(\d+\.\d+\.\d+)\s+(.+?)(?=(?:- \[(?: |x|X|deferred)\]\s+\d+\.\d+\.\d+)|(?:\r?\n#{2,4}\s)|\Z)", re.S)
REQUIREMENT = re.compile(r"^### Requirement:\s*(.+?)\s*$", re.M)
SCENARIO = re.compile(r"^#### Scenario:\s*(.+?)\s*$", re.M)
TOKEN = re.compile(r"[a-z0-9\u4e00-\u9fff]+", re.I)

MAJOR_CAPABILITY = {
    2: "rust-plugin-abi-and-ui-toolchain",
    3: "extension-package-and-feature-lifecycle",
    4: "extension-jobs-values-and-dynamic-columns",
    5: "extension-jobs-values-and-dynamic-columns",
    6: "source-example-plugin-suite",
    7: "extension-options-management",
    8: "extension-view-modes-and-directory-tree-scan",
    9: "extension-commands-forms-and-operation-plans",
    10: "lua-extension-registrar-and-tool-execution",
    11: "virtual-folder-stream-and-mutation",
    12: "lock-owner-host-service",
    13: "extension-skin-customization",
    14: "source-example-plugin-suite",
}


def slug(text: str) -> str:
    value = "-".join(TOKEN.findall(text.lower()))
    return value[:72].strip("-") or hashlib.sha256(text.encode()).hexdigest()[:12]


def parse_specs(spec_root: Path) -> list[dict[str, Any]]:
    output: list[dict[str, Any]] = []
    for spec_path in sorted(spec_root.glob("*/spec.md")):
        capability = spec_path.parent.name
        source = spec_path.read_text(encoding="utf-8-sig")
        matches = list(REQUIREMENT.finditer(source))
        for index, match in enumerate(matches):
            end = matches[index + 1].start() if index + 1 < len(matches) else len(source)
            title = match.group(1)
            requirement_selector = f"req:{capability}:{slug(title)}"
            scenarios = []
            for scenario in SCENARIO.findall(source[match.end():end]):
                scenarios.append({"title": scenario, "selector": f"scenario:{capability}:{slug(title)}:{slug(scenario)}"})
            output.append({"capability": capability, "title": title, "selector": requirement_selector, "scenarios": scenarios})
    return output


def parse_tasks(tasks_path: Path) -> list[tuple[str, str]]:
    source = tasks_path.read_text(encoding="utf-8-sig")
    return [(match.group(1), " ".join(match.group(2).split())) for match in TASK.finditer(source)]


def _tokens(text: str) -> set[str]:
    return {token for token in TOKEN.findall(text.lower()) if len(token) > 2}


def build_matrix(spec_root: Path, tasks_path: Path) -> dict[str, Any]:
    requirements = parse_specs(spec_root)
    by_capability: dict[str, list[dict[str, Any]]] = {}
    for requirement in requirements:
        by_capability.setdefault(requirement["capability"], []).append(requirement)
    mappings = []
    for task_id, text in parse_tasks(tasks_path):
        major = int(task_id.split(".", 1)[0])
        capability = MAJOR_CAPABILITY.get(major)
        mapping_kind = "requirement"
        requirement_selector = None
        scenario_selector = None
        if capability is None:
            mapping_kind = "governance" if major in {1, 16} else "integration"
        else:
            candidates = by_capability[capability]
            task_tokens = _tokens(text)
            ranked = []
            for position, requirement in enumerate(candidates):
                requirement_tokens = _tokens(requirement["title"] + " " + " ".join(item["title"] for item in requirement["scenarios"]))
                ranked.append((len(task_tokens & requirement_tokens), -position, requirement))
            selected = max(ranked)[2]
            requirement_selector = selected["selector"]
            if selected["scenarios"]:
                scenario_selector = max(
                    selected["scenarios"],
                    key=lambda item: (len(task_tokens & _tokens(item["title"])), -selected["scenarios"].index(item)),
                )["selector"]
        coverage_kind = "negative-or-recovery" if re.search(r"拒絕|失敗|fail|reject|stale|cancel|rollback|recovery|tamper|missing|invalid|escape|drift", text, re.I) else "positive"
        mappings.append({
            "task_id": task_id,
            "task_text_sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
            "mapping_kind": mapping_kind,
            "requirement_selector": requirement_selector,
            "scenario_selector": scenario_selector,
            "gate_id": f"gate-{task_id}",
            "coverage_kind": coverage_kind,
            "evidence_scope": "production",
            "procedure": {
                "kind": "command",
                "command": "python scripts/traceability_matrix.py --validate openspec/changes/build-extensible-plugin-platform/traceability/traceability-matrix.json --spec-root openspec/changes/build-extensible-plugin-platform/specs --tasks openspec/changes/build-extensible-plugin-platform/tasks.md",
                "cwd": ".",
                "environment": {"PYTHONUTF8": "1", "uitest_executed": "false"},
                "expected_exit_and_artifacts": "exit 0; canonical traceability-matrix.json SHA-256",
            },
        })
    return {"schema_version": 1, "requirements": requirements, "mappings": mappings}


def validate_matrix(matrix: Any, spec_root: Path, tasks_path: Path) -> list[str]:
    if not isinstance(matrix, dict) or set(matrix) != {"schema_version", "requirements", "mappings"} or matrix.get("schema_version") != 1:
        return ["matrix has invalid top-level fields"]
    expected = build_matrix(spec_root, tasks_path)
    issues: list[str] = []
    expected_requirements = {item["selector"] for item in expected["requirements"]}
    actual_requirements = {item.get("selector") for item in matrix.get("requirements", []) if isinstance(item, dict)}
    missing = expected_requirements - actual_requirements
    if missing:
        issues.append(f"missing requirement selector: {sorted(missing)[0]}")
    unknown = actual_requirements - expected_requirements
    if unknown:
        issues.append(f"unknown requirement selector: {sorted(unknown)[0]}")
    scenario_selectors = {scenario["selector"] for item in expected["requirements"] for scenario in item["scenarios"]}
    expected_tasks = {item["task_id"] for item in expected["mappings"]}
    mappings = matrix.get("mappings", [])
    actual_tasks = {item.get("task_id") for item in mappings if isinstance(item, dict)}
    if expected_tasks - actual_tasks:
        issues.append(f"orphan leaf without mapping: {sorted(expected_tasks - actual_tasks)[0]}")
    if actual_tasks - expected_tasks:
        issues.append(f"mapping references unknown leaf: {sorted(actual_tasks - expected_tasks)[0]}")
    gate_ids: set[str] = set()
    for mapping in mappings if isinstance(mappings, list) else []:
        if not isinstance(mapping, dict):
            issues.append("mapping must be an object")
            continue
        kind = mapping.get("mapping_kind")
        requirement = mapping.get("requirement_selector")
        scenario = mapping.get("scenario_selector")
        if kind == "requirement" and requirement not in expected_requirements:
            issues.append(f"unknown selector on {mapping.get('task_id')}: {requirement}")
        if kind == "requirement" and scenario not in scenario_selectors:
            issues.append(f"unknown scenario selector on {mapping.get('task_id')}: {scenario}")
        if kind in {"governance", "integration"} and (requirement is not None or scenario is not None):
            issues.append(f"{kind} leaf {mapping.get('task_id')} must not claim requirement coverage")
        if mapping.get("evidence_scope") == "trait-mock-only":
            issues.append(f"mock-only coverage cannot close leaf {mapping.get('task_id')}")
        gate = mapping.get("gate_id")
        if not isinstance(gate, str) or gate in gate_ids:
            issues.append(f"gate_id is missing or duplicate on {mapping.get('task_id')}")
        gate_ids.add(gate)
        procedure = mapping.get("procedure")
        required_procedure = {"kind", "command", "cwd", "environment", "expected_exit_and_artifacts"}
        if not isinstance(procedure, dict) or set(procedure) != required_procedure or not all(procedure.get(field) for field in ("kind", "command", "cwd", "expected_exit_and_artifacts")):
            issues.append(f"gate {gate} lacks an exact procedure")
    canonical_expected = json.dumps(expected, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    canonical_actual = json.dumps(matrix, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    if canonical_actual != canonical_expected:
        issues.append("matrix differs from deterministic spec/task generation")
    return issues


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--spec-root", type=Path, required=True)
    parser.add_argument("--tasks", type=Path, required=True)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--generate", type=Path)
    action.add_argument("--validate", type=Path)
    arguments = parser.parse_args(argv)
    if arguments.generate:
        matrix = build_matrix(arguments.spec_root, arguments.tasks)
        arguments.generate.parent.mkdir(parents=True, exist_ok=True)
        arguments.generate.write_text(json.dumps(matrix, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8", newline="\n")
        print(hashlib.sha256(arguments.generate.read_bytes()).hexdigest())
        return 0
    try:
        matrix = json.loads(arguments.validate.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"cannot read matrix: {error}", file=sys.stderr)
        return 1
    issues = validate_matrix(matrix, arguments.spec_root, arguments.tasks)
    for issue in issues:
        print(issue, file=sys.stderr)
    if not issues:
        print(hashlib.sha256(arguments.validate.read_bytes()).hexdigest())
    return 1 if issues else 0


if __name__ == "__main__":
    raise SystemExit(main())
