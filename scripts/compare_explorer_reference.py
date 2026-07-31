#!/usr/bin/env python3
"""Compare Windows Explorer and application pixels plus named visual regions."""

from __future__ import annotations

import argparse
import json
import shutil
from pathlib import Path
from typing import Any

from PIL import Image, ImageChops, ImageDraw, ImageEnhance

REGION_SCHEMA_VERSION = 2
ALLOWED_MASK_KINDS = {"dynamic-content", "typography-edge", "dynamic-state"}
RECT_FIELDS = ("x", "y", "width", "height")


def _load_json(path: Path) -> dict[str, Any]:
    if not path.is_file():
        raise FileNotFoundError(f"required metadata is missing: {path}")
    with path.open("r", encoding="utf-8-sig") as stream:
        value = json.load(stream)
    if not isinstance(value, dict):
        raise ValueError(f"metadata root must be an object: {path}")
    return value


def _validate_rect(rect: Any, label: str) -> dict[str, float]:
    if not isinstance(rect, dict) or any(field not in rect for field in RECT_FIELDS):
        raise ValueError(f"{label} must contain x, y, width, and height")
    result = {field: float(rect[field]) for field in RECT_FIELDS}
    if any(not _is_finite(value) for value in result.values()):
        raise ValueError(f"{label} contains non-finite coordinates")
    if result["width"] < 0 or result["height"] < 0:
        raise ValueError(f"{label} contains a negative size")
    return result


def _is_finite(value: float) -> bool:
    return value == value and value not in (float("inf"), float("-inf"))


def _region_map(document: dict[str, Any], label: str) -> dict[str, dict[str, Any]]:
    if document.get("schema_version") != REGION_SCHEMA_VERSION:
        raise ValueError(f"{label} region schema must be version {REGION_SCHEMA_VERSION}")
    regions = document.get("regions")
    if not isinstance(regions, list):
        raise ValueError(f"{label} regions must be an array")
    result: dict[str, dict[str, Any]] = {}
    for index, region in enumerate(regions):
        if not isinstance(region, dict):
            raise ValueError(f"{label} region {index} must be an object")
        region_id = region.get("id")
        if not isinstance(region_id, str) or not region_id or region_id in result:
            raise ValueError(f"{label} region ids must be non-empty and unique")
        result[region_id] = region
    return result


def _coordinate_space(document: dict[str, Any], regions: dict[str, dict[str, Any]], rect_key: str) -> tuple[float, float]:
    coordinate_space = document.get("coordinate_space")
    if isinstance(coordinate_space, dict):
        width = float(coordinate_space.get("width", 0))
        height = float(coordinate_space.get("height", 0))
        if width > 0 and height > 0:
            return width, height
    window_region = regions.get("explorer-window")
    if window_region:
        rect = _validate_rect(window_region.get(rect_key), f"explorer-window.{rect_key}")
        if rect["width"] > 0 and rect["height"] > 0:
            return rect["width"], rect["height"]
    raise ValueError("region metadata needs a positive coordinate_space or explorer-window")


def _edge_values(rect: dict[str, float]) -> dict[str, float]:
    return {
        "left": rect["x"],
        "top": rect["y"],
        "right": rect["x"] + rect["width"],
        "bottom": rect["y"] + rect["height"],
        "center_x": rect["x"] + rect["width"] / 2,
        "center_y": rect["y"] + rect["height"] / 2,
        "width": rect["width"],
        "height": rect["height"],
    }


