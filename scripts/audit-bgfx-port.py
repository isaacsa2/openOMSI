#!/usr/bin/env python3
"""Inventory direct wgpu dependencies before porting openOMSI to bgfx.

Read-only, standard-library-only tool; no game content is copied.
Run: python scripts/audit-bgfx-port.py --output bgfx-inventory.json
The counts are migration scope indicators, NOT a proof of correctness.
"""
from __future__ import annotations

import argparse
import json
import re
from collections import Counter
from pathlib import Path

SCOPES = ("crates", "launcher", "apple")
EXTENSIONS = {".rs", ".toml", ".wgsl", ".metal", ".hlsl", ".glsl", ".vert", ".frag"}
PATTERNS = {
    "wgpu": re.compile(r"\bwgpu\b"),
    "wgpu_hal": re.compile(r"\bwgpu[_-]hal\b"),
    "naga": re.compile(r"\bnaga\b"),
    "wgsl": re.compile(r"\bwgsl\b", re.IGNORECASE),
    "gpu_surface": re.compile(r"\bSurface(Texture|Configuration)?\b|get_current_texture|\.present\("),
    "command_submission": re.compile(r"\bCommandEncoder\b|\.submit\("),
    "gpu_resources": re.compile(r"\b(BindGroup|RenderPipeline|ComputePipeline|TextureView)\b"),
}
IGNORED_DIRS = {"target", "node_modules", ".git", "build", "dist", ".gradle", ".venv"}

def classify(path: Path) -> str:
    p = path.as_posix()
    if p.startswith("crates/omsi-render/"):
        return "renderer"
    if p.startswith("crates/omsi-app/"):
        return "game-and-platform"
    if p.startswith("crates/omsi-ui/") or p.startswith("crates/omsi-plugin-ui/"):
        return "ui"
    if p.startswith("crates/omsi-xr/"):
        return "vr"
    if p.startswith("crates/omsi-navigator/") or p.startswith("crates/omsi-mapdraw/"):
        return "navigation"
    if p.startswith("launcher/") or p.startswith("apple/"):
        return "launchers"
    return "other"

def scan(root: Path) -> dict:
    files: list[dict] = []
    totals: Counter[str] = Counter()
    by_subsystem: dict[str, Counter[str]] = {}
    for name in SCOPES:
        scope = root / name
        if not scope.exists():
            continue
        for candidate in scope.rglob("*"):
            if not candidate.is_file() or candidate.suffix.lower() not in EXTENSIONS:
                continue
            rel = candidate.relative_to(root)
            if any(part in IGNORED_DIRS for part in rel.parts):
                continue
            try:
                text = candidate.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            found = {key: len(pattern.findall(text)) for key, pattern in PATTERNS.items()}
            found = {key: count for key, count in found.items() if count}
            if not found:
                continue
            subsystem = classify(rel)
            item = {"path": rel.as_posix(), "subsystem": subsystem, "hits": found}
            files.append(item)
            totals.update(found)
            by_subsystem.setdefault(subsystem, Counter()).update(found)
    files.sort(key=lambda item: item["path"])
    return {
        "description": "OpenOMSI wgpu/bgfx migration inventory; read-only lexical matches, not verified call graph",
        "scopes": list(SCOPES),
        "files_with_matches": len(files),
        "total_hits": dict(sorted(totals.items())),
        "by_subsystem": {
            key: dict(sorted(value.items()))
            for key, value in sorted(by_subsystem.items())
        },
        "files": files,
    }

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path, help="Write full JSON result to this file")
    parser.add_argument("--top", type=int, default=20, help="Number of hottest files to print")
    args = parser.parse_args()
    root = args.root.resolve()
    if not (root / "Cargo.toml").is_file():
        parser.error(f"not an openOMSI source root (Cargo.toml missing): {root}")
    report = scan(root)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Direct GPU migration references: {report['files_with_matches']} files")
    print("Counts:", json.dumps(report["total_hits"], sort_keys=True))
    for item in sorted(
        report["files"],
        key=lambda item: (-sum(item["hits"].values()), item["path"]),
    )[:max(0, args.top)]:
        print(f"  {item['path']}: {item['hits']}")
    if args.output:
        print(f"Full inventory: {args.output}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
