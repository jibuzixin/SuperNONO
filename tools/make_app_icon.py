#!/usr/bin/env python3
"""Build the macOS app icon (and .icns) from a rendered NONO walk frame.

The frame comes from the app itself, so the icon always matches the current
artwork:

    nono --render-frame 1 walk down 0 24 /tmp/nono-frame.png
    python3 tools/make_app_icon.py /tmp/nono-frame.png dist-build/icon

Requires Pillow (the project's bundled Python already ships it).
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

CANVAS = 1024
MARGIN = 74  # transparent padding around the squircle
RADIUS = 232


def load_pet(frame_path: Path) -> Image.Image:
    """Crop the frame to the pet and drop the baked-in ground-shadow ellipse."""
    frame = Image.open(frame_path).convert("RGBA")
    bbox = frame.getbbox()
    if bbox is None:
        raise SystemExit(f"frame has no visible pixels: {frame_path}")
    frame = frame.crop(bbox)

    alpha = frame.getchannel("A")
    width, height = frame.size
    occupied = [alpha.crop((0, y, width, y + 1)).getbbox() is not None for y in range(height)]

    # The shadow sits below the pet with a transparent gap in between; keep the
    # topmost run of occupied rows so the icon has no floating dark ellipse.
    runs = []
    start = None
    for y, on in enumerate(occupied):
        if on and start is None:
            start = y
        elif not on and start is not None:
            runs.append((start, y))
            start = None
    if start is not None:
        runs.append((start, height))
    if not runs:
        raise SystemExit("frame alpha channel is empty")
    # Drop the flat shadow blob at the bottom (but never the ears/body above it).
    if len(runs) >= 2:
        gap = runs[-1][0] - runs[-2][1]
        height_last = runs[-1][1] - runs[-1][0]
        if gap > height * 0.02 and height_last < height * 0.12:
            runs.pop()
    pet = frame.crop((0, runs[0][0], width, runs[-1][1]))
    tight = pet.getbbox()
    return pet.crop(tight)


def squircle_mask(size: int, inset: int, radius: int) -> Image.Image:
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((inset, inset, size - inset - 1, size - inset - 1), radius=radius, fill=255)
    return mask


def vertical_gradient(size: int, top: tuple[int, int, int], bottom: tuple[int, int, int]) -> Image.Image:
    grad = Image.new("RGB", (1, size))
    pixels = grad.load()
    for y in range(size):
        t = y / max(1, size - 1)
        pixels[0, y] = tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
    return grad.resize((size, size), Image.Resampling.BILINEAR)


def build_icon(pet: Image.Image) -> Image.Image:
    size = CANVAS
    icon = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    # Rounded tile with a soft cyan gradient and a top-left highlight.
    tile_mask = squircle_mask(size, MARGIN, RADIUS)
    tile = vertical_gradient(size, (245, 253, 255), (122, 216, 238)).convert("RGBA")

    highlight = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    hd = ImageDraw.Draw(highlight)
    hd.ellipse((-size * 0.25, -size * 0.55, size * 1.1, size * 0.55), fill=(255, 255, 255, 110))
    highlight = highlight.filter(ImageFilter.GaussianBlur(90))
    tile = Image.alpha_composite(tile, highlight)
    tile.putalpha(tile_mask)
    icon = Image.alpha_composite(icon, tile)

    # Inner rim so the tile keeps a crisp edge on any wallpaper.
    rim = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    rd = ImageDraw.Draw(rim)
    rd.rounded_rectangle(
        (MARGIN + 2, MARGIN + 2, size - MARGIN - 3, size - MARGIN - 3),
        radius=RADIUS - 2,
        outline=(255, 255, 255, 150),
        width=3,
    )
    rim.putalpha(Image.composite(rim.getchannel("A"), Image.new("L", (size, size), 0), tile_mask))
    icon = Image.alpha_composite(icon, rim)

    # Pet, scaled to breathe inside the tile.
    target_h = 690
    scale = target_h / pet.height
    pet = pet.resize((round(pet.width * scale), round(pet.height * scale)), Image.Resampling.LANCZOS)
    max_w = 660
    if pet.width > max_w:
        scale = max_w / pet.width
        pet = pet.resize((max_w, round(pet.height * scale)), Image.Resampling.LANCZOS)
    px = (size - pet.width) // 2
    py = round(size * 0.84) - pet.height

    # Contact shadow under the pet.
    shadow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    sd = ImageDraw.Draw(shadow)
    sd.ellipse(
        (
            px + pet.width * 0.16,
            py + pet.height - 34,
            px + pet.width * 0.84,
            py + pet.height + 34,
        ),
        fill=(24, 92, 110, 95),
    )
    shadow = shadow.filter(ImageFilter.GaussianBlur(24))
    icon = Image.alpha_composite(icon, shadow)
    icon.alpha_composite(pet, (px, py))
    return icon


def write_icns(icon: Image.Image, out_dir: Path) -> Path:
    iconset = out_dir / "AppIcon.iconset"
    if iconset.exists():
        shutil.rmtree(iconset)
    iconset.mkdir(parents=True)
    for base in (16, 32, 128, 256, 512):
        for suffix, factor in (("", 1), ("@2x", 2)):
            px = base * factor
            icon.resize((px, px), Image.Resampling.LANCZOS).save(iconset / f"icon_{base}x{base}{suffix}.png")
    icns = out_dir / "AppIcon.icns"
    subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(icns)], check=True)
    return icns


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("frame", type=Path, help="rendered NONO frame (transparent PNG)")
    parser.add_argument("out_dir", type=Path)
    args = parser.parse_args()

    args.out_dir.mkdir(parents=True, exist_ok=True)
    pet = load_pet(args.frame.expanduser())
    icon = build_icon(pet)
    png = args.out_dir / "nono-1024.png"
    icon.save(png)
    icns = write_icns(icon, args.out_dir)
    print(f"wrote {png} and {icns}")


if __name__ == "__main__":
    main()
