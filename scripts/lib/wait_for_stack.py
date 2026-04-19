"""Poll gateway health until the full stack is ready or timeout (used by `make dev`)."""
from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path


def _run_health_check(py: str, health_script: Path, *, quiet: bool) -> int:
    args = [py, str(health_script)]
    if quiet:
        args.append("--quiet")
    return subprocess.run(args, capture_output=False, text=True).returncode


def _print_tail_logs(repo: Path, env_file: Path) -> None:
    print("\n--- Last 120 lines from all services (docker compose logs) ---\n", file=sys.stderr)
    subprocess.run(
        ["docker", "compose", "--env-file", str(env_file), "logs", "--tail=120"],
        cwd=str(repo),
        check=False,
    )


def main() -> int:
    repo = os.environ.get("RAILS_CORE_ROOT")
    if not repo:
        print("error: RAILS_CORE_ROOT is not set", file=sys.stderr)
        return 1
    repo_path = Path(repo).resolve()
    env_file = repo_path / ".env"
    if not env_file.is_file():
        print(f"error: missing {env_file}", file=sys.stderr)
        return 1

    health_script = repo_path / "scripts" / "lib" / "health_check.py"
    if not health_script.is_file():
        print(f"error: missing {health_script}", file=sys.stderr)
        return 1

    timeout_s = float(os.environ.get("DEV_WAIT_TIMEOUT_SEC", "900"))
    interval_s = float(os.environ.get("DEV_WAIT_INTERVAL_SEC", "5"))
    deadline = time.monotonic() + timeout_s
    py = sys.executable

    print(
        f"Waiting for all health checks (timeout {int(timeout_s)}s, every {int(interval_s)}s)…",
        flush=True,
    )
    time.sleep(min(interval_s, 8.0))

    while time.monotonic() < deadline:
        code = _run_health_check(py, health_script, quiet=True)
        if code == 0:
            print("\nAll health checks passed:\n", flush=True)
            _run_health_check(py, health_script, quiet=False)
            print("\nRails-Core is running\n", flush=True)
            print("Service URLs (via gateway on port 8080):\n", flush=True)
            subprocess.run(
                [py, str(repo_path / "scripts" / "lib" / "print_gateway_hints.py")],
                check=False,
            )
            print(
                "\nDev credentials and DB URLs were printed earlier by bootstrap when applicable.\n"
                "Stream container logs with: make logs\n"
                "Stop the stack with: make stop   (or: make reset)\n",
                flush=True,
            )
            return 0
        remaining = int(deadline - time.monotonic())
        if remaining > 0:
            print(
                f"Health checks not ready yet ({remaining}s left)…",
                flush=True,
            )
        time.sleep(interval_s)

    print(
        "\nerror: timed out waiting for stack health. "
        "Running a verbose health check once; then compose logs.",
        file=sys.stderr,
    )
    _run_health_check(py, health_script, quiet=False)
    print("", file=sys.stderr)
    _print_tail_logs(repo_path, env_file)
    print(
        "\nTip: fix the issue, then `make reset` and `make dev` again, or run `make health` while containers stay up.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
