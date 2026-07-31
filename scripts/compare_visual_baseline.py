#!/usr/bin/env python3
"""Compare an immutable visual baseline with one captured actual fixture."""

from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path

from PIL import Image


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8-sig") as stream:
        return json.load(stream)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--actual", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    args = parser.parse_args()

    baseline_image_path = args.baseline / "screenshot.png"
    actual_image_path = args.actual / "screenshot.png"
    baseline_diagnostics_path = args.baseline / "diagnostics.json"
    actual_diagnostics_path = args.actual / "diagnostics.json"
    for path in [
        baseline_image_path,
        actual_image_path,
        baseline_diagnostics_path,
        actual_diagnostics_path,
    ]:
        if not path.is_file():
            raise FileNotFoundError(path)

    config = load_json(args.config)
    baseline_diagnostics = load_json(baseline_diagnostics_path)
    actual_diagnostics = load_json(actual_diagnostics_path)
    baseline = Image.open(baseline_image_path).convert("RGBA")
    actual = Image.open(actual_image_path).convert("RGBA")
    if baseline.size != actual.size:
        size_error = f"image size differs: baseline={baseline.size}, actual={actual.size}"
    else:
        size_error = None

    args.output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(baseline_image_path, args.output / "baseline.png")
    shutil.copy2(actual_image_path, args.output / "actual.png")
    shutil.copy2(baseline_diagnostics_path, args.output / "baseline-diagnostics.json")
    shutil.copy2(actual_diagnostics_path, args.output / "actual-diagnostics.json")
    for name in ["metadata.json"]:
        for source, prefix in [(args.baseline, "baseline"), (args.actual, "actual")]:
            candidate = source / name
            if candidate.is_file():
                shutil.copy2(candidate, args.output / f"{prefix}-{name}")

    changed = 0
    compared = 0
    max_delta_seen = 0
    max_channel_delta = int(config["pixel"]["max_channel_delta"])
    alpha_is_strict = bool(config["pixel"]["alpha_is_strict"])
    diff = Image.new("RGBA", baseline.size, (0, 0, 0, 255))
    baseline_pixels = baseline.load()
    actual_pixels = actual.load()
    diff_pixels = diff.load()
    edge = max(
        (
            int(mask["thickness_pixels"])
            for mask in config.get("masks", [])
            if mask.get("kind") == "edge"
        ),
        default=0,
    )

    if size_error is None:
        width, height = baseline.size
        for y in range(height):
            for x in range(width):
                if x < edge or y < edge or x >= width - edge or y >= height - edge:
                    diff_pixels[x, y] = (32, 32, 32, 255)
                    continue
                base_pixel = baseline_pixels[x, y]
                actual_pixel = actual_pixels[x, y]
                deltas = [abs(a - b) for a, b in zip(base_pixel, actual_pixel)]
                max_delta_seen = max(max_delta_seen, max(deltas))
                compared += 1
                rgb_changed = max(deltas[:3]) > max_channel_delta
                alpha_changed = deltas[3] != 0 if alpha_is_strict else False
                if rgb_changed or alpha_changed:
                    changed += 1
                    strength = max(deltas)
                    diff_pixels[x, y] = (255, max(0, 255 - strength * 3), 0, 255)
                else:
                    gray = sum(actual_pixel[:3]) // 6
                    diff_pixels[x, y] = (gray, gray, gray, 255)
    diff.save(args.output / "diff.png")

    ratio = changed / compared if compared else 1.0
    pixel_pass = size_error is None and ratio <= float(
        config["pixel"]["max_changed_pixel_ratio"]
    )
    baseline_colors = baseline_diagnostics["theme"]["colors"]
    actual_colors = actual_diagnostics["theme"]["colors"]
    colors_pass = (
        baseline_colors == actual_colors
        if config["diagnostics"]["semantic_colors_exact"]
        else True
    )
    layout_tolerance = float(config["diagnostics"]["layout_max_absolute_delta"])
    layout_deltas: dict[str, float | str] = {}
    layout_pass = True
    all_layout_keys = sorted(
        set(baseline_diagnostics["layout"]) | set(actual_diagnostics["layout"])
    )
    for key in all_layout_keys:
        baseline_value = baseline_diagnostics["layout"].get(key)
        actual_value = actual_diagnostics["layout"].get(key)
        if isinstance(baseline_value, (int, float)) and isinstance(
            actual_value, (int, float)
        ):
            delta = abs(float(baseline_value) - float(actual_value))
            layout_deltas[key] = delta
            layout_pass = layout_pass and delta <= layout_tolerance
        elif baseline_value != actual_value:
            layout_deltas[key] = f"{baseline_value!r} != {actual_value!r}"
            layout_pass = False

    passed = pixel_pass and colors_pass and layout_pass
    report = {
        "schema_version": 1,
        "passed": passed,
        "pixel": {
            "passed": pixel_pass,
            "size_error": size_error,
            "compared_pixels": compared,
            "changed_pixels": changed,
            "changed_pixel_ratio": ratio,
            "max_changed_pixel_ratio": config["pixel"][
                "max_changed_pixel_ratio"
            ],
            "max_channel_delta_seen": max_delta_seen,
            "channel_tolerance": max_channel_delta,
            "masked_edge_pixels": edge,
        },
        "diagnostics": {
            "semantic_colors_passed": colors_pass,
            "layout_passed": layout_pass,
            "layout_deltas": layout_deltas,
            "layout_tolerance": layout_tolerance,
        },
        "baseline_directory": str(args.baseline.resolve()),
        "actual_directory": str(args.actual.resolve()),
    }
    with (args.output / "report.json").open("w", encoding="utf-8") as stream:
        json.dump(report, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    print(f"Visual comparison {'passed' if passed else 'failed'}: {args.output}")
    print(f"Changed ratio: {ratio:.6f}; semantic colors: {colors_pass}; layout: {layout_pass}")
    return 0 if passed else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:  # diagnostics command must leave a readable failure
        print(f"visual comparison error: {error}", file=sys.stderr)
        raise