def _compare_regions(
    reference: dict[str, Any],
    actual: dict[str, Any],
    tolerance_percent: float,
    small_reference_threshold: float,
    small_absolute_tolerance: float,
) -> dict[str, Any]:
    reference_regions = _region_map(reference, "reference")
    actual_regions = _region_map(actual, "application")
    reference_space = _coordinate_space(reference, reference_regions, "physical_rect")
    actual_space = _coordinate_space(actual, actual_regions, "physical_rect")
    comparisons: list[dict[str, Any]] = []
    missing = sorted(set(reference_regions) - set(actual_regions))
    unexpected = sorted(set(actual_regions) - set(reference_regions))
    tolerance_ratio = tolerance_percent / 100.0

    for region_id in sorted(set(reference_regions) & set(actual_regions)):
        reference_rect = _validate_rect(reference_regions[region_id].get("physical_rect"), f"reference {region_id}")
        actual_rect = _validate_rect(actual_regions[region_id].get("physical_rect"), f"application {region_id}")
        reference_edges = _edge_values(reference_rect)
        actual_edges = _edge_values(actual_rect)
        fields: dict[str, Any] = {}
        region_passed = True
        for field in reference_edges:
            axis = 0 if field in {"left", "right", "center_x", "width"} else 1
            reference_value = reference_edges[field]
            actual_value = actual_edges[field]
            reference_normalized = reference_value / reference_space[axis]
            actual_normalized = actual_value / actual_space[axis]
            normalized_delta = abs(actual_normalized - reference_normalized)
            relative_delta = normalized_delta / max(abs(reference_normalized), 1e-9)
            absolute_delta = abs(actual_value - reference_value)
            if abs(reference_value) < small_reference_threshold:
                passed = absolute_delta <= small_absolute_tolerance
                threshold = {"kind": "absolute", "value": small_absolute_tolerance}
            else:
                passed = relative_delta <= tolerance_ratio
                threshold = {"kind": "relative", "value": tolerance_ratio}
            region_passed = region_passed and passed
            fields[field] = {
                "reference": reference_value,
                "actual": actual_value,
                "absolute_delta": absolute_delta,
                "normalized_delta": normalized_delta,
                "relative_delta": relative_delta,
                "threshold": threshold,
                "passed": passed,
            }
        comparisons.append({"id": region_id, "passed": region_passed, "fields": fields})

    failed = [comparison["id"] for comparison in comparisons if not comparison["passed"]]
    gaps = _compare_gaps(
        reference,
        reference_regions,
        actual_regions,
        reference_space,
        actual_space,
        tolerance_ratio,
        small_reference_threshold,
        small_absolute_tolerance,
    )
    failed_gaps = [gap["id"] for gap in gaps if not gap["passed"]]
    worst_fields = sorted(
        (
            {
                "region": comparison["id"],
                "field": field,
                "relative_delta": values["relative_delta"],
                "absolute_delta": values["absolute_delta"],
                "passed": values["passed"],
            }
            for comparison in comparisons
            for field, values in comparison["fields"].items()
        ),
        key=lambda value: (value["relative_delta"], value["absolute_delta"]),
        reverse=True,
    )[:20]
    return {
        "tolerance_percent": tolerance_percent,
        "small_reference_threshold": small_reference_threshold,
        "small_absolute_tolerance": small_absolute_tolerance,
        "reference_coordinate_space": list(reference_space),
        "actual_coordinate_space": list(actual_space),
        "compared_region_count": len(comparisons),
        "missing_regions": missing,
        "unexpected_regions": unexpected,
        "failed_regions": failed,
        "failed_gaps": failed_gaps,
        "passed": not missing and not failed and not failed_gaps,
        "regions": comparisons,
        "gaps": gaps,
        "worst_fields": worst_fields,
    }


