#!/usr/bin/env python3
"""Create deterministic plugin-platform CI evidence archives and receipts."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo


WORKFLOW = ".github/workflows/plugin-platform-evidence.yml"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_archive(output: Path, inputs: list[Path]) -> None:
    members: list[tuple[str, bytes]] = []
    for source in inputs:
        if not source.is_file():
            raise ValueError(f"evidence input is not a regular file: {source}")
        name = source.as_posix().lstrip("/")
        if name.startswith("../") or "/../" in name:
            raise ValueError(f"evidence input escapes archive root: {source}")
        members.append((name, source.read_bytes()))
    if not members:
        raise ValueError("at least one evidence input is required")
    if len({name for name, _ in members}) != len(members):
        raise ValueError("evidence archive has duplicate normalized paths")
    inventory = [{"path": name, "sha256": hashlib.sha256(body).hexdigest(), "bytes": len(body)} for name, body in sorted(members)]
    manifest = json.dumps({"schema_version": 1, "files": inventory}, sort_keys=True, separators=(",", ":")).encode("utf-8")
    output.parent.mkdir(parents=True, exist_ok=True)
    with ZipFile(output, "w", compression=ZIP_DEFLATED, compresslevel=9, strict_timestamps=True) as archive:
        for name, body in sorted(members) + [("evidence-input-manifest.json", manifest)]:
            entry = ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            entry.compress_type = ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            archive.writestr(entry, body, compress_type=ZIP_DEFLATED, compresslevel=9)


def locator(artifact_id: int, run_id: int, sha: str, name: str, artifact_digest: str) -> str:
    from urllib.parse import quote

    return (
        f"ci://github.com/damody/SuperExplorer/artifacts/{artifact_id}?run={run_id}&sha={sha}"
        f"&workflow={WORKFLOW}&name={quote(name, safe='-._~')}#sha256={artifact_digest}"
    )


def write_receipt(output: Path, *, artifact_id: int, run_id: int, sha: str, name: str, artifact_digest: str, subcheck_key: str) -> None:
    if artifact_id < 1 or run_id < 1 or len(sha) != 40 or len(artifact_digest) != 64:
        raise ValueError("receipt identity is invalid")
    evidence_locator = locator(artifact_id, run_id, sha, name, artifact_digest)
    receipt = {
        "schema_version": 1,
        "subcheck_key": subcheck_key,
        "artifact_sha256": artifact_digest,
        "artifact_locator": evidence_locator,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(receipt, sort_keys=True, separators=(",", ":")), encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    archive = subparsers.add_parser("archive")
    archive.add_argument("--output", type=Path, required=True)
    archive.add_argument("inputs", type=Path, nargs="+")
    receipt = subparsers.add_parser("receipt")
    receipt.add_argument("--output", type=Path, required=True)
    receipt.add_argument("--artifact-id", type=int, required=True)
    receipt.add_argument("--run-id", type=int, required=True)
    receipt.add_argument("--sha", required=True)
    receipt.add_argument("--name", required=True)
    receipt.add_argument("--artifact-digest", required=True)
    receipt.add_argument("--subcheck-key", required=True)
    arguments = parser.parse_args()
    if arguments.command == "archive":
        write_archive(arguments.output, arguments.inputs)
    else:
        write_receipt(arguments.output, **vars(arguments))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
