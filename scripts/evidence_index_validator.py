#!/usr/bin/env python3
"""Validate the append-only evidence ledger for the plugin-platform OpenSpec change.

The JSON Schema documents one JSONL record.  This module intentionally keeps
the cross-record rules here so a release gate can validate one complete index
without a third-party Python dependency.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import urllib.request
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any, Iterable, Mapping


SCHEMA_VERSION = 1
TASK_ID_PATTERN = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
EVENT_ID_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._:-]*$")
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
RELEASE_LOCATOR_PATTERN = re.compile(r"^release://local/[A-Za-z0-9][A-Za-z0-9._/-]*#sha256=([0-9a-f]{64})$")
TERMINAL_STATUSES = {"passed", "not-applicable", "superseded"}
NON_CLOSING_STATUSES = {"failed", "blocked", "stale", "unexecuted"}
KNOWN_STATUSES = TERMINAL_STATUSES | NON_CLOSING_STATUSES
MAX_ARTIFACT_BYTES = 64 * 1024 * 1024
ARTIFACT_RETRIEVAL_TIMEOUT_SECONDS = 30
REQUIRED_FIELDS = {
    "schema_version",
    "event_id",
    "previous_event_sha256",
    "previous_index_sha256",
    "task_id",
    "record_kind",
    "priority",
    "release_blocking",
    "mandatory",
    "status",
    "gate_id",
    "procedure_kind",
    "subcheck_key",
    "artifact_or_command",
    "cwd",
    "environment",
    "expected_exit_and_artifacts",
    "expected",
    "actual",
    "exit_code_or_reviewer",
    "sha256",
    "local_result_path",
    "immutable_locator",
    "artifact_manifest_sha256",
    "artifact_manifest_locator",
    "retention_policy",
    "related_gates",
    "adjustment_id",
    "timestamp",
    "evidence_scope",
    "depends_on",
}
ALLOWED_FIELDS = REQUIRED_FIELDS | {
    "replacement_task_id",
    "not_applicable_condition",
    "stale_for_supersession_event_sha256",
    "revalidation_of_event_sha256",
    "revalidated_against_task_id",
}
ALLOWED_RETENTION_POLICIES = {
    "local-rerunnable-result",
    "signed-release-evidence-bundle",
}
PREAPPROVED_CONDITIONAL_NA_TASKS = {"16.2.2"}


@dataclass(frozen=True)
class ValidationIssue:
    line: int
    message: str

    def __str__(self) -> str:
        return f"line {self.line}: {self.message}"


def _is_non_empty_string(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def canonical_event_sha256(record: Mapping[str, Any]) -> str:
    """Return the hash used by a later event's previous_event_sha256 pointer."""
    source = json.dumps(record, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(source).hexdigest()


def _is_valid_locator(locator: Any) -> bool:
    return isinstance(locator, str) and RELEASE_LOCATOR_PATTERN.fullmatch(locator) is not None


def _locator_digest(locator: str) -> str | None:
    release = RELEASE_LOCATOR_PATTERN.fullmatch(locator)
    return release.group(1) if release else None


def _validate_record_shape(record: Any, line: int) -> list[ValidationIssue]:
    if not isinstance(record, dict):
        return [ValidationIssue(line, "record must be a JSON object")]

    issues: list[ValidationIssue] = []
    for field in sorted(REQUIRED_FIELDS - record.keys()):
        issues.append(ValidationIssue(line, f"missing required field: {field}"))
    for field in sorted(record.keys() - ALLOWED_FIELDS):
        issues.append(ValidationIssue(line, f"unknown field: {field}"))
    if issues:
        return issues

    if record["schema_version"] != SCHEMA_VERSION:
        issues.append(ValidationIssue(line, f"schema_version must be {SCHEMA_VERSION}"))
    if not _is_non_empty_string(record["event_id"]) or not EVENT_ID_PATTERN.fullmatch(record["event_id"]):
        issues.append(ValidationIssue(line, "event_id must be a stable event identity"))
    previous_event = record["previous_event_sha256"]
    if previous_event is not None and (not isinstance(previous_event, str) or not SHA256_PATTERN.fullmatch(previous_event)):
        issues.append(ValidationIssue(line, "previous_event_sha256 must be null or lowercase SHA-256"))
    previous_index = record["previous_index_sha256"]
    if previous_index is not None and (not isinstance(previous_index, str) or not SHA256_PATTERN.fullmatch(previous_index)):
        issues.append(ValidationIssue(line, "previous_index_sha256 must be null or lowercase SHA-256"))
    if not _is_non_empty_string(record["task_id"]) or not TASK_ID_PATTERN.fullmatch(record["task_id"]):
        issues.append(ValidationIssue(line, "task_id must be a three-level numeric L3 ID"))
    record_kind = record["record_kind"]
    if record_kind not in {"leaf-result", "retained-bundle"}:
        issues.append(ValidationIssue(line, "record_kind must be leaf-result or retained-bundle"))
    if record["priority"] not in {"P0", "P1"}:
        issues.append(ValidationIssue(line, "priority must be P0 or P1"))
    for field in ("release_blocking", "mandatory"):
        if not isinstance(record[field], bool):
            issues.append(ValidationIssue(line, f"{field} must be boolean"))
    if record["status"] not in KNOWN_STATUSES:
        issues.append(ValidationIssue(line, "status is unknown"))
    for field in ("gate_id", "subcheck_key", "artifact_or_command", "cwd", "expected_exit_and_artifacts", "expected", "actual", "adjustment_id"):
        if not _is_non_empty_string(record[field]):
            issues.append(ValidationIssue(line, f"{field} must be a non-empty string"))
    procedure_kind = record["procedure_kind"]
    if procedure_kind not in {"command", "manual"}:
        issues.append(ValidationIssue(line, "procedure_kind must be command or manual"))
    if not isinstance(record["environment"], dict) or not all(
        isinstance(key, str) and isinstance(value, str) for key, value in record["environment"].items()
    ):
        issues.append(ValidationIssue(line, "environment must be an object with string values"))
    reviewer = record["exit_code_or_reviewer"]
    if procedure_kind == "command" and (isinstance(reviewer, bool) or not isinstance(reviewer, int)):
        issues.append(ValidationIssue(line, "command procedure requires integer exit_code_or_reviewer and exact artifact_or_command command"))
    elif procedure_kind == "manual" and not _is_non_empty_string(reviewer):
        issues.append(ValidationIssue(line, "manual procedure requires reviewer identity and exact artifact_or_command procedure/report"))
    elif procedure_kind not in {"command", "manual"} and (isinstance(reviewer, bool) or not isinstance(reviewer, (int, str)) or isinstance(reviewer, str) and not reviewer.strip()):
        issues.append(ValidationIssue(line, "exit_code_or_reviewer must be an exit code or reviewer"))
    if not isinstance(record["sha256"], str) or not SHA256_PATTERN.fullmatch(record["sha256"]):
        issues.append(ValidationIssue(line, "sha256 must be lowercase SHA-256"))
    local_result_path = record["local_result_path"]
    locator = record["immutable_locator"]
    manifest_hash = record["artifact_manifest_sha256"]
    manifest_locator = record["artifact_manifest_locator"]
    retention_policy = record["retention_policy"]
    if record_kind == "leaf-result":
        if not isinstance(local_result_path, str) or local_result_path != f"{record['task_id']}/result.json":
            issues.append(ValidationIssue(line, "leaf-result local_result_path must be <task_id>/result.json"))
        if locator is not None or manifest_hash is not None or manifest_locator is not None:
            issues.append(ValidationIssue(line, "leaf-result must not require retained-bundle locators or manifest hash"))
        if retention_policy != "local-rerunnable-result":
            issues.append(ValidationIssue(line, "leaf-result retention_policy must be local-rerunnable-result"))
    elif record_kind == "retained-bundle":
        if local_result_path is not None:
            issues.append(ValidationIssue(line, "retained-bundle local_result_path must be null"))
        if not _is_valid_locator(locator):
            issues.append(ValidationIssue(line, "immutable_locator must be a content-addressed local release:// locator"))
        elif _locator_digest(locator) != record["sha256"]:
            issues.append(ValidationIssue(line, "immutable_locator SHA-256 fragment must equal sha256"))
        if isinstance(locator, str) and re.search(r"(?:^|[\\/])target(?:[\\/]|$)", locator, flags=re.IGNORECASE):
            issues.append(ValidationIssue(line, "immutable_locator must not point into a mutable target/ path"))
        if not isinstance(manifest_hash, str) or not SHA256_PATTERN.fullmatch(manifest_hash):
            issues.append(ValidationIssue(line, "artifact_manifest_sha256 must be lowercase SHA-256"))
        if not _is_valid_locator(manifest_locator):
            issues.append(ValidationIssue(line, "artifact_manifest_locator must be a content-addressed local release:// locator"))
        elif _locator_digest(manifest_locator) != manifest_hash:
            issues.append(ValidationIssue(line, "artifact_manifest_locator SHA-256 fragment must equal artifact_manifest_sha256"))
        if isinstance(manifest_locator, str) and re.search(r"(?:^|[\\/])target(?:[\\/]|$)", manifest_locator, flags=re.IGNORECASE):
            issues.append(ValidationIssue(line, "artifact_manifest_locator must not point into a mutable target/ path"))
        if retention_policy != "signed-release-evidence-bundle":
            issues.append(ValidationIssue(line, "retained-bundle retention_policy must be signed-release-evidence-bundle"))
    elif retention_policy not in ALLOWED_RETENTION_POLICIES:
        issues.append(ValidationIssue(line, "retention_policy is unknown"))
    related_gates = record["related_gates"]
    if not isinstance(related_gates, list) or not related_gates or not all(_is_non_empty_string(gate) for gate in related_gates):
        issues.append(ValidationIssue(line, "related_gates must be a non-empty string array"))
    elif len(set(related_gates)) != len(related_gates):
        issues.append(ValidationIssue(line, "related_gates must not contain duplicates"))
    elif record["gate_id"] not in related_gates:
        issues.append(ValidationIssue(line, "related_gates must include gate_id"))
    if record["evidence_scope"] not in {"production", "audit-backfill", "trait-mock-only"}:
        issues.append(ValidationIssue(line, "evidence_scope is unknown"))
    task_major = int(record["task_id"].split(".", 1)[0]) if isinstance(record["task_id"], str) and TASK_ID_PATTERN.fullmatch(record["task_id"]) else None
    command_text = record["artifact_or_command"].lower() if isinstance(record["artifact_or_command"], str) else ""
    requests_uitest = "explorer-uitest" in command_text or re.search(r"(?:^|[^a-z])uitest(?:[^a-z]|$)", command_text) is not None
    if requests_uitest and (task_major is not None and task_major <= 5 or task_major == 6 and record["task_id"] != "6.4.7"):
        issues.append(ValidationIssue(line, "UITEST execution is ineligible before Task 6 final gate 6.4.7"))
    dependencies = record["depends_on"]
    if not isinstance(dependencies, list) or not all(isinstance(task, str) and TASK_ID_PATTERN.fullmatch(task) for task in dependencies):
        issues.append(ValidationIssue(line, "depends_on must contain L3 task IDs"))
    elif len(set(dependencies)) != len(dependencies):
        issues.append(ValidationIssue(line, "depends_on must not contain duplicates"))
    timestamp = record["timestamp"]
    if not _is_non_empty_string(timestamp) or not timestamp.endswith("Z"):
        issues.append(ValidationIssue(line, "timestamp must be an ISO-8601 UTC timestamp ending in Z"))
    else:
        try:
            datetime.fromisoformat(timestamp.removesuffix("Z") + "+00:00")
        except ValueError:
            issues.append(ValidationIssue(line, "timestamp must be a valid ISO-8601 UTC timestamp"))
    stale_event_hash = record.get("stale_for_supersession_event_sha256")
    if stale_event_hash is not None and (not isinstance(stale_event_hash, str) or not SHA256_PATTERN.fullmatch(stale_event_hash)):
        issues.append(ValidationIssue(line, "stale_for_supersession_event_sha256 must be lowercase SHA-256"))
    revalidation_event_hash = record.get("revalidation_of_event_sha256")
    revalidated_task_id = record.get("revalidated_against_task_id")
    if revalidation_event_hash is not None or revalidated_task_id is not None:
        if not isinstance(revalidation_event_hash, str) or not SHA256_PATTERN.fullmatch(revalidation_event_hash) or not isinstance(revalidated_task_id, str) or not TASK_ID_PATTERN.fullmatch(revalidated_task_id):
            issues.append(ValidationIssue(line, "revalidation metadata requires stale-event SHA-256 and replacement L3 task ID"))
    return issues


def _validate_cross_record_rules(records: list[tuple[int, dict[str, Any]]]) -> list[ValidationIssue]:
    issues: list[ValidationIssue] = []
    event_lines: dict[str, int] = {}
    task_events: dict[str, list[tuple[int, dict[str, Any]]]] = {}
    for line, record in records:
        event_id = record["event_id"]
        if event_id in event_lines:
            issues.append(ValidationIssue(line, f"duplicate evidence event_id {event_id}; first appears on line {event_lines[event_id]}"))
        else:
            event_lines[event_id] = line
        task_events.setdefault(record["task_id"], []).append((line, record))

    latest_by_task = {task_id: events[-1] for task_id, events in task_events.items()}
    for position, (line, record) in enumerate(records):
        expected_hash = None if position == 0 else canonical_event_sha256(records[position - 1][1])
        if record["previous_index_sha256"] != expected_hash:
            issues.append(ValidationIssue(line, "previous_index_sha256 must reference the immediately preceding JSONL event"))
    for task_id, events in task_events.items():
        for position, (line, record) in enumerate(events):
            previous_hash = record["previous_event_sha256"]
            if position == 0:
                if previous_hash is not None:
                    issues.append(ValidationIssue(line, f"first event for task {task_id} must have null previous_event_sha256"))
                continue
            prior_line, prior_record = events[position - 1]
            expected_hash = canonical_event_sha256(prior_record)
            if previous_hash != expected_hash:
                issues.append(ValidationIssue(line, f"previous_event_sha256 must reference prior event on line {prior_line} for task {task_id}"))
    subcheck_lines: dict[str, tuple[int, str]] = {}
    for line, record in records:
        if record["status"] not in TERMINAL_STATUSES:
            continue
        task_id = record["task_id"]
        subcheck_key = record["subcheck_key"]
        existing = subcheck_lines.get(subcheck_key)
        if existing is not None:
            issues.append(ValidationIssue(line, f"subcheck_key {subcheck_key} closes more than one L3 ({existing[1]} on line {existing[0]} and {task_id})"))
        else:
            subcheck_lines[subcheck_key] = (line, task_id)

    for line, record in records:
        status = record["status"]
        if status == "passed" and isinstance(record["exit_code_or_reviewer"], int) and record["exit_code_or_reviewer"] != 0:
            issues.append(ValidationIssue(line, "passed command evidence must have exit code 0"))
        if status == "not-applicable":
            condition = record.get("not_applicable_condition")
            if not _is_non_empty_string(condition) or not isinstance(record["exit_code_or_reviewer"], str):
                issues.append(ValidationIssue(line, "not-applicable evidence requires an environment condition and reviewer"))
        if status == "superseded":
            replacement = record.get("replacement_task_id")
            if not isinstance(replacement, str) or not TASK_ID_PATTERN.fullmatch(replacement) or replacement == record["task_id"]:
                issues.append(ValidationIssue(line, "superseded evidence requires one distinct replacement_task_id"))
            elif replacement not in latest_by_task:
                issues.append(ValidationIssue(line, f"replacement_task_id {replacement} has no evidence record"))

    replacement_edges = {
        task_id: record["replacement_task_id"]
        for task_id, (_, record) in latest_by_task.items()
        if record["status"] == "superseded" and isinstance(record.get("replacement_task_id"), str)
    }
    for source_task in replacement_edges:
        seen: set[str] = set()
        task_id = source_task
        while task_id in replacement_edges:
            if task_id in seen:
                issues.append(ValidationIssue(latest_by_task[source_task][0], f"supersession replacement cycle includes {task_id}"))
                break
            seen.add(task_id)
            task_id = replacement_edges[task_id]
    return issues


def completion_issues(
    records: list[tuple[int, dict[str, Any]]],
    policy_tasks: Mapping[str, Mapping[str, Any]] | None = None,
    selected_task_ids: set[str] | None = None,
) -> list[ValidationIssue]:
    """Return reasons the latest event for an L3 cannot resolve it complete.

    Historical failed, blocked, stale, unexecuted, and trait/mock-only events
    are valid append-only history.  They fail only when they are the latest
    state being offered as task completion.
    """
    latest_by_task = _latest_by_task(records)
    issues: list[ValidationIssue] = []
    for task_id, (line, record) in latest_by_task.items():
        if selected_task_ids is not None and task_id not in selected_task_ids:
            continue
        status = record["status"]
        if status in NON_CLOSING_STATUSES:
            issues.append(ValidationIssue(line, f"latest status {status} cannot resolve an L3 complete"))
        if record["evidence_scope"] == "trait-mock-only":
            issues.append(ValidationIssue(line, "latest trait/mock-only evidence cannot resolve an L3 complete"))
        if record["status"] == "not-applicable":
            authority = policy_tasks.get(record["task_id"]) if policy_tasks else None
            approval = authority.get("not_applicable") if authority else None
            if authority and authority.get("mandatory"):
                issues.append(ValidationIssue(line, "authoritative mandatory P0/P1 leaves cannot be not-applicable"))
            elif not isinstance(approval, dict) or not approval.get("approved") or record["adjustment_id"] != approval.get("approval_id"):
                issues.append(ValidationIssue(line, "not-applicable completion requires matching authoritative policy approval"))
    return issues


def _task_ids_from_plan(path: Path) -> set[str]:
    try:
        source = path.read_text(encoding="utf-8-sig")
    except OSError as error:
        raise ValueError(f"cannot read task plan {path}: {error}") from error
    return set(re.findall(r"(?m)^\s*- \[[ xX]\]\s+([0-9]+\.[0-9]+\.[0-9]+)\b", source))


def _lineage_mapping_issues(path: Path, known_task_ids: set[str]) -> list[ValidationIssue]:
    """Validate legacy-lineage-map-v1 targets without treating the map as evidence."""
    try:
        mapping = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        return [ValidationIssue(0, f"cannot read lineage mapping {path}: {error}")]
    entries = mapping.get("entries") if isinstance(mapping, dict) else None
    if not isinstance(entries, list):
        return [ValidationIssue(0, "lineage mapping must contain an entries array")]
    issues: list[ValidationIssue] = []
    for number, entry in enumerate(entries, start=1):
        if not isinstance(entry, dict) or not isinstance(entry.get("new_l3_ids"), list):
            issues.append(ValidationIssue(0, f"lineage mapping entry {number} must contain new_l3_ids"))
            continue
        for task_id in entry["new_l3_ids"]:
            if not isinstance(task_id, str) or not TASK_ID_PATTERN.fullmatch(task_id) or task_id not in known_task_ids:
                issues.append(ValidationIssue(0, f"lineage mapping entry {number} targets unknown L3 ID {task_id!r}"))
    return issues


def _retrieve_bounded(source: str, maximum_bytes: int = MAX_ARTIFACT_BYTES) -> bytes:
    with urllib.request.urlopen(source, timeout=ARTIFACT_RETRIEVAL_TIMEOUT_SECONDS) as response:
        content_length = response.headers.get("Content-Length")
        if content_length and (not content_length.isdecimal() or int(content_length) > maximum_bytes):
            raise OSError(f"response exceeds {maximum_bytes} byte retrieval limit")
        chunks: list[bytes] = []
        total_bytes = 0
        while chunk := response.read(min(1024 * 1024, maximum_bytes + 1)):
            total_bytes += len(chunk)
            if total_bytes > maximum_bytes:
                raise OSError(f"response exceeds {maximum_bytes} byte retrieval limit")
            chunks.append(chunk)
    return b"".join(chunks)


def _verify_artifacts(records: Iterable[tuple[int, dict[str, Any]]], locator_map: Mapping[str, str]) -> list[ValidationIssue]:
    issues: list[ValidationIssue] = []
    for line, record in records:
        if record["record_kind"] != "retained-bundle":
            continue
        locator = record["immutable_locator"]
        manifest_locator = record["artifact_manifest_locator"]
        try:
            artifact_bytes = _retrieve_bounded(locator_map[locator])
            manifest_bytes = _retrieve_bounded(locator_map[manifest_locator])
        except KeyError:
            issues.append(ValidationIssue(line, "test locator map is unavailable for a local release evidence locator"))
            continue
        except OSError as error:
            issues.append(ValidationIssue(line, f"cannot retrieve immutable evidence for {locator}: {error}"))
            continue
        if hashlib.sha256(artifact_bytes).hexdigest() != record["sha256"]:
            issues.append(ValidationIssue(line, f"retrieved artifact SHA-256 mismatch for {locator}"))
            continue
        if hashlib.sha256(manifest_bytes).hexdigest() != record["artifact_manifest_sha256"]:
            issues.append(ValidationIssue(line, f"retrieved artifact manifest SHA-256 mismatch for {manifest_locator}"))
            continue
        try:
            manifest = json.loads(manifest_bytes.decode("utf-8-sig"))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            issues.append(ValidationIssue(line, f"artifact manifest is not valid UTF-8 JSON: {error}"))
            continue
        if not isinstance(manifest, dict) or manifest.get("schema_version") != 1:
            issues.append(ValidationIssue(line, "artifact manifest must be a schema_version 1 object"))
            continue
        if manifest.get("subcheck_key") != record["subcheck_key"]:
            issues.append(ValidationIssue(line, "artifact manifest subcheck_key does not bind evidence subcheck_key"))
        if manifest.get("artifact_sha256") != record["sha256"] or manifest.get("artifact_locator") != locator:
            issues.append(ValidationIssue(line, "artifact manifest does not bind the retained artifact bytes and locator"))
    return issues


def _verify_local_results(records: Iterable[tuple[int, dict[str, Any]]], result_root: Path) -> list[ValidationIssue]:
    issues: list[ValidationIssue] = []
    try:
        resolved_root = result_root.resolve(strict=True)
    except OSError as error:
        return [ValidationIssue(0, f"cannot resolve local result root {result_root}: {error}")]
    for line, record in records:
        if record["record_kind"] != "leaf-result":
            continue
        relative_path = Path(record["local_result_path"])
        try:
            result_path = (resolved_root / relative_path).resolve(strict=True)
            result_path.relative_to(resolved_root)
            result_bytes = result_path.read_bytes()
        except (OSError, ValueError) as error:
            issues.append(ValidationIssue(line, f"cannot read contained local result {relative_path.as_posix()}: {error}"))
            continue
        if len(result_bytes) > MAX_ARTIFACT_BYTES:
            issues.append(ValidationIssue(line, "local result exceeds retrieval size limit"))
            continue
        if hashlib.sha256(result_bytes).hexdigest() != record["sha256"]:
            issues.append(ValidationIssue(line, "local result SHA-256 does not match evidence sha256"))
            continue
        try:
            report = json.loads(result_bytes.decode("utf-8-sig"))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            issues.append(ValidationIssue(line, f"local result is not valid UTF-8 JSON: {error}"))
            continue
        if not isinstance(report, dict) or report.get("schema_version") != 1 or report.get("task_id") != record["task_id"]:
            issues.append(ValidationIssue(line, "local result does not bind schema_version and task_id"))
            continue
        if report.get("exit_code") != record["exit_code_or_reviewer"]:
            issues.append(ValidationIssue(line, "local result exit_code does not match evidence record"))
        report_environment = report.get("environment")
        if not isinstance(report_environment, dict) or report_environment.get("uitest_executed") != record["environment"].get("uitest_executed"):
            issues.append(ValidationIssue(line, "local result environment does not bind uitest_executed"))
        if record["status"] == "passed" and (report.get("actual") != "passed" or not record["actual"].lower().startswith("passed")):
            issues.append(ValidationIssue(line, "passed evidence must bind a local result whose actual outcome is passed"))
    return issues


def _load_authoritative_policy(path: Path) -> tuple[dict[str, Any] | None, list[ValidationIssue]]:
    try:
        policy = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        return None, [ValidationIssue(0, f"cannot read authoritative policy {path}: {error}")]
    if not isinstance(policy, dict):
        return None, [ValidationIssue(0, "authoritative policy must be a JSON object")]
    required = {"schema_version", "required_prefix_event_sha256", "expected_ledger_sha256", "tasks"}
    if set(policy) != required or policy.get("schema_version") != 1:
        return None, [ValidationIssue(0, "authoritative policy has an invalid schema or fields")]
    if not all(isinstance(policy[key], str) and SHA256_PATTERN.fullmatch(policy[key]) for key in ("required_prefix_event_sha256", "expected_ledger_sha256")):
        return None, [ValidationIssue(0, "authoritative policy checkpoints must be lowercase SHA-256")]
    if not isinstance(policy["tasks"], list) or not policy["tasks"]:
        return None, [ValidationIssue(0, "authoritative policy must register at least one L3 task")]
    task_ids: set[str] = set()
    for task in policy["tasks"]:
        if not isinstance(task, dict) or not {"task_id", "priority", "release_blocking", "mandatory", "depends_on", "gate_ids"}.issubset(task):
            return None, [ValidationIssue(0, "authoritative policy task entry is incomplete")]
        if not isinstance(task["task_id"], str) or not TASK_ID_PATTERN.fullmatch(task["task_id"]) or task["task_id"] in task_ids:
            return None, [ValidationIssue(0, "authoritative policy task IDs must be unique L3 IDs")]
        if task["priority"] not in {"P0", "P1"} or not isinstance(task["release_blocking"], bool) or not isinstance(task["mandatory"], bool):
            return None, [ValidationIssue(0, "authoritative policy task priority/blocking/mandatory fields are invalid")]
        if not isinstance(task["depends_on"], list) or not isinstance(task["gate_ids"], list) or not task["gate_ids"]:
            return None, [ValidationIssue(0, "authoritative policy task dependencies or gates are invalid")]
        if not all(isinstance(dependency, str) and TASK_ID_PATTERN.fullmatch(dependency) for dependency in task["depends_on"]) or len(set(task["depends_on"])) != len(task["depends_on"]):
            return None, [ValidationIssue(0, "authoritative policy dependencies must be unique L3 IDs")]
        if not all(_is_non_empty_string(gate) for gate in task["gate_ids"]) or len(set(task["gate_ids"])) != len(task["gate_ids"]):
            return None, [ValidationIssue(0, "authoritative policy gates must be unique non-empty IDs")]
        approval = task.get("not_applicable")
        if approval is not None and (not isinstance(approval, dict) or set(approval) != {"approved", "approval_id"} or approval.get("approved") is not True or not _is_non_empty_string(approval.get("approval_id"))):
            return None, [ValidationIssue(0, "authoritative not-applicable approval is invalid")]
        if task["task_id"] not in PREAPPROVED_CONDITIONAL_NA_TASKS and (task["mandatory"] is not True or approval is not None):
            return None, [ValidationIssue(0, f"authoritative policy may preapprove not-applicable only for {sorted(PREAPPROVED_CONDITIONAL_NA_TASKS)}")]
        if task["task_id"] in PREAPPROVED_CONDITIONAL_NA_TASKS and task["mandatory"] is not False:
            return None, [ValidationIssue(0, f"preapproved conditional task {task['task_id']} must be nonmandatory in policy")]
        task_ids.add(task["task_id"])
    for task in policy["tasks"]:
        if any(dependency not in task_ids for dependency in task["depends_on"]):
            return None, [ValidationIssue(0, "authoritative policy dependencies must reference registered L3 tasks")]
    return policy, []


def _latest_by_task(records: Iterable[tuple[int, dict[str, Any]]]) -> dict[str, tuple[int, dict[str, Any]]]:
    latest: dict[str, tuple[int, dict[str, Any]]] = {}
    for line, record in records:
        latest[record["task_id"]] = (line, record)
    return latest


def _authorized_terminal_issue(record: Mapping[str, Any], policy_tasks: Mapping[str, Mapping[str, Any]]) -> str | None:
    if record["evidence_scope"] == "trait-mock-only":
        return "trait/mock-only evidence"
    if record["status"] == "passed":
        return None
    if record["status"] != "not-applicable":
        return f"latest status {record['status']}"
    authority = policy_tasks.get(record["task_id"])
    approval = authority.get("not_applicable") if authority else None
    if authority and authority.get("mandatory"):
        return "authoritative mandatory leaf is not-applicable"
    if not isinstance(approval, dict) or not approval.get("approved") or record["adjustment_id"] != approval.get("approval_id"):
        return "not-applicable lacks authoritative approval"
    return None


def _resolved_replacement_chain(
    source_task_id: str,
    latest_by_task: Mapping[str, tuple[int, dict[str, Any]]],
    policy_tasks: Mapping[str, Mapping[str, Any]],
) -> tuple[list[str], str | None]:
    """Return replacement IDs from source through the terminal successor, or an error."""
    chain = [source_task_id]
    seen = {source_task_id}
    task_id = source_task_id
    while True:
        current = latest_by_task.get(task_id)
        if current is None:
            return chain, f"replacement task {task_id} has no current evidence"
        record = current[1]
        if record["status"] != "superseded":
            terminal_issue = _authorized_terminal_issue(record, policy_tasks)
            if terminal_issue:
                return chain, f"replacement chain ends with {terminal_issue}, not passed or authorized not-applicable"
            return chain, None
        replacement = record.get("replacement_task_id")
        if not isinstance(replacement, str) or replacement not in policy_tasks:
            return chain, f"superseded task {task_id} has no authoritative replacement"
        if replacement in seen:
            return chain, f"supersession replacement cycle includes {replacement}"
        seen.add(replacement)
        chain.append(replacement)
        task_id = replacement


def _transitive_dependents(source_task_id: str, policy_tasks: Mapping[str, Mapping[str, Any]]) -> set[str]:
    reverse_dependencies: dict[str, set[str]] = {}
    for task_id, task in policy_tasks.items():
        for dependency in task["depends_on"]:
            reverse_dependencies.setdefault(dependency, set()).add(task_id)
    dependents: set[str] = set()
    pending = list(reverse_dependencies.get(source_task_id, ()))
    while pending:
        task_id = pending.pop()
        if task_id in dependents:
            continue
        dependents.add(task_id)
        pending.extend(reverse_dependencies.get(task_id, ()))
    return dependents


def _supersession_issues(
    records: list[tuple[int, dict[str, Any]]], policy_tasks: Mapping[str, Mapping[str, Any]]
) -> list[ValidationIssue]:
    latest = _latest_by_task(records)
    events_by_task: dict[str, list[tuple[int, dict[str, Any]]]] = {}
    for line, record in records:
        events_by_task.setdefault(record["task_id"], []).append((line, record))
    issues: list[ValidationIssue] = []
    for source_task_id, source_events in events_by_task.items():
        for event_position, (source_line, source_record) in enumerate(source_events):
            if source_record["status"] != "superseded":
                continue
            if event_position != len(source_events) - 1:
                issues.append(ValidationIssue(source_line, f"superseded task {source_task_id} must remain superseded and cannot append a later source event"))
                continue
            replacement_chain, chain_issue = _resolved_replacement_chain(source_task_id, latest, policy_tasks)
            if chain_issue:
                issues.append(ValidationIssue(source_line, chain_issue))
                continue
            source_event_hash = canonical_event_sha256(source_record)
            for dependent_task_id in _transitive_dependents(source_task_id, policy_tasks):
                dependent_events = events_by_task.get(dependent_task_id, [])
                stale_events = [
                    (line, record)
                    for line, record in dependent_events
                    if line > source_line
                    and record["status"] == "stale"
                    and record.get("stale_for_supersession_event_sha256") == source_event_hash
                ]
                if not stale_events:
                    issues.append(ValidationIssue(source_line, f"authoritative dependent {dependent_task_id} lacks post-supersession stale event for {source_task_id}"))
                    continue
                latest_dependent = latest.get(dependent_task_id)
                if latest_dependent is None or latest_dependent[1]["status"] not in {"passed", "not-applicable"}:
                    continue
                latest_line, latest_record = latest_dependent
                stale_hashes = {canonical_event_sha256(record) for _, record in stale_events}
                if latest_record.get("revalidation_of_event_sha256") not in stale_hashes or latest_record.get("revalidated_against_task_id") not in replacement_chain[1:]:
                    issues.append(ValidationIssue(latest_line, f"revalidated dependent {dependent_task_id} must bind its post-supersession stale event and a successor replacement for {source_task_id}"))
    return issues


def _dependency_closure(task_ids: set[str], policy_tasks: Mapping[str, Mapping[str, Any]]) -> set[str]:
    closure = set(task_ids)
    pending = list(task_ids)
    while pending:
        task_id = pending.pop()
        authority = policy_tasks.get(task_id)
        if authority is None:
            continue
        for dependency in authority["depends_on"]:
            if dependency not in closure:
                closure.add(dependency)
                pending.append(dependency)
    return closure


def _closure_issues(
    records: list[tuple[int, dict[str, Any]]],
    policy: Mapping[str, Any],
    known_task_ids: set[str],
    selected_task_ids: set[str] | None,
    closure_kind: str,
) -> list[ValidationIssue]:
    if not records:
        return [ValidationIssue(0, "closure rejects an empty evidence ledger")]
    issues: list[ValidationIssue] = []
    event_hashes = {canonical_event_sha256(record) for _, record in records}
    if policy["required_prefix_event_sha256"] not in event_hashes:
        issues.append(ValidationIssue(0, "ledger does not contain the authoritative required prefix checkpoint"))
    if canonical_event_sha256(records[-1][1]) != policy["expected_ledger_sha256"]:
        issues.append(ValidationIssue(0, "ledger final event does not match the authoritative expected ledger checkpoint"))
    policy_tasks = {task["task_id"]: task for task in policy["tasks"]}
    if set(policy_tasks) != known_task_ids:
        issues.append(ValidationIssue(0, "authoritative policy task registry must exactly match tasks.md L3 IDs"))
    latest_by_task: dict[str, tuple[int, dict[str, Any]]] = {}
    for line, record in records:
        latest_by_task[record["task_id"]] = (line, record)
    requested_scope = selected_task_ids if selected_task_ids is not None else set(policy_tasks)
    scope = _dependency_closure(requested_scope, policy_tasks)
    for task_id in sorted(scope - set(policy_tasks)):
        issues.append(ValidationIssue(0, f"selected closure task {task_id} is absent from authoritative policy"))
    for task_id in sorted(scope - known_task_ids):
        issues.append(ValidationIssue(0, f"selected closure task {task_id} is absent from tasks.md"))
    for task_id in sorted(scope & set(policy_tasks)):
        if task_id not in latest_by_task:
            issues.append(ValidationIssue(0, f"closure has no latest evidence event for authoritative task {task_id}"))
    task6_ids = {task_id for task_id in known_task_ids if task_id.startswith("6.") and task_id != "6.4.7"}
    if "6.4.7" in scope:
        gate_authority = policy_tasks.get("6.4.7")
        if gate_authority is None or not task6_ids.issubset(set(gate_authority["depends_on"])):
            issues.append(ValidationIssue(0, "6.4.7 authority must depend on every preceding Task 6 leaf"))
    for task_id, (line, record) in latest_by_task.items():
        authority = policy_tasks.get(task_id)
        if authority is None:
            issues.append(ValidationIssue(line, f"task_id {task_id} is absent from authoritative policy"))
            continue
        for field in ("priority", "release_blocking", "mandatory"):
            if record[field] != authority[field]:
                issues.append(ValidationIssue(line, f"claimant {field} does not match authoritative policy"))
        if set(record["depends_on"]) != set(authority["depends_on"]):
            issues.append(ValidationIssue(line, "claimant depends_on does not match authoritative policy"))
        if record["gate_id"] not in authority["gate_ids"]:
            issues.append(ValidationIssue(line, "claimant gate_id is absent from authoritative policy"))
    issues.extend(completion_issues(records, policy_tasks, scope))
    issues.extend(_supersession_issues(records, policy_tasks))
    if closure_kind == "release":
        issues.append(ValidationIssue(0, "release closure is unavailable until task 1.1.8 supplies signed retained-bundle trust verification"))
    return issues


def validate_index(
    path: Path,
    *,
    verify_artifacts: bool = False,
    locator_map: Mapping[str, str] | None = None,
    tasks_path: Path | None = None,
    require_complete: bool = False,
    closure_policy_path: Path | None = None,
    lineage_mapping_path: Path | None = None,
    closure_task_ids: set[str] | None = None,
    closure_kind: str = "leaf",
    local_result_root: Path | None = None,
) -> list[ValidationIssue]:
    """Return all record and cross-record validation issues in a JSONL index."""
    issues: list[ValidationIssue] = []
    records: list[tuple[int, dict[str, Any]]] = []
    try:
        lines = path.read_text(encoding="utf-8-sig").splitlines()
    except OSError as error:
        return [ValidationIssue(0, f"cannot read {path}: {error}")]
    for line, source in enumerate(lines, start=1):
        if not source.strip():
            issues.append(ValidationIssue(line, "blank JSONL lines are not permitted"))
            continue
        try:
            record = json.loads(source)
        except json.JSONDecodeError as error:
            issues.append(ValidationIssue(line, f"invalid JSON: {error.msg}"))
            continue
        issues.extend(_validate_record_shape(record, line))
        if not _validate_record_shape(record, line):
            records.append((line, record))
    issues.extend(_validate_cross_record_rules(records))
    if tasks_path is not None:
        try:
            known_task_ids = _task_ids_from_plan(tasks_path)
        except ValueError as error:
            issues.append(ValidationIssue(0, str(error)))
        else:
            for line, record in records:
                if record["task_id"] not in known_task_ids:
                    issues.append(ValidationIssue(line, f"task_id {record['task_id']} is not an L3 in {tasks_path}"))
                replacement = record.get("replacement_task_id")
                if replacement is not None and replacement not in known_task_ids:
                    issues.append(ValidationIssue(line, f"replacement_task_id {replacement} is not an L3 in {tasks_path}"))
            if lineage_mapping_path is not None:
                issues.extend(_lineage_mapping_issues(lineage_mapping_path, known_task_ids))
    elif lineage_mapping_path is not None:
        issues.append(ValidationIssue(0, "lineage mapping validation requires tasks.md"))
    verified_artifacts = False
    if verify_artifacts:
        issues.extend(_verify_artifacts(records, locator_map or {}))
        verified_artifacts = True
    if require_complete and closure_policy_path is None:
        issues.extend(completion_issues(records))
    needs_local_results = require_complete or closure_policy_path is not None
    if needs_local_results and any(record["record_kind"] == "leaf-result" for _, record in records):
        if local_result_root is None:
            issues.append(ValidationIssue(0, "leaf completion requires --local-result-root to recheck result bytes"))
        else:
            issues.extend(_verify_local_results(records, local_result_root))
    if closure_policy_path is not None:
        policy, policy_issues = _load_authoritative_policy(closure_policy_path)
        issues.extend(policy_issues)
        if policy is not None:
            if tasks_path is None:
                issues.append(ValidationIssue(0, "closure requires authoritative tasks.md target validation"))
            else:
                try:
                    known_task_ids = _task_ids_from_plan(tasks_path)
                except ValueError as error:
                    issues.append(ValidationIssue(0, str(error)))
                else:
                    issues.extend(_closure_issues(records, policy, known_task_ids, closure_task_ids, closure_kind))
                    if not verified_artifacts:
                        # Closure never trusts claimant-provided test URL maps.
                        # A production local release-bundle resolver must supply
                        # retained bytes from the configured local trust root.
                        issues.extend(_verify_artifacts(records, {}))
    return issues


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Validate a plugin-platform evidence-index.jsonl file")
    parser.add_argument("index", type=Path, help="path to evidence-index.jsonl")
    parser.add_argument("--verify-artifacts", action="store_true", help="retrieve each configured immutable locator and recheck SHA-256")
    parser.add_argument("--locator-map", type=Path, help="JSON object mapping immutable locators to test URLs; never authoritative for closure")
    parser.add_argument("--tasks", type=Path, help="optional tasks.md whose L3 IDs must contain every task and replacement target")
    parser.add_argument("--require-complete", action="store_true", help="check claimant latest states only; use --closure for authoritative closure")
    parser.add_argument("--closure", action="store_true", help="fail closed using the release-integrator authoritative policy and checkpoints")
    parser.add_argument("--closure-kind", choices=("leaf", "release"), default="leaf", help="leaf closure is bundle-free; release closure fails closed until signed retained-bundle verification exists")
    parser.add_argument("--local-result-root", type=Path, help="root containing <task-id>/result.json for leaf completion hash verification")
    parser.add_argument("--policy", type=Path, help="release-integrator-owned evidence policy required by --closure")
    parser.add_argument("--lineage-map", type=Path, help="optional legacy-lineage-map-v1 file whose new_l3_ids must exist in --tasks")
    parser.add_argument("--task", action="append", dest="closure_tasks", metavar="L3_ID", help="repeatable L3 task selected for authoritative --closure; omit for full release closure")
    arguments = parser.parse_args(argv)
    if arguments.locator_map and not arguments.verify_artifacts:
        parser.error("--locator-map requires --verify-artifacts")
    if arguments.closure and not arguments.policy:
        parser.error("--closure requires --policy")
    if arguments.closure and not arguments.tasks:
        parser.error("--closure requires --tasks")
    if arguments.closure and arguments.locator_map:
        parser.error("--closure never accepts a test locator map; it resolves signed local release evidence from its trust root")
    if arguments.policy and not arguments.closure:
        parser.error("--policy requires --closure")
    if arguments.lineage_map and not arguments.tasks:
        parser.error("--lineage-map requires --tasks")
    if arguments.closure_tasks and not arguments.closure:
        parser.error("--task requires --closure")
    if arguments.closure_tasks and any(not TASK_ID_PATTERN.fullmatch(task_id) for task_id in arguments.closure_tasks):
        parser.error("--task must be a three-level numeric L3 ID")
    locator_map: Mapping[str, str] | None = None
    if arguments.verify_artifacts:
        if arguments.locator_map:
            try:
                loaded_map = json.loads(arguments.locator_map.read_text(encoding="utf-8-sig"))
            except (OSError, json.JSONDecodeError) as error:
                parser.error(f"cannot read locator map: {error}")
            if not isinstance(loaded_map, dict) or not all(isinstance(key, str) and isinstance(value, str) for key, value in loaded_map.items()):
                parser.error("locator map must be a JSON object of locator-to-URL strings")
            locator_map = loaded_map
    issues = validate_index(
        arguments.index,
        verify_artifacts=arguments.verify_artifacts,
        locator_map=locator_map,
        tasks_path=arguments.tasks,
        require_complete=arguments.require_complete,
        closure_policy_path=arguments.policy if arguments.closure else None,
        lineage_mapping_path=arguments.lineage_map,
        closure_task_ids=set(arguments.closure_tasks) if arguments.closure_tasks else None,
        closure_kind=arguments.closure_kind,
        local_result_root=arguments.local_result_root,
    )
    if issues:
        for issue in issues:
            print(issue, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