def _compare_gaps(
    reference: dict[str, Any],
    reference_regions: dict[str, dict[str, Any]],
    actual_regions: dict[str, dict[str, Any]],
    reference_space: tuple[float, float],
    actual_space: tuple[float, float],
    tolerance_ratio: float,
    small_reference_threshold: float,
    small_absolute_tolerance: float,
) -> list[dict[str, Any]]:
    checks = reference.get("gap_checks", [])
    if not isinstance(checks, list):
        raise ValueError("gap_checks must be an array")
    results = []
    for index, check in enumerate(checks):
        if not isinstance(check, dict):
            raise ValueError(f"gap check {index} must be an object")
        check_id = str(check.get("id", ""))
        first_id, second_id = check.get("first"), check.get("second")
        axis = check.get("axis")
        if not check_id or axis not in {"horizontal", "vertical"}:
            raise ValueError(f"gap check {index} needs an id and a valid axis")
        if first_id not in reference_regions or second_id not in reference_regions:
            raise ValueError(f"gap check {check_id} references an unknown reference region")
        if first_id not in actual_regions or second_id not in actual_regions:
            results.append({"id": check_id, "passed": False, "reason": "missing-actual-region"})
            continue
        coordinate_axis = 0 if axis == "horizontal" else 1
        origin = "x" if coordinate_axis == 0 else "y"
        extent = "width" if coordinate_axis == 0 else "height"

        def gap(regions: dict[str, dict[str, Any]]) -> float:
            first = _validate_rect(regions[first_id].get("physical_rect"), f"gap {check_id} first")
            second = _validate_rect(regions[second_id].get("physical_rect"), f"gap {check_id} second")
            return second[origin] - (first[origin] + first[extent])

        measured_reference_value = gap(reference_regions)
        reference_value = float(check.get("expected", measured_reference_value))
        actual_value = gap(actual_regions)
        reference_normalized = reference_value / reference_space[coordinate_axis]
        actual_normalized = actual_value / actual_space[coordinate_axis]
        normalized_delta = abs(actual_normalized - reference_normalized)
        relative_delta = normalized_delta / max(abs(reference_normalized), 1e-9)
        absolute_delta = abs(actual_value - reference_value)
        if abs(reference_value) < small_reference_threshold:
            passed = absolute_delta <= small_absolute_tolerance
            threshold = {"kind": "absolute", "value": small_absolute_tolerance}
        else:
            passed = relative_delta <= tolerance_ratio
            threshold = {"kind": "relative", "value": tolerance_ratio}
        results.append({
            "id": check_id,
            "axis": axis,
            "first": first_id,
            "second": second_id,
            "reference": reference_value,
            "measured_reference": measured_reference_value,
            "actual": actual_value,
            "absolute_delta": absolute_delta,
            "normalized_delta": normalized_delta,
            "relative_delta": relative_delta,
            "threshold": threshold,
            "passed": passed,
        })
    return results


def _coverage_report(document: dict[str, Any], field: str) -> dict[str, Any]:
    regions = _region_map(document, "coverage")
    populated = sorted(region_id for region_id, region in regions.items() if region.get(field) is not None)
    return {
        "region_count": len(regions),
        "populated_count": len(populated),
        "populated_regions": populated,
        "coverage_ratio": len(populated) / len(regions) if regions else 0.0,
    }


def _compare_typography(reference: dict[str, Any], actual: dict[str, Any], logical_tolerance: float) -> dict[str, Any]:
    reference_regions = _region_map(reference, "typography reference")
    actual_regions = _region_map(actual, "typography application")
    expected = {
        region_id: region["typography_reference"]
        for region_id, region in reference_regions.items()
        if region.get("typography_reference") is not None
    }
    comparisons = []
    missing = []
    for region_id, reference_style in sorted(expected.items()):
        actual_style = actual_regions.get(region_id, {}).get("typography_reference")
        if not isinstance(reference_style, dict):
            raise ValueError(f"typography reference {region_id} must be an object")
        if not isinstance(actual_style, dict):
            missing.append(region_id)
            continue
        numeric = {}
        for field in ("size", "line_height", "baseline"):
            reference_value = float(reference_style.get(field, 0))
            actual_value = float(actual_style.get(field, 0))
            delta = abs(actual_value - reference_value)
            numeric[field] = {
                "reference": reference_value,
                "actual": actual_value,
                "absolute_delta": delta,
                "tolerance": logical_tolerance,
                "passed": delta <= logical_tolerance,
            }
        family_reference = str(reference_style.get("family", ""))
        family_actual = str(actual_style.get("family", ""))
        weight_reference = int(reference_style.get("weight", 0))
        weight_actual = int(actual_style.get("weight", 0))
        family_passed = family_reference.casefold() == family_actual.casefold()
        weight_passed = weight_reference == weight_actual
        passed = all(field["passed"] for field in numeric.values()) and family_passed and weight_passed
        comparisons.append({
            "id": region_id,
            "passed": passed,
            "family": {"reference": family_reference, "actual": family_actual, "passed": family_passed},
            "weight": {"reference": weight_reference, "actual": weight_actual, "passed": weight_passed},
            "numeric": numeric,
        })
    return {
        "logical_tolerance": logical_tolerance,
        "compared_region_count": len(comparisons),
        "missing_regions": missing,
        "failed_regions": [item["id"] for item in comparisons if not item["passed"]],
        "passed": bool(expected) and not missing and all(item["passed"] for item in comparisons),
        "comparisons": comparisons,
        "reference": _coverage_report(reference, "typography_reference"),
        "application": _coverage_report(actual, "typography_reference"),
    }


