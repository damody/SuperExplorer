#!/usr/bin/env python3
"""Append a hash-linked local contract result to the OpenSpec evidence ledger."""

from __future__ import annotations

import argparse
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path

try:
    from scripts.evidence_index_validator import canonical_event_sha256
except ModuleNotFoundError:
    from evidence_index_validator import canonical_event_sha256


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("task_id")
    parser.add_argument("--index", type=Path, required=True)
    parser.add_argument("--result-root", type=Path, required=True)
    parser.add_argument("--depends-on", action="append", default=[])
    parser.add_argument("--subcheck-prefix", default="evidence-governance")
    parser.add_argument("--gate-id", default="evidence-governance-local-contract")
    parser.add_argument("--command")
    arguments = parser.parse_args()
    lines = [line for line in arguments.index.read_text(encoding="utf-8-sig").splitlines() if line.strip()]
    records = [json.loads(line) for line in lines]
    result_relative = f"{arguments.task_id}/result.json"
    result_path = arguments.result_root / result_relative
    result_bytes = result_path.read_bytes()
    report = json.loads(result_bytes.decode("utf-8-sig"))
    if report.get("task_id") != arguments.task_id or report.get("exit_code") != 0 or report.get("actual") != "passed":
        raise SystemExit("refusing to append a non-passing or mismatched local result")
    digest = hashlib.sha256(result_bytes).hexdigest()
    previous_for_task = next((canonical_event_sha256(item) for item in reversed(records) if item["task_id"] == arguments.task_id), None)
    previous_index = canonical_event_sha256(records[-1]) if records else None
    now = datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    record = {
        "schema_version": 1,
        "event_id": f"local-{now.replace(':', '').replace('-', '')}-{arguments.task_id}-{digest[:12]}",
        "previous_event_sha256": previous_for_task,
        "previous_index_sha256": previous_index,
        "task_id": arguments.task_id,
        "record_kind": "leaf-result",
        "priority": "P0",
        "release_blocking": True,
        "mandatory": True,
        "status": "passed",
        "gate_id": arguments.gate_id,
        "procedure_kind": "command",
        "subcheck_key": f"{arguments.subcheck_prefix}-{arguments.task_id}",
        "artifact_or_command": arguments.command or f"powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run_evidence_governance_contract.ps1 -TaskId {arguments.task_id}",
        "cwd": ".",
        "environment": {"uitest_executed": "false", "validation_authority": "local-only"},
        "expected_exit_and_artifacts": f"exit 0; target/openspec-evidence/build-extensible-plugin-platform/{result_relative}",
        "expected": "local governance contract passes without UITEST",
        "actual": f"passed; result sha256={digest}",
        "exit_code_or_reviewer": 0,
        "sha256": digest,
        "local_result_path": result_relative,
        "immutable_locator": None,
        "artifact_manifest_sha256": None,
        "artifact_manifest_locator": None,
        "retention_policy": "local-rerunnable-result",
        "related_gates": [arguments.gate_id],
        "adjustment_id": "A-local-validation-20260804",
        "timestamp": now,
        "evidence_scope": "production",
        "depends_on": arguments.depends_on,
    }
    with arguments.index.open("a", encoding="utf-8", newline="\n") as output:
        output.write(json.dumps(record, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n")
    print(f"APPENDED {arguments.task_id} {canonical_event_sha256(record)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
