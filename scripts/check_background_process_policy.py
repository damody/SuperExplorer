#!/usr/bin/env python3
"""Reject unclassified production process launches and policy drift."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

COMMAND_PATTERN = re.compile(r"(?:std::process::)?Command::new\s*\(")
TEST_MODULE_PATTERN = re.compile(r"^\s*mod\s+tests\s*\{", re.MULTILINE)


def _normalized(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def _production_prefix(text: str) -> str:
    match = TEST_MODULE_PATTERN.search(text)
    return text if match is None else text[: match.start()]


def validate(root: Path, inventory_path: Path) -> list[str]:
    inventory = json.loads(inventory_path.read_text(encoding="utf-8"))
    sites = inventory.get("sites", [])
    file_classes = {
        entry["path"]: entry["classification"]
        for entry in inventory.get("file_classifications", [])
    }
    expected_by_path: dict[str, list[dict[str, str]]] = {}
    errors: list[str] = []
    for site in sites:
        expected_by_path.setdefault(site["path"], []).append(site)
        if site["classification"] not in {"hidden-background", "explicit-visible"}:
            errors.append(f"{site['path']}: invalid production classification")

    for contract in inventory.get("source_contracts", []):
        relative = contract["path"]
        path = root / relative
        if not path.is_file():
            errors.append(f"{relative}: contracted source file does not exist")
            continue
        text = _production_prefix(path.read_text(encoding="utf-8"))
        forbidden = contract.get("forbidden_anchor")
        if forbidden is not None and forbidden in text:
            errors.append(f"{relative}: forbidden source policy remains: {forbidden}")
        required = contract.get("required_anchor")
        if required is not None:
            count = text.count(required)
            expected_count = contract.get("required_count", 1)
            if count != expected_count:
                errors.append(
                    f"{relative}: required policy count for {required!r} is {count}, expected {expected_count}"
                )

    discovered: set[tuple[str, str]] = set()
    crates = root / "crates"
    for path in sorted(crates.rglob("*.rs")):
        relative = _normalized(path, root)
        if path.name == "build.rs" or "/tests/" in f"/{relative}":
            continue
        if relative in file_classes:
            continue
        text = path.read_text(encoding="utf-8")
        production = _production_prefix(text)
        expected = expected_by_path.get(relative, [])
        for line_number, line in enumerate(production.splitlines(), start=1):
            if COMMAND_PATTERN.search(line) is None:
                continue
            matching = [site for site in expected if site["anchor"] in line]
            if len(matching) != 1:
                errors.append(f"{relative}:{line_number}: unclassified production process launch")
                continue
            site = matching[0]
            key = (relative, site["anchor"])
            if key in discovered:
                errors.append(f"{relative}:{line_number}: duplicate inventory anchor")
            discovered.add(key)

        for site in expected:
            key = (relative, site["anchor"])
            if key not in discovered:
                errors.append(f"{relative}: inventory anchor not found: {site['anchor']}")
            required = site["required_anchor"]
            if required not in production:
                errors.append(
                    f"{relative}: {site['classification']} site lacks required policy: {required}"
                )

    for relative in expected_by_path:
        if not (root / relative).is_file():
            errors.append(f"{relative}: inventoried source file does not exist")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument(
        "--inventory",
        type=Path,
        default=Path("openspec/changes/hide-background-process-windows/process-launch-inventory.json"),
    )
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    inventory = arguments.inventory
    if not inventory.is_absolute():
        inventory = root / inventory
    errors = validate(root, inventory)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("background process policy inventory passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
