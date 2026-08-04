#!/usr/bin/env python3
"""Verify locally retained, release-integrator signed evidence bundles.

The verifier deliberately has no network or third-party crypto dependency.  A
trust root owns both the accepted RSA subjects and the immutable repository.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import stat
import sys
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Any, Mapping

LOCATOR = re.compile(r"^release://local/([A-Za-z0-9][A-Za-z0-9._/-]*)#sha256=([0-9a-f]{64})$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
SOURCE_REVISION = re.compile(r"^[0-9a-f]{40}$")
MAX_FILE_BYTES = 64 * 1024 * 1024
RSA_SHA256_PREFIX = bytes.fromhex("3031300d060960864801650304020105000420")


def _canonical(value: Mapping[str, Any]) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def _decode_integer(value: Any) -> int:
    if not isinstance(value, str):
        raise ValueError("RSA modulus must be base64")
    return int.from_bytes(base64.b64decode(value, validate=True), "big")


def _verify_rs256(payload: bytes, signature_text: Any, modulus: int, exponent: int) -> bool:
    if not isinstance(signature_text, str) or modulus.bit_length() < 2048 or exponent < 3 or exponent % 2 == 0:
        return False
    try:
        signature = base64.b64decode(signature_text, validate=True)
    except (ValueError, TypeError):
        return False
    width = (modulus.bit_length() + 7) // 8
    if len(signature) != width:
        return False
    encoded = pow(int.from_bytes(signature, "big"), exponent, modulus).to_bytes(width, "big")
    digest_info = RSA_SHA256_PREFIX + hashlib.sha256(payload).digest()
    padding_length = width - len(digest_info) - 3
    expected = b"\x00\x01" + b"\xff" * padding_length + b"\x00" + digest_info
    return padding_length >= 8 and encoded == expected


def _parse_utc(value: Any) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        raise ValueError("timestamp must be UTC and end in Z")
    return datetime.fromisoformat(value[:-1] + "+00:00").astimezone(timezone.utc)


def _contained_regular_file(root: Path, relative: str, maximum_bytes: int) -> tuple[Path | None, str | None]:
    path = PurePosixPath(relative)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts) or "\\" in relative:
        return None, "locator path is not canonical"
    try:
        resolved_root = root.resolve(strict=True)
        cursor = resolved_root
        for part in path.parts:
            cursor = cursor / part
            info = cursor.lstat()
            attributes = getattr(info, "st_file_attributes", 0)
            if stat.S_ISLNK(info.st_mode) or attributes & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400):
                return None, "locator path traverses a symlink or reparse point"
        resolved = cursor.resolve(strict=True)
        resolved.relative_to(resolved_root)
        info = resolved.stat()
        if not stat.S_ISREG(info.st_mode):
            return None, "locator does not identify a regular file"
        if info.st_size > maximum_bytes:
            return None, f"retained file exceeds {maximum_bytes} byte limit"
        return resolved, None
    except (OSError, ValueError) as error:
        return None, f"cannot resolve contained retained file: {error}"


def _resolve_locator(repository: Path, locator: Any, maximum_bytes: int) -> tuple[bytes | None, str | None]:
    match = LOCATOR.fullmatch(locator) if isinstance(locator, str) else None
    if match is None:
        return None, "only canonical local release:// locators are accepted"
    path, issue = _contained_regular_file(repository, match.group(1), maximum_bytes)
    if issue:
        return None, issue
    assert path is not None
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != match.group(2):
        return None, "retained locator SHA-256 does not match bytes"
    return data, None


def verify_record(record: Mapping[str, Any], trust_root: Path) -> list[str]:
    """Verify one retained-bundle ledger record against a local trust root."""
    issues: list[str] = []
    try:
        policy = json.loads((trust_root / "trust-policy.json").read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        return [f"cannot read release trust policy: {error}"]
    if not isinstance(policy, dict) or set(policy) != {"schema_version", "repository", "minimum_retention_days", "subjects"} or policy.get("schema_version") != 1:
        return ["release trust policy has invalid fields"]
    repository_name = policy.get("repository")
    if not isinstance(repository_name, str):
        return ["release trust repository is invalid"]
    repository, repository_issue = _contained_regular_file(trust_root, repository_name + "/.release-root", 1024)
    if repository_issue:
        return [f"release trust repository marker is invalid: {repository_issue}"]
    assert repository is not None
    repository_root = repository.parent
    manifest_bytes, issue = _resolve_locator(repository_root, record.get("artifact_manifest_locator"), MAX_FILE_BYTES)
    if issue:
        return [f"signed manifest: {issue}"]
    assert manifest_bytes is not None
    if hashlib.sha256(manifest_bytes).hexdigest() != record.get("artifact_manifest_sha256"):
        return ["signed manifest hash does not bind evidence record"]
    try:
        envelope = json.loads(manifest_bytes.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        return [f"signed manifest is not canonical UTF-8 JSON: {error}"]
    if not isinstance(envelope, dict) or set(envelope) != {"schema_version", "subject", "algorithm", "payload", "signature"} or envelope.get("schema_version") != 1:
        return ["signed manifest envelope has invalid fields"]
    if envelope.get("algorithm") != "RS256" or not isinstance(envelope.get("payload"), dict):
        return ["signed manifest algorithm or payload is invalid"]
    if manifest_bytes != _canonical(envelope) + b"\n":
        issues.append("signed manifest bytes are not canonical JSON plus newline")
    subjects = policy.get("subjects")
    matching = [item for item in subjects if isinstance(item, dict) and item.get("subject") == envelope.get("subject")] if isinstance(subjects, list) else []
    if len(matching) != 1:
        issues.append("signed manifest subject is not uniquely trusted")
    else:
        subject = matching[0]
        try:
            modulus = _decode_integer(subject.get("modulus_base64"))
            exponent = int(subject.get("exponent"))
        except (ValueError, TypeError):
            issues.append("trusted subject RSA key is invalid")
        else:
            if subject.get("algorithm") != "RS256" or not _verify_rs256(_canonical(envelope["payload"]), envelope.get("signature"), modulus, exponent):
                issues.append("signed manifest signature or trust subject verification failed")
    payload = envelope["payload"]
    required = {"bundle_id", "task_id", "subcheck_key", "source_revision", "artifact_locator", "artifact_sha256", "artifact_size", "retained_at", "retain_until"}
    if set(payload) != required:
        issues.append("signed manifest payload has invalid fields")
        return issues
    bindings = {
        "task_id": record.get("task_id"),
        "subcheck_key": record.get("subcheck_key"),
        "artifact_locator": record.get("immutable_locator"),
        "artifact_sha256": record.get("sha256"),
        "source_revision": record.get("environment", {}).get("source_revision") if isinstance(record.get("environment"), dict) else None,
    }
    for field, expected in bindings.items():
        if payload.get(field) != expected:
            issues.append(f"signed manifest {field} does not bind evidence record")
    if not isinstance(payload.get("bundle_id"), str) or not payload["bundle_id"]:
        issues.append("signed manifest bundle_id is invalid")
    if not SOURCE_REVISION.fullmatch(payload.get("source_revision", "")):
        issues.append("signed manifest source_revision is not a full Git revision")
    artifact_bytes, artifact_issue = _resolve_locator(repository_root, payload.get("artifact_locator"), MAX_FILE_BYTES)
    if artifact_issue:
        issues.append(f"retained artifact: {artifact_issue}")
    elif artifact_bytes is not None and payload.get("artifact_size") != len(artifact_bytes):
        issues.append("signed manifest artifact_size does not bind retained bytes")
    try:
        retained_at = _parse_utc(payload.get("retained_at"))
        retain_until = _parse_utc(payload.get("retain_until"))
        minimum_days = int(policy.get("minimum_retention_days"))
        if minimum_days < 1 or (retain_until - retained_at).total_seconds() < minimum_days * 86400:
            issues.append("signed manifest retention interval is below trust policy")
        if retain_until <= datetime.now(timezone.utc):
            issues.append("signed retained bundle has expired")
    except (ValueError, TypeError):
        issues.append("signed manifest retention metadata is invalid")
    return issues


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Verify a signed local retained evidence bundle")
    parser.add_argument("record", type=Path, help="JSON file containing one retained-bundle ledger record")
    parser.add_argument("--trust-root", type=Path, required=True)
    arguments = parser.parse_args(argv)
    try:
        record = json.loads(arguments.record.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        parser.error(f"cannot read record: {error}")
    issues = verify_record(record, arguments.trust_root)
    for issue in issues:
        print(issue, file=sys.stderr)
    return 1 if issues else 0


if __name__ == "__main__":
    raise SystemExit(main())
