#!/usr/bin/env python3
"""Print vendored service directory check as a tabulate table (colored when TTY)."""
from __future__ import annotations

import sys
from pathlib import Path

from tabulate import tabulate

from read_manifest import paths_from_manifest

_GREEN = "\033[32m"
_RED = "\033[31m"
_RESET = "\033[0m"


def _status_cell(present: bool) -> str:
    if present:
        text = "\u2713 OK"
        return f"{_GREEN}{text}{_RESET}" if sys.stdout.isatty() else text
    text = "\u2717 missing"
    return f"{_RED}{text}{_RESET}" if sys.stdout.isatty() else text


def main() -> int:
    if len(sys.argv) < 3:
        print(
            "usage: print_vendor_services_check.py <repo-root> <services.json>",
            file=sys.stderr,
        )
        return 2
    repo_root = Path(sys.argv[1]).resolve()
    manifest = Path(sys.argv[2]).resolve()
    if not manifest.is_file():
        print(f"missing manifest: {manifest}", file=sys.stderr)
        return 1

    rows: list[tuple[str, str]] = []
    missing = 0
    for rel in paths_from_manifest(manifest):
        present = (repo_root / rel).is_dir()
        if not present:
            missing += 1
        rows.append((rel, _status_cell(present)))

    print(tabulate(rows, headers=["Service", "Status"], tablefmt="fancy_grid", stralign="left"))
    return 1 if missing else 0


if __name__ == "__main__":
    raise SystemExit(main())
