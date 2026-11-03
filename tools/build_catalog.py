#!/usr/bin/env python3
"""Build the runtime asset catalog from the per-directory NONO manifests.

The upstream asset pack ships one manifest per directory with slightly
different shapes (walk / actions / expressions).  The desktop pet only ever
reads `assets/catalog.json`, which normalises all of that into one schema and
validates every entry against the actual Lottie file.

Usage:
    python3 tools/build_catalog.py                 # write assets/catalog.json
    python3 tools/build_catalog.py --check         # validate, non-zero on drift
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

FORM_NAMES = {1: "形态 1", 2: "形态 2", 3: "形态 3"}
DIRECTIONS = [
    "down",
    "leftdown",
    "left",
    "leftup",
    "up",
    "rightup",
    "right",
    "rightdown",
]

# Emitted before the catalog so the file is easy to diff / review.
SCHEMA_VERSION = 1

# Direction the pet should face when an action returns to idle.  The original
# client leaves the pet facing the direction it was using the object from.
RETURN_FACING_BY_SLUG = {"summon": "down", "cube": "down"}
RETURN_FACING_BY_FORM_SLUG = {
    (1, "battery"): "left",
    (2, "battery"): "right",
    (2, "ball"): "down",
    (3, "ball"): "down",
}


class CatalogError(RuntimeError):
    pass


def load_json(path: Path):
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def lottie_header(path: Path):
    doc = load_json(path)
    return {
        "w": doc.get("w"),
        "h": doc.get("h"),
        "fr": doc.get("fr"),
        "ip": doc.get("ip", 0),
        "op": doc.get("op"),
        "layers": len(doc.get("layers", [])),
        "assets": len(doc.get("assets", [])),
        "chars": len(doc.get("chars", [])),
    }


def check_header(where: str, path: Path, canvas, fps: int, frames: int | None):
    if not path.is_file():
        raise CatalogError(f"{where}: missing file {path}")
    head = lottie_header(path)
    if [head["w"], head["h"]] != list(canvas):
        raise CatalogError(
            f"{where}: canvas mismatch manifest={canvas} lottie={[head['w'], head['h']]}"
        )
    if fps and head["fr"] != fps:
        raise CatalogError(f"{where}: fps mismatch manifest={fps} lottie={head['fr']}")
    if frames is not None:
        actual = (head["op"] or 0) - (head["ip"] or 0)
        if actual != frames:
            raise CatalogError(
                f"{where}: frame count mismatch manifest={frames} lottie={actual}"
            )
    return head


def audio_of(entry: dict, assets_dir: Path):
    """Normalise an upstream `audio` block into `{file, loops, ...}` or None."""
    block = entry.get("audio") or {}
    status = block.get("status")
    rel = block.get("file")
    if status != "ok" or not rel:
        return None
    path = assets_dir / rel
    if not path.is_file():
        raise CatalogError(f"audio file missing: {path}")
    loops = block.get("loops") or 1
    # The source uses 999999 for "loop until the next animation starts".
    infinite = loops > 1000
    return {
        "file": rel,
        "loops": None if infinite else int(loops),
        "loop_until_stop": infinite,
        "stop": block.get("stop"),
    }


def build(assets_dir: Path):
    forms = []
    for form in sorted(FORM_NAMES):
        walk_dir = assets_dir / f"nono-form{form}-walk"
        actions_dir = assets_dir / f"nono-form{form}-actions"
        expr_dir = assets_dir / f"nono-form{form}-expressions"
        if not walk_dir.is_dir():
            continue
        walk_meta = load_json(walk_dir / "meta.json")
        walk_manifest = load_json(walk_dir / "manifest.json")
        anchor = walk_meta["petAnchor"]
        fps = walk_manifest["frameRate"]

        directions = {}
        for entry in walk_manifest["directions"]:
            rel = f"nono-form{form}-walk/{entry['file']}"
            path = assets_dir / rel
            check_header(rel, path, entry["canvas"], fps, entry["frames"])
            directions[entry["dir"]] = {
                "file": rel,
                "canvas": entry["canvas"],
                "frames": entry["frames"],
                "fps": fps,
                "pet_anchor": anchor,
                "content_bbox": entry.get("contentBBox"),
                "loop": True,
                "audio": audio_of(entry, assets_dir),
            }
        missing = [d for d in DIRECTIONS if d not in directions]
        if missing:
            raise CatalogError(f"form {form}: missing walk directions {missing}")

        actions = []
        if (actions_dir / "manifest.json").is_file():
            manifest = load_json(actions_dir / "manifest.json")
            for entry in manifest["actions"]:
                rel = f"nono-form{form}-actions/{entry['file']}"
                path = assets_dir / rel
                check_header(rel, path, entry["canvas"], fps, entry["frames"])
                playback = entry.get("playback") or {}
                actions.append(
                    {
                        "id": entry["id"],
                        "slug": entry["slug"],
                        "name": entry["name"],
                        "file": rel,
                        "canvas": entry["canvas"],
                        "frames": entry["frames"],
                        "fps": fps,
                        "pet_anchor": entry["petAnchor"],
                        "content_bbox": entry.get("contentBBox"),
                        "loop": bool(playback.get("loop", False)),
                        "auto_end": bool(playback.get("autoEnd", 1)),
                        "return_facing": RETURN_FACING_BY_FORM_SLUG.get(
                            (form, entry["slug"])
                        )
                        or RETURN_FACING_BY_SLUG.get(entry["slug"]),
                        "audio": audio_of(entry, assets_dir),
                    }
                )

        expressions = []
        if (expr_dir / "manifest.json").is_file():
            manifest = load_json(expr_dir / "manifest.json")
            for entry in manifest["expressions"]:
                rel = f"nono-form{form}-expressions/{entry['file']}"
                path = assets_dir / rel
                check_header(rel, path, entry["canvas"], fps, entry["frames"])
                expressions.append(
                    {
                        "id": entry["id"],
                        "slug": entry["slug"],
                        "name": entry["name"],
                        "file": rel,
                        "canvas": entry["canvas"],
                        "frames": entry["frames"],
                        "fps": fps,
                        "pet_anchor": entry["petAnchor"],
                        "content_bbox": entry.get("contentBBox"),
                        "loop": False,
                        "auto_end": True,
                        "audio": audio_of(entry, assets_dir),
                    }
                )

        forms.append(
            {
                "id": form,
                "name": FORM_NAMES[form],
                "fps": fps,
                "walk": {
                    "pet_anchor": anchor,
                    "directions": directions,
                    # Idle is a held pose: the first frame of the down-facing walk.
                    "idle_direction": "down",
                    "idle_frame": 0,
                },
                "actions": sorted(actions, key=lambda a: a["id"]),
                "expressions": expressions,
            }
        )

    return {
        "format_version": SCHEMA_VERSION,
        "name": "超能 NONO",
        "generated_by": "tools/build_catalog.py",
        "note": "运行时唯一素材入口；由各目录 manifest.json + meta.json 归一化生成，不要手改。",
        "default_form": 1,
        "forms": forms,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", default="assets", type=Path)
    parser.add_argument("--out", default=None, type=Path)
    parser.add_argument(
        "--check",
        action="store_true",
        help="only validate; fail if the catalog on disk is out of date",
    )
    args = parser.parse_args(argv)

    assets_dir: Path = args.assets
    out: Path = args.out or (assets_dir / "catalog.json")
    try:
        catalog = build(assets_dir)
    except CatalogError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    text = json.dumps(catalog, ensure_ascii=False, indent=2) + "\n"
    if args.check:
        current = out.read_text(encoding="utf-8") if out.is_file() else None
        if current != text:
            print(f"catalog out of date: {out}", file=sys.stderr)
            return 1
        print(f"catalog up to date: {out}")
        return 0

    out.write_text(text, encoding="utf-8")
    forms = catalog["forms"]
    print(
        f"wrote {out} ({len(forms)} forms, "
        f"{sum(len(f['actions']) for f in forms)} actions, "
        f"{sum(len(f['expressions']) for f in forms)} expressions)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