def _compare_icons(reference: dict[str, Any], actual: dict[str, Any], tolerance_percent: float) -> dict[str, Any]:
    reference_regions = _region_map(reference, "icon reference")
    actual_regions = _region_map(actual, "icon application")
    reference_space = _coordinate_space(reference, reference_regions, "physical_rect")
    actual_space = _coordinate_space(actual, actual_regions, "physical_rect")
    expected = {
        region_id: region["icon_bounds"]
        for region_id, region in reference_regions.items()
        if region.get("icon_bounds") is not None
    }
    comparisons = []
    missing = []
    tolerance = tolerance_percent / 100.0
    for region_id, reference_bounds in sorted(expected.items()):
        actual_bounds = actual_regions.get(region_id, {}).get("icon_bounds")
        if actual_bounds is None:
            missing.append(region_id)
            continue
        reference_edges = _edge_values(_validate_rect(reference_bounds, f"reference icon {region_id}"))
        actual_edges = _edge_values(_validate_rect(actual_bounds, f"application icon {region_id}"))
        fields = {}
        for field, reference_value in reference_edges.items():
            axis = 0 if field in {"left", "right", "center_x", "width"} else 1
            actual_value = actual_edges[field]
            normalized_delta = abs(actual_value / actual_space[axis] - reference_value / reference_space[axis])
            relative_delta = normalized_delta / max(abs(reference_value / reference_space[axis]), 1e-9)
            fields[field] = {
                "reference": reference_value,
                "actual": actual_value,
                "relative_delta": relative_delta,
                "passed": relative_delta <= tolerance,
            }
        comparisons.append({"id": region_id, "passed": all(item["passed"] for item in fields.values()), "fields": fields})
    return {
        "tolerance_percent": tolerance_percent,
        "compared_region_count": len(comparisons),
        "missing_regions": missing,
        "failed_regions": [item["id"] for item in comparisons if not item["passed"]],
        "passed": bool(expected) and not missing and all(item["passed"] for item in comparisons),
        "comparisons": comparisons,
        "reference": _coverage_report(reference, "icon_bounds"),
        "application": _coverage_report(actual, "icon_bounds"),
    }
def _color_sample_report(reference: dict[str, Any], explorer: Image.Image, application: Image.Image) -> dict[str, Any]:
    samples = reference.get("color_samples", [])
    if not isinstance(samples, list):
        raise ValueError("color_samples must be an array")
    results = []
    for index, sample in enumerate(samples):
        if not isinstance(sample, dict) or not sample.get("id"):
            raise ValueError(f"color sample {index} needs an id")
        point = sample.get("point")
        if not isinstance(point, dict):
            raise ValueError(f"color sample {index} needs a point")
        x, y = int(point.get("x", -1)), int(point.get("y", -1))
        if not (0 <= x < explorer.width and 0 <= y < explorer.height and x < application.width and y < application.height):
            raise ValueError(f"color sample {sample['id']} is outside the common image")
        reference_rgb = explorer.getpixel((x, y))
        actual_rgb = application.getpixel((x, y))
        deltas = [abs(actual_rgb[channel] - reference_rgb[channel]) for channel in range(3)]
        results.append({
            "id": sample["id"],
            "point": {"x": x, "y": y},
            "reference_rgb": list(reference_rgb),
            "actual_rgb": list(actual_rgb),
            "channel_deltas": deltas,
            "maximum_channel_delta": max(deltas),
            "passed": max(deltas) <= int(sample.get("channel_tolerance", 12)),
        })
    return {"sample_count": len(results), "passed": all(item["passed"] for item in results), "samples": results}


