#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

HANDOFF_FIELDS = {"diff","test_commands","test_results","evidence_ids","known_risks","remaining_dependencies"}


def _prefix(pattern: str) -> str:
    return pattern.split("*", 1)[0].rstrip("/")


def validate_policy(policy: Any) -> list[str]:
    issues: list[str] = []
    if not isinstance(policy, dict) or policy.get("schema_version") != 1:
        return ["policy schema_version must be 1"]
    roles = policy.get("roles")
    if not isinstance(roles, list) or not roles:
        return ["policy requires roles"]
    owners: list[tuple[str,str]] = []
    for role in roles:
        if not isinstance(role, dict) or set(role) != {"role","wave","owned_paths","forbidden_paths"}:
            issues.append("role entry has invalid fields"); continue
        for path in role["owned_paths"]:
            prefix = _prefix(path)
            for other_prefix, other_role in owners:
                if prefix == other_prefix or prefix.startswith(other_prefix + "/") or other_prefix.startswith(prefix + "/"):
                    issues.append(f"mutable path ownership overlap: {other_role} and {role['role']} own {prefix}")
            owners.append((prefix, role["role"]))
    if policy.get("shared_manifest_integrator") != "release-integrator":
        issues.append("shared manifests must have release-integrator ownership")
    if set(policy.get("handoff_required_fields", [])) != HANDOFF_FIELDS:
        issues.append("handoff required fields are incomplete")
    classes = policy.get("adjustment_classes", {})
    if set(classes) != {"A","B","C"}:
        issues.append("adjustment classes A/B/C are required")
    return issues


def validate_handoff(handoff: Any) -> list[str]:
    if not isinstance(handoff, dict): return ["handoff must be an object"]
    missing = HANDOFF_FIELDS - set(handoff)
    return [f"handoff missing {field}" for field in sorted(missing)]


def validate_adjustment(adjustment: Any) -> list[str]:
    if not isinstance(adjustment, dict): return ["adjustment must be an object"]
    kind = adjustment.get("class")
    if kind == "A":
        issues=[]
        if adjustment.get("l3_ids_before") != adjustment.get("l3_ids_after"): issues.append("A refinement must preserve permanent L3 IDs")
        if adjustment.get("evidence_lineage_preserved") is not True: issues.append("A refinement must preserve evidence lineage")
        return issues
    if kind == "B":
        issues=[]
        if adjustment.get("affected_work_paused") is not True: issues.append("B correction must pause affected work")
        if not adjustment.get("stale_dependent_evidence_ids"): issues.append("B correction must mark dependent evidence stale")
        validation=adjustment.get("openspec_validation")
        if not isinstance(validation,dict) or validation.get("exit_code") != 0 or not validation.get("command"): issues.append("B correction must rerun OpenSpec validation")
        return issues
    if kind == "C":
        protected=adjustment.get("protected_change")
        if protected not in {"public-abi","gate","permission"}: return ["C change protected_change is invalid"]
        approval=adjustment.get("user_approval")
        if not isinstance(approval,dict) or not approval.get("approval_id") or approval.get("approved") is not True: return ["C change requires explicit user approval record"]
        return []
    return ["unknown adjustment class"]


def main() -> int:
    parser=argparse.ArgumentParser(); parser.add_argument("policy",type=Path); arguments=parser.parse_args()
    try: policy=json.loads(arguments.policy.read_text(encoding="utf-8-sig"))
    except (OSError,json.JSONDecodeError) as error: print(error,file=sys.stderr); return 1
    issues=validate_policy(policy)
    for issue in issues: print(issue,file=sys.stderr)
    return 1 if issues else 0

if __name__ == "__main__": raise SystemExit(main())
