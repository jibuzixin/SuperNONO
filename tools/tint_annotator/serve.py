#!/usr/bin/env python3
"""Tiny local server for the tint-mask annotator.

GET  /                      -> index.html (this folder)
GET  /manifest.json         -> <data>/manifest.json
GET  /frames/...            -> <data>/frames/...
GET  /masks/<id>.png        -> <masks>/<id>.png
POST /mask/<id>.png         -> write the posted PNG into <masks>/
"""
import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlparse

TOOL_DIR = Path(__file__).resolve().parent


class Handler(BaseHTTPRequestHandler):
    data_dir = TOOL_DIR / "data"
    masks_dir = TOOL_DIR.parent.parent / "assets" / "tint_masks"

    def log_message(self, fmt, *args):
        pass

    def parts_file(self):
        return self.masks_dir.parent / "tint_parts.json"

    @staticmethod
    def anim_from_flat(flat):
        parts = flat.split("_", 2)
        return "/".join(parts)

    def _send(self, code, body, ctype):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = unquote(urlparse(self.path).path)
        if path == "/":
            file = TOOL_DIR / "index.html"
            ctype = "text/html; charset=utf-8"
        elif path == "/manifest.json":
            file = self.data_dir / "manifest.json"
            ctype = "application/json"
        elif path == "/app.js":
            file = TOOL_DIR / "app.js"
            ctype = "application/javascript"
        elif path == "/style.css":
            file = TOOL_DIR / "style.css"
            ctype = "text/css"
        elif path.startswith("/frames/"):
            file = self.data_dir / path.lstrip("/")
            ctype = "image/png"
        elif path.startswith("/masks/") and path.endswith(".png"):
            file = self.masks_dir / Path(path).name
            ctype = "image/png"
        elif path.startswith("/layers/"):
            file = self.data_dir / path.lstrip("/")
            ctype = "image/png" if file.suffix == ".png" else "application/json"
        elif path.startswith("/parts/") and path.endswith(".json"):
            file = self.parts_file()
            if not file.is_file():
                self._send(404, b"{}", "application/json")
                return
            data = json.loads(file.read_text())
            anim = self.anim_from_flat(Path(path).name[:-5])
            entry = data.get("animations", {}).get(anim, [])
            body = json.dumps({"id": anim, "parts": entry}).encode()
            self._send(200, body, "application/json")
            return
        else:
            self._send(404, b"not found", "text/plain")
            return
        if not file.is_file():
            self._send(404, b"not found", "text/plain")
            return
        self._send(200, file.read_bytes(), ctype)

    def do_POST(self):
        path = unquote(urlparse(self.path).path)
        if path.startswith("/parts/") and path.endswith(".json"):
            length = int(self.headers.get("Content-Length", "0"))
            try:
                payload = json.loads(self.rfile.read(length).decode())
            except Exception:
                self._send(400, b"bad json", "text/plain")
                return
            anim = payload.get("id") or self.anim_from_flat(Path(path).name[:-5])
            parts = []
            for item in payload.get("parts", payload.get("layers", [])):
                if isinstance(item, dict):
                    parts.append({"layer": int(item.get("layer", 0)), "paths": [int(v) for v in item.get("paths", [])]})
                else:
                    parts.append(int(item))
            file = self.parts_file()
            data = {"format_version": 1, "animations": {}}
            if file.is_file():
                data = json.loads(file.read_text())
            data.setdefault("animations", {})[anim] = parts
            file.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")
            self._send(200, b"ok", "text/plain")
            return
        if not path.startswith("/mask/"):
            self._send(404, b"not found", "text/plain")
            return
        name = Path(path).name
        if not name.endswith(".png"):
            name += ".png"
        length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(length)
        if not body.startswith(b"\x89PNG"):
            self._send(400, b"not a png", "text/plain")
            return
        self.masks_dir.mkdir(parents=True, exist_ok=True)
        (self.masks_dir / name).write_bytes(body)
        self._send(200, b"ok", "text/plain")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", type=Path, default=TOOL_DIR / "data")
    ap.add_argument("--masks", type=Path, default=TOOL_DIR.parent.parent / "assets" / "tint_masks")
    ap.add_argument("--port", type=int, default=54321)
    args = ap.parse_args()
    Handler.data_dir = args.data
    Handler.masks_dir = args.masks
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"annotator: http://127.0.0.1:{args.port}/")
    print(f"data: {args.data}")
    print(f"masks: {args.masks}")
    server.serve_forever()


if __name__ == "__main__":
    main()
