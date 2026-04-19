"""Print submodule-relative paths from config/services.json (one per line)."""
from __future__ import annotations

import json
import sys
from pathlib import Path


def paths_from_manifest(manifest_path: Path) -> list[str]:
    data = json.loads(manifest_path.read_text(encoding="utf-8"))
    out: list[str] = []
    for item in data.get("services", []):
        out.append(item["path"])
    return out


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: read_manifest.py <services.json>", file=sys.stderr)
        return 2
    manifest = Path(sys.argv[1])
    if not manifest.is_file():
        print(f"missing manifest: {manifest}", file=sys.stderr)
        return 1
    for p in paths_from_manifest(manifest):
        print(p)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
