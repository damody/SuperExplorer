from __future__ import annotations

import copy
import tempfile
import unittest
from pathlib import Path

from scripts.traceability_matrix import build_matrix, validate_matrix


class TraceabilityMatrixTests(unittest.TestCase):
    def fixture(self) -> tuple[tempfile.TemporaryDirectory[str], Path, Path]:
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        specs = root / "specs" / "capability"
        specs.mkdir(parents=True)
        (specs / "spec.md").write_text("### Requirement: Known requirement\n#### Scenario: Known scenario\n- **WHEN** x\n- **THEN** y\n", encoding="utf-8")
        tasks = root / "tasks.md"
        tasks.write_text("- [deferred] 2.1.1 known leaf\n", encoding="utf-8")
        return temporary, root / "specs", tasks

    def test_real_matrix_has_eleven_capabilities_and_all_tasks(self) -> None:
        root = Path(__file__).parents[2] / "openspec" / "changes" / "build-extensible-plugin-platform"
        matrix = build_matrix(root / "specs", root / "tasks.md")
        self.assertEqual(len({item["capability"] for item in matrix["requirements"]}), 11)
        self.assertGreater(len(matrix["mappings"]), 500)
        self.assertEqual(validate_matrix(matrix, root / "specs", root / "tasks.md"), [])

    def test_missing_unknown_orphan_and_mock_only_fail_independently(self) -> None:
        root = Path(__file__).parents[2] / "openspec" / "changes" / "build-extensible-plugin-platform"
        specs, tasks = root / "specs", root / "tasks.md"
        baseline = build_matrix(specs, tasks)
        missing = copy.deepcopy(baseline)
        missing["requirements"].pop()
        self.assertIn("missing requirement selector", "\n".join(validate_matrix(missing, specs, tasks)))
        unknown = copy.deepcopy(baseline)
        unknown["requirements"][0]["selector"] = "req:unknown:selector"
        self.assertIn("unknown requirement selector", "\n".join(validate_matrix(unknown, specs, tasks)))
        orphan = copy.deepcopy(baseline)
        orphan["mappings"].pop()
        self.assertIn("orphan leaf", "\n".join(validate_matrix(orphan, specs, tasks)))
        mock_only = copy.deepcopy(baseline)
        mock_only["mappings"][0]["evidence_scope"] = "trait-mock-only"
        self.assertIn("mock-only coverage", "\n".join(validate_matrix(mock_only, specs, tasks)))


if __name__ == "__main__":
    unittest.main()
