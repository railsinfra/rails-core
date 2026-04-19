"""HTTP checks against the nginx gateway (default http://127.0.0.1:8080)."""
from __future__ import annotations

import json
import os
import subprocess
import sys


def _curl_body(url: str) -> tuple[int, str]:
    r = subprocess.run(
        ["curl", "-fsS", "--max-time", "10", url],
        capture_output=True,
        text=True,
    )
    return r.returncode, r.stdout or ""


def _check_health_json(url: str, label: str, *, quiet: bool) -> bool:
    code, body = _curl_body(url)
    if code != 0:
        if not quiet:
            print(f"FAIL {label} {url} (curl exit {code})", file=sys.stderr)
        return False
    try:
        obj = json.loads(body)
    except json.JSONDecodeError:
        if not quiet:
            print(f"FAIL {label} {url} (invalid JSON)", file=sys.stderr)
        return False
    # Rust services use "healthy"; ledger uses "ok" — both mean up.
    if obj.get("status") not in ("ok", "healthy"):
        if not quiet:
            print(
                f"FAIL {label} {url} (expected status ok|healthy, got {obj.get('status')!r})",
                file=sys.stderr,
            )
        return False
    if not quiet:
        print(f"OK  {label} {url}")
    return True


def main() -> int:
    quiet = "--quiet" in sys.argv
    base = os.environ.get("GATEWAY_URL", "http://127.0.0.1:8080").rstrip("/")
    failures = 0
    for path, label in (
        (f"{base}/health", "gateway health"),
        (f"{base}/users/health", "users health"),
        (f"{base}/accounts/health", "accounts health"),
        (f"{base}/ledger/health", "ledger health"),
    ):
        if not _check_health_json(path, label, quiet=quiet):
            failures += 1

    docs = f"{base}/docs/"
    r = subprocess.run(
        ["curl", "-fsS", "--max-time", "10", docs],
        capture_output=True,
        text=True,
    )
    if r.returncode == 0:
        if not quiet:
            print(f"OK  docs static {docs}")
    else:
        if not quiet:
            print(f"FAIL docs static {docs}", file=sys.stderr)
        failures += 1

    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
