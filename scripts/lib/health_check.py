"""HTTP checks against the nginx gateway (default http://127.0.0.1:8080)."""
from __future__ import annotations

import os
import subprocess
import sys


def main() -> int:
    base = os.environ.get("GATEWAY_URL", "http://127.0.0.1:8080").rstrip("/")
    checks = [
        (f"{base}/users/health", "users health"),
        (f"{base}/accounts/health", "accounts health"),
        (f"{base}/docs/", "docs static"),
    ]
    failures = 0
    for url, label in checks:
        r = subprocess.run(
            ["curl", "-fsS", "--max-time", "3", url],
            capture_output=True,
            text=True,
        )
        if r.returncode == 0:
            print(f"OK  {label} {url}")
        else:
            print(f"FAIL {label} {url}", file=sys.stderr)
            failures += 1
    # Ledger may not expose GET /health; best-effort root through gateway.
    ledger = f"{base}/ledger/"
    r = subprocess.run(
        ["curl", "-fsS", "--max-time", "3", "-o", "/dev/null", "-w", "%{http_code}", ledger],
        capture_output=True,
        text=True,
    )
    if r.returncode == 0 and r.stdout.strip() in ("200", "302", "301", "404"):
        print(f"OK  ledger gateway {ledger} (HTTP {r.stdout.strip()})")
    else:
        print(f"WARN ledger gateway {ledger} (non-fatal)", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
