from __future__ import annotations

import base64
import hashlib
import json
import os
import tempfile
import unittest
from pathlib import Path

from scripts.signed_release_bundle import RSA_SHA256_PREFIX, verify_record


N_B64 = "7eNMouAY2AMGew1/Gqos04WiiukF+qRbdxDEIihYwEfgSekisI89IfvGdZ/fmI43RoYh8uhvi2MMs4fcic2DSF8QCbn+8UFAobaBDx3gZE63i6m/rqX0wR6L4sN/kGoWa5wG7ypeUgtmlnLzxg0+2R/my5jCP5Y+v8rGqnbm83YqsHErYJRJTnB8dY4M/V9lcY5+FWR3G6vQMfTx+FpQMeNsfoF+YxbUWqvvsOWwo581TXUzXRUasoqNXdJu0Saf+c3iMihdSC8QLGs34QuWIRzQs5Kahh0nDxF23OfBE+KSCpIh1XL8nj12YHRkgc/W8rE+9vIyIUUcu3XlnrTVwQ=="
D_B64 = "r5qMpeb7L+n7zLZHz20zrekF9bjSOxU8l4X+4qAZ88abILRCcUcrf3yWIZokuj6xOxQk12URgjlZ1OVDvU3BzZivBB8SvRDIHxfT6U7KqAEbqLNj1g6XwD9GT9y0u+w0JLeGvuHtwm68Ce9NWDPK4wiTnFNlgP7tzzJmuMsQ7UIvJaPiI8UVpwHAcWARnMXVJ1ttQZlnLepyG/RR8qmrpJ+SuEhJmk5rFa83cy3HcZePGbYeN1jWwzJ1ZgY0WaYaidrrO3ABpC+bppmwPtXdA+MQxgGfpFwNpTtU5FsER9qlNCS+ax/ZFJCsxHbadMMajSREXj8dskqbECfLHXx8CQ=="


def canonical(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def sign(payload: dict[str, object]) -> str:
    modulus = int.from_bytes(base64.b64decode(N_B64), "big")
    private = int.from_bytes(base64.b64decode(D_B64), "big")
    width = (modulus.bit_length() + 7) // 8
    digest_info = RSA_SHA256_PREFIX + hashlib.sha256(canonical(payload)).digest()
    encoded = b"\x00\x01" + b"\xff" * (width - len(digest_info) - 3) + b"\x00" + digest_info
    return base64.b64encode(pow(int.from_bytes(encoded, "big"), private, modulus).to_bytes(width, "big")).decode("ascii")


class SignedReleaseBundleTests(unittest.TestCase):
    def fixture(self, root: Path) -> tuple[dict[str, object], Path, Path]:
        repository = root / "repository"
        repository.mkdir()
        (repository / ".release-root").write_bytes(b"")
        artifact = repository / "bundles" / "evidence.bin"
        artifact.parent.mkdir()
        artifact.write_bytes(b"retained evidence")
        artifact_hash = hashlib.sha256(artifact.read_bytes()).hexdigest()
        artifact_locator = f"release://local/bundles/evidence.bin#sha256={artifact_hash}"
        payload: dict[str, object] = {
            "bundle_id": "rc-1/evidence",
            "task_id": "1.1.8",
            "subcheck_key": "signed-retained-bundle-verifier",
            "source_revision": "1" * 40,
            "artifact_locator": artifact_locator,
            "artifact_sha256": artifact_hash,
            "artifact_size": len(artifact.read_bytes()),
            "retained_at": "2026-08-04T00:00:00Z",
            "retain_until": "2036-08-04T00:00:00Z",
        }
        envelope = {"schema_version": 1, "subject": "release-integrator", "algorithm": "RS256", "payload": payload, "signature": sign(payload)}
        manifest = repository / "manifests" / "evidence.json"
        manifest.parent.mkdir()
        manifest.write_bytes(canonical(envelope) + b"\n")
        manifest_hash = hashlib.sha256(manifest.read_bytes()).hexdigest()
        manifest_locator = f"release://local/manifests/evidence.json#sha256={manifest_hash}"
        policy = {
            "schema_version": 1,
            "repository": "repository",
            "minimum_retention_days": 30,
            "subjects": [{"subject": "release-integrator", "algorithm": "RS256", "modulus_base64": N_B64, "exponent": 65537}],
        }
        (root / "trust-policy.json").write_bytes(canonical(policy) + b"\n")
        record: dict[str, object] = {
            "task_id": "1.1.8",
            "subcheck_key": "signed-retained-bundle-verifier",
            "sha256": artifact_hash,
            "immutable_locator": artifact_locator,
            "artifact_manifest_sha256": manifest_hash,
            "artifact_manifest_locator": manifest_locator,
            "environment": {"source_revision": "1" * 40},
        }
        return record, artifact, manifest

    def test_accepts_trusted_canonical_signed_bundle(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            record, _, _ = self.fixture(Path(temporary))
            self.assertEqual(verify_record(record, Path(temporary)), [])

    def test_rejects_tamper_wrong_subject_binding_and_expired_retention(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            record, artifact, _ = self.fixture(root)
            artifact.write_bytes(b"tampered")
            self.assertIn("SHA-256", "\n".join(verify_record(record, root)))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            record, _, manifest = self.fixture(root)
            envelope = json.loads(manifest.read_text(encoding="utf-8"))
            envelope["subject"] = "claimant"
            manifest.write_bytes(canonical(envelope) + b"\n")
            digest = hashlib.sha256(manifest.read_bytes()).hexdigest()
            record["artifact_manifest_sha256"] = digest
            record["artifact_manifest_locator"] = f"release://local/manifests/evidence.json#sha256={digest}"
            messages = "\n".join(verify_record(record, root))
            self.assertIn("not uniquely trusted", messages)

    @unittest.skipIf(os.name == "nt", "creating a symlink may require Windows developer mode")
    def test_rejects_symlink_escape(self) -> None:
        with tempfile.TemporaryDirectory() as temporary, tempfile.TemporaryDirectory() as outside:
            root = Path(temporary)
            record, artifact, _ = self.fixture(root)
            artifact.unlink()
            artifact.symlink_to(Path(outside) / "escape")
            self.assertIn("symlink or reparse", "\n".join(verify_record(record, root)))


if __name__ == "__main__":
    unittest.main()
