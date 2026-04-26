#!/usr/bin/env python3
"""Print local gateway URLs as a tabulate table (same style as neon_bootstrap credentials)."""
from __future__ import annotations

from tabulate import tabulate

ROWS: list[tuple[str, str]] = [
    ("Gateway + APIs", "http://localhost:8080/"),
    ("Static docs", "http://localhost:8080/docs/"),
    ("Users API", "http://localhost:8080/users/..."),
    ("Accounts API", "http://localhost:8080/accounts/..."),
    ("Ledger API", "http://localhost:8080/ledger/..."),
    ("Audit API (health)", "http://localhost:8080/audit/..."),
]


def main() -> None:
    print(
        tabulate(
            ROWS,
            headers=["Endpoint", "URL"],
            tablefmt="fancy_grid",
            stralign="left",
        )
    )


if __name__ == "__main__":
    main()