def _mask_config(reference: dict[str, Any]) -> list[dict[str, Any]]:
    masks = reference.get("pixel_masks", [])
    if not isinstance(masks, list):
        raise ValueError("pixel_masks must be an array")
    validated = []
    for index, mask in enumerate(masks):
        if not isinstance(mask, dict) or mask.get("kind") not in ALLOWED_MASK_KINDS:
            raise ValueError(f"pixel mask {index} must use an allowed non-layout kind")
        validated.append({
            "kind": mask["kind"],
            "reason": str(mask.get("reason", "")),
            "rect": _validate_rect(mask.get("rect"), f"pixel mask {index}"),
        })
    return validated


def _draw_overlay(image: Image.Image, regions: dict[str, dict[str, Any]], rect_key: str, color: tuple[int, int, int]) -> None:
    draw = ImageDraw.Draw(image)
    for region_id, region in regions.items():
        rect = _validate_rect(region.get(rect_key), f"overlay {region_id}")
        left, top = rect["x"], rect["y"]
        right, bottom = left + rect["width"], top + rect["height"]
        draw.rectangle((left, top, right, bottom), outline=color, width=2)
        draw.ellipse(((left + right) / 2 - 2, (top + bottom) / 2 - 2, (left + right) / 2 + 2, (top + bottom) / 2 + 2), fill=color)


