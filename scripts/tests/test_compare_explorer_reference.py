from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from PIL import Image

SCRIPT = Path(__file__).parents[1] / "compare_explorer_reference.py"
SPEC = importlib.util.spec_from_file_location("compare_explorer_reference", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def rect(x: float, y: float, width: float, height: float) -> dict[str, float]:
    return {"x": x, "y": y, "width": width, "height": height}


def document(region_rect: dict[str, float], *, duplicate: bool = False) -> dict:
    region = {"id": "explorer-window", "physical_rect": region_rect}
    regions = [region, region.copy()] if duplicate else [region]
    return {"schema_version": 2, "coordinate_space": {"width": 100, "height": 100}, "regions": regions, "pixel_masks": []}


class RegionComparatorTests(unittest.TestCase):
    def test_typography_gate_compares_family_weight_and_logical_metrics(self) -> None:
        style = {
            "profile": "windows-11-zh-tw",
            "family": "Microsoft JhengHei UI",
            "size": 12,
            "weight": 400,
            "line_height": 16,
            "baseline": 13,
        }
        reference = document(rect(0, 0, 100, 100))
        actual = document(rect(0, 0, 100, 100))
        reference["regions"][0]["typography_reference"] = style.copy()
        actual["regions"][0]["typography_reference"] = {**style, "size": 13}
        report = MODULE._compare_typography(reference, actual, 1)
        self.assertTrue(report["passed"])
        actual["regions"][0]["typography_reference"] = {**style, "baseline": 15}
        report = MODULE._compare_typography(reference, actual, 1)
        self.assertFalse(report["passed"])
        self.assertEqual(report["failed_regions"], ["explorer-window"])

    def test_icon_gate_compares_bounds_in_each_window_coordinate_space(self) -> None:
        reference = document(rect(0, 0, 100, 100))
        actual = document(rect(0, 0, 200, 200))
        actual["coordinate_space"] = {"width": 200, "height": 200}
        reference["regions"][0]["icon_bounds"] = rect(10, 10, 20, 20)
        actual["regions"][0]["icon_bounds"] = rect(20, 20, 40, 40)
        self.assertTrue(MODULE._compare_icons(reference, actual, 10)["passed"])
        actual["regions"][0]["icon_bounds"] = rect(60, 20, 40, 40)
        self.assertFalse(MODULE._compare_icons(reference, actual, 10)["passed"])

    def test_passes_within_ten_percent_and_uses_one_pixel_for_small_reference(self) -> None:
        report = MODULE._compare_regions(document(rect(0, 0, 100, 100)), document(rect(0, 0, 109, 100)), 10, 10, 1)
        self.assertTrue(report["passed"])
        small_reference = document(rect(0, 0, 5, 100))
        small_actual = document(rect(0, 0, 6, 100))
        self.assertTrue(MODULE._compare_regions(small_reference, small_actual, 10, 10, 1)["passed"])

    def test_compares_gaps_and_accepts_zero_sized_regions(self) -> None:
        reference = document(rect(0, 0, 100, 100))
        actual = document(rect(0, 0, 100, 100))
        reference["regions"].append({"id": "zero", "physical_rect": rect(20, 20, 0, 0)})
        actual["regions"].append({"id": "zero", "physical_rect": rect(20, 20, 0, 0)})
        reference["gap_checks"] = [{"id": "window-to-zero", "first": "explorer-window", "second": "zero", "axis": "horizontal"}]
        report = MODULE._compare_regions(reference, actual, 10, 10, 1)
        self.assertTrue(report["passed"])
        actual["regions"][1]["physical_rect"]["x"] = 40
        self.assertFalse(MODULE._compare_regions(reference, actual, 10, 10, 1)["passed"])

    def test_reports_every_field_and_fails_over_threshold(self) -> None:
        report = MODULE._compare_regions(document(rect(10, 10, 50, 50)), document(rect(30, 10, 50, 50)), 10, 10, 1)
        self.assertFalse(report["passed"])
        self.assertEqual(report["failed_regions"], ["explorer-window"])
        self.assertIn("center_x", report["regions"][0]["fields"])

    def test_rejects_schema_duplicate_and_layout_masks(self) -> None:
        invalid = document(rect(0, 0, 100, 100))
        invalid["schema_version"] = 1
        with self.assertRaisesRegex(ValueError, "schema"):
            MODULE._compare_regions(invalid, document(rect(0, 0, 100, 100)), 10, 10, 1)
        with self.assertRaisesRegex(ValueError, "unique"):
            MODULE._region_map(document(rect(0, 0, 100, 100), duplicate=True), "duplicate")
        invalid_mask = document(rect(0, 0, 100, 100))
        invalid_mask["pixel_masks"] = [{"kind": "layout", "rect": rect(0, 0, 10, 10)}]
        with self.assertRaisesRegex(ValueError, "non-layout"):
            MODULE._mask_config(invalid_mask)

    def test_cli_handles_success_threshold_size_and_missing_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            explorer, application, output = root / "explorer", root / "application", root / "output"
            explorer.mkdir(); application.mkdir()
            Image.new("RGB", (10, 10), "white").save(explorer / "screenshot.png")
            Image.new("RGB", (10, 10), "white").save(application / "screenshot.png")
            (explorer / "regions.json").write_text(json.dumps(document(rect(0, 0, 100, 100))), encoding="utf-8")
            (application / "diagnostics.json").write_text(json.dumps({"region_diagnostics": document(rect(0, 0, 100, 100))}), encoding="utf-8")
            success = subprocess.run([sys.executable, str(SCRIPT), "--explorer", str(explorer), "--application", str(application), "--output", str(output), "--require-region-pass"], check=False)
            self.assertEqual(success.returncode, 0)
            self.assertTrue((output / "masked-diff.png").is_file())
            failed_document = {"region_diagnostics": document(rect(30, 0, 100, 100))}
            (application / "diagnostics.json").write_text(json.dumps(failed_document), encoding="utf-8")
            failed = subprocess.run([sys.executable, str(SCRIPT), "--explorer", str(explorer), "--application", str(application), "--output", str(output), "--require-region-pass"], check=False)
            self.assertEqual(failed.returncode, 1)
            Image.new("RGB", (9, 10), "white").save(application / "screenshot.png")
            size_failed = subprocess.run([sys.executable, str(SCRIPT), "--explorer", str(explorer), "--application", str(application), "--output", str(output), "--require-same-image-size"], check=False)
            self.assertNotEqual(size_failed.returncode, 0)
            (application / "diagnostics.json").unlink()
            missing = subprocess.run([sys.executable, str(SCRIPT), "--explorer", str(explorer), "--application", str(application), "--output", str(output)], check=False)
            self.assertNotEqual(missing.returncode, 0)


if __name__ == "__main__":
    unittest.main()
