#!/usr/bin/env python3
"""Compile light/dark/tinted extra app icons into Assets.car."""

from __future__ import annotations
import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RENDITIONS = ROOT / "Assets" / "Renditions"
SIZES = [
    (16, 1),
    (16, 2),
    (32, 1),
    (32, 2),
    (128, 1),
    (128, 2),
    (256, 1),
    (256, 2),
    (512, 1),
    (512, 2),
]
VARIANTS = [
    (None, "Default.png"),
    ("dark", "Dark.png"),
    ("tinted", "TintedDark.png"),
]


def sips_resize(src: Path, dest: Path, pixels: int) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    subprocess.check_call(
        ["sips", "-z", str(pixels), str(pixels), str(src), "--out", str(dest)],
        stdout=subprocess.DEVNULL,
    )


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: compile_app_icon.py <app-resources-dir>", file=sys.stderr)
        return 2
    resources = Path(sys.argv[1])
    resources.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as tmp:
        catalog = Path(tmp) / "Assets.xcassets"
        iconset = catalog / "AppIcon.appiconset"
        iconset.mkdir(parents=True)
        images = []
        for appearance, source_name in VARIANTS:
            source = RENDITIONS / source_name
            for size, scale in SIZES:
                pixels = size * scale
                filename = f"{appearance or 'any'}-{size}@{scale}x.png"
                sips_resize(source, iconset / filename, pixels)
                entry = {
                    "filename": filename,
                    "idiom": "mac",
                    "scale": f"{scale}x",
                    "size": f"{size}x{size}",
                }
                if appearance:
                    entry["appearances"] = [
                        {"appearance": "luminosity", "value": appearance}
                    ]
                images.append(entry)
        (iconset / "Contents.json").write_text(
            json.dumps({"images": images, "info": {"author": "xcode", "version": 1}}, indent=2)
        )
        (catalog / "Contents.json").write_text(
            json.dumps({"info": {"author": "xcode", "version": 1}}, indent=2)
        )
        plist = Path(tmp) / "partial.plist"
        subprocess.check_call(
            [
                "xcrun",
                "actool",
                "--compile",
                str(resources),
                "--platform",
                "macosx",
                "--minimum-deployment-target",
                "14.0",
                "--app-icon",
                "AppIcon",
                "--output-partial-info-plist",
                str(plist),
                str(catalog),
            ]
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