def _pixel_comparison(explorer: Image.Image, application: Image.Image, tolerance: int, masks: list[dict[str, Any]]) -> tuple[Image.Image, Image.Image, dict[str, Any]]:
    common_size = (min(explorer.width, application.width), min(explorer.height, application.height))
    explorer_common = explorer.crop((0, 0, *common_size))
    application_common = application.crop((0, 0, *common_size))
    raw_diff = ImageChops.difference(explorer_common, application_common)
    masked_diff = raw_diff.copy()
    masked_draw = ImageDraw.Draw(masked_diff)
    for mask in masks:
        rect = mask["rect"]
        masked_draw.rectangle((rect["x"], rect["y"], rect["x"] + rect["width"], rect["y"] + rect["height"]), fill=(0, 0, 0))
    changed = 0
    maximum_delta = 0
    total_delta = 0
    changed_bounds = [common_size[0], common_size[1], -1, -1]
    heatmap = Image.new("RGB", common_size, (24, 24, 24))
    heatmap_pixels = heatmap.load()
    for y in range(common_size[1]):
        for x in range(common_size[0]):
            delta = max(masked_diff.getpixel((x, y)))
            maximum_delta = max(maximum_delta, delta)
            total_delta += delta
            if delta > tolerance:
                changed += 1
                changed_bounds = [min(changed_bounds[0], x), min(changed_bounds[1], y), max(changed_bounds[2], x), max(changed_bounds[3], y)]
                heatmap_pixels[x, y] = (255, max(0, 255 - delta * 2), 0)
            else:
                value = sum(application_common.getpixel((x, y))) // 9
                heatmap_pixels[x, y] = (value, value, value)
    compared = common_size[0] * common_size[1]
    return heatmap, masked_diff, {
        "common_top_left_size": list(common_size),
        "channel_tolerance": tolerance,
        "compared_pixels": compared,
        "changed_pixels": changed,
        "changed_pixel_ratio": changed / compared if compared else 1.0,
        "mean_max_channel_delta": total_delta / compared if compared else 255.0,
        "maximum_channel_delta": maximum_delta,
        "changed_bounds": changed_bounds if changed else None,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--explorer", type=Path, required=True)
    parser.add_argument("--application", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--explorer-regions", type=Path)
    parser.add_argument("--application-diagnostics", type=Path)
    parser.add_argument("--channel-tolerance", type=int, default=12)
    parser.add_argument("--region-tolerance-percent", type=float, default=10.0)
    parser.add_argument("--small-reference-threshold", type=float, default=10.0)
    parser.add_argument("--small-absolute-tolerance", type=float, default=1.0)
    parser.add_argument("--require-region-pass", action="store_true")
    parser.add_argument("--require-typography-pass", action="store_true")
    parser.add_argument("--require-icon-pass", action="store_true")
    parser.add_argument("--require-same-image-size", action="store_true")
    args = parser.parse_args()

    explorer_path = args.explorer / "screenshot.png"
    application_path = args.application / "screenshot.png"
    if not explorer_path.is_file() or not application_path.is_file():
        raise FileNotFoundError("both input directories must contain screenshot.png")
    explorer = Image.open(explorer_path).convert("RGB")
    application = Image.open(application_path).convert("RGB")
    if args.require_same_image_size and explorer.size != application.size:
        raise ValueError(f"image size mismatch: Explorer {explorer.size}, application {application.size}")

    explorer_regions_path = args.explorer_regions or args.explorer / "regions.json"
    application_diagnostics_path = args.application_diagnostics or args.application / "diagnostics.json"
    reference_document = _load_json(explorer_regions_path)
    application_document = _load_json(application_diagnostics_path)
    actual_document = application_document.get("region_diagnostics", application_document)
    region_report = _compare_regions(reference_document, actual_document, args.region_tolerance_percent, args.small_reference_threshold, args.small_absolute_tolerance)
    typography_report = _compare_typography(reference_document, actual_document, 1.0)
    icon_report = _compare_icons(reference_document, actual_document, args.region_tolerance_percent)
    masks = _mask_config(reference_document)
    heatmap, masked_diff, pixel_report = _pixel_comparison(explorer, application, args.channel_tolerance, masks)

    args.output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(explorer_path, args.output / "explorer.png")
    shutil.copy2(application_path, args.output / "application.png")
    heatmap.save(args.output / "diff.png")
    ImageEnhance.Contrast(ImageChops.difference(explorer.crop((0, 0, *pixel_report["common_top_left_size"])), application.crop((0, 0, *pixel_report["common_top_left_size"])))).enhance(2.0).save(args.output / "raw-diff.png")
    ImageEnhance.Contrast(masked_diff).enhance(2.0).save(args.output / "masked-diff.png")
    overlay = explorer.copy()
    _draw_overlay(overlay, _region_map(reference_document, "reference"), "physical_rect", (0, 180, 0))
    actual_overlay = application.copy()
    _draw_overlay(actual_overlay, _region_map(actual_document, "application"), "physical_rect", (255, 80, 0))
    overlay.save(args.output / "reference-overlay.png")
    actual_overlay.save(args.output / "application-overlay.png")

    for source, name in [(args.explorer / "metadata.json", "explorer-metadata.json"), (args.application / "metadata.json", "application-metadata.json")]:
        if source.is_file():
            shutil.copy2(source, args.output / name)
    shutil.copy2(explorer_regions_path, args.output / "reference-regions.json")
    shutil.copy2(application_diagnostics_path, args.output / "application-diagnostics.json")

    report = {
        "schema_version": 2,
        "comparison_kind": "windows-explorer-to-application",
        "explorer_size": list(explorer.size),
        "application_size": list(application.size),
        "size_delta": [application.width - explorer.width, application.height - explorer.height],
        "pixel_masks": masks,
        "pixels": pixel_report,
        "regions": region_report,
        "icon_report": icon_report,
        "color_samples": _color_sample_report(reference_document, explorer, application),
        "typography_report": typography_report,
        "reproduce": "scripts/compare_explorer_reference.ps1 with the report input directories",
        "passed": region_report["passed"],
    }
    with (args.output / "report.json").open("w", encoding="utf-8") as stream:
        json.dump(report, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    print(json.dumps(report, ensure_ascii=False))
    failed_required_gate = (args.require_region_pass and not region_report["passed"]) or (
        args.require_typography_pass and not typography_report["passed"]
    ) or (args.require_icon_pass and not icon_report["passed"])
    return 1 if failed_required_gate else 0


if __name__ == "__main__":
    raise SystemExit(main())
