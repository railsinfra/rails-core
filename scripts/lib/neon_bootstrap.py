"""
Neon API provisioning for rails-core local dev (RAI-9).

Called from scripts/bootstrap.sh after layout verification.
Script deps: scripts/lib/requirements.txt (installed by bootstrap / reset paths).
"""
from __future__ import annotations

import argparse
import json
import os
import random
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

from tabulate import tabulate

NEON_CONSOLE_ORIGIN = "https://console.neon.tech"
NEON_API = f"{NEON_CONSOLE_ORIGIN}/api/v2"

# Placeholders from .env.example — treat as "not configured"
_URL_INCOMPLETE_MARKERS = (
    "YOUR_POSTGRES_HOST",
    "USER:PASSWORD@",
    "@HOST:",  # historical bad placeholder
)

SERVICE_DBS: tuple[tuple[str, str], ...] = (
    ("USERS_DATABASE_URL", "users-db"),
    ("ACCOUNTS_DATABASE_URL", "accounts-db"),
    ("LEDGER_DATABASE_URL", "ledger-db"),
    ("AUDIT_DATABASE_URL", "audit-db"),
)

def _neon_msg(msg: str) -> None:
    print(f"Neon: {msg}", file=sys.stderr, flush=True)


class NeonTransientError(Exception):
    """Retryable Neon / network condition."""


def _read_text(path: Path) -> str | None:
    if not path.is_file():
        return None
    return path.read_text(encoding="utf-8")


def parse_dotenv(text: str) -> dict[str, str]:
    """Minimal KEY=VALUE parser (one line per entry; ignores export prefix)."""
    out: dict[str, str] = {}
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[7:].strip()
        if "=" not in line:
            continue
        k, v = line.split("=", 1)
        key = k.strip()
        val = v.strip().strip('"').strip("'")
        out[key] = val
    return out


def dotenv_lines(text: str) -> list[str]:
    return text.splitlines()


def _env_truthy(env: dict[str, str], key: str) -> bool:
    v = os.environ.get(key, "").strip().lower() or env.get(key, "").strip().lower()
    return v in ("1", "yes", "true", "on")


def is_placeholder_database_url(url: str) -> bool:
    u = url.strip()
    if not u.startswith("postgresql://") and not u.startswith("postgres://"):
        return True
    return any(m in u for m in _URL_INCOMPLETE_MARKERS)


def urls_configured(env: dict[str, str]) -> bool:
    for key, _db in SERVICE_DBS:
        v = env.get(key, "").strip()
        if not v or is_placeholder_database_url(v):
            return False
    return True


def _bitly_fetch_default_group_guid(access_token: str) -> str | None:
    """Resolve a Bitly group_guid via GET /v4/groups (required by many shorten calls)."""
    url = "https://api-ssl.bitly.com/v4/groups"
    if url.lower().startswith(("http://", "https://")):
        req = urllib.request.Request(
            url,
            headers={"Authorization": f"Bearer {access_token}"},
            method="GET",
        )
        try:
            with urllib.request.urlopen(req, timeout=15) as resp:  # noqa: S310 — fixed Bitly API URL
                raw = resp.read().decode("utf-8")
            data = json.loads(raw) if raw else {}
        except (urllib.error.URLError, json.JSONDecodeError, TimeoutError):
            return None
    else:
        raise ValueError(f"Invalid URL scheme for Bitly API: {url}")
    groups = data.get("groups") if isinstance(data, dict) else None
    if not isinstance(groups, list) or not groups:
        return None
    for g in groups:
        if isinstance(g, dict) and g.get("is_active") is True:
            gid = g.get("guid")
            if isinstance(gid, str) and gid.strip():
                return gid.strip()
    first = groups[0]
    if isinstance(first, dict):
        gid = first.get("guid")
        if isinstance(gid, str) and gid.strip():
            return gid.strip()
    return None


def _bitly_shorten(
    long_url: str,
    access_token: str,
    *,
    group_guid: str | None,
) -> str | None:
    """Return a bit.ly short link, or None on failure."""
    payload: dict[str, Any] = {
        "long_url": long_url,
        "domain": "bit.ly",
    }
    if group_guid:
        payload["group_guid"] = group_guid
    body = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        "https://api-ssl.bitly.com/v4/shorten",
        data=body,
        headers={
            "Authorization": f"Bearer {access_token}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:  # noqa: S310 — fixed Bitly API URL
            raw = resp.read().decode("utf-8")
        data = json.loads(raw) if raw else {}
    except (urllib.error.URLError, json.JSONDecodeError, TimeoutError):
        return None
    if isinstance(data, dict):
        link = data.get("link")
        if isinstance(link, str) and link.startswith("http"):
            return link
    return None


def _isgd_shorten(url: str) -> str | None:
    """Shorten via is.gd public API (no API key). See https://is.gd/developers.php — do not use for secrets."""
    if not url.lower().startswith(("http://", "https://")):
        raise ValueError(f"Unsupported URL scheme: {url}")

    q = urllib.parse.urlencode({"format": "simple", "url": url})
    api = f"https://is.gd/create.php?{q}"
    req = urllib.request.Request(
        api,
        method="GET",
        headers={
            "User-Agent": (
                "rails-core-neon-bootstrap/1 "
                "(https://github.com/railsinfra/rails-core; Neon console deep links only)"
            ),
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:  # noqa: S310 — fixed is.gd API URL
            body = resp.read().decode("utf-8").strip()
    except (urllib.error.URLError, UnicodeDecodeError, TimeoutError):
        return None
    if body.startswith("Error:"):
        return None
    if body.startswith("https://"):
        return body
    if body.startswith("http://is.gd/") or body.startswith("http://v.gd/"):
        return "https://" + body[len("http://"):]
    return None


def shorten_https_url(
    url: str,
    bitly_token: str | None,
    *,
    group_guid: str | None = None,
    allow_public_shortener: bool = True,
) -> str:
    """Shorten https URLs for display: Bitly when configured, else is.gd (unless disabled)."""
    if not url or url == "—" or not url.startswith("https://"):
        return url
    token = (bitly_token or "").strip()
    short: str | None = None
    if token:
        short = _bitly_shorten(url, token, group_guid=group_guid)
        if not short and group_guid is not None:
            short = _bitly_shorten(url, token, group_guid=None)
    if not short and allow_public_shortener:
        short = _isgd_shorten(url)
    return short if short else url


def neon_console_project_url(project_id: str) -> str:
    """Neon Console project overview (deep link)."""
    pid = urllib.parse.quote(project_id, safe="")
    return f"{NEON_CONSOLE_ORIGIN}/app/projects/{pid}"


def neon_console_branch_database_url(project_id: str, branch_id: str, database_name: str) -> str:
    """Deep link to branch Tables for `database` (Neon Console path …/branches/…/tables?database=…)."""
    pid = urllib.parse.quote(project_id, safe="")
    bid = urllib.parse.quote(branch_id, safe="")
    q = urllib.parse.urlencode({"database": database_name})
    return f"{NEON_CONSOLE_ORIGIN}/app/projects/{pid}/branches/{bid}/tables?{q}"


def _print_dev_credentials_table(rows: list[tuple[str, str, str]]) -> None:
    """Pretty-print Neon database names and console links as a bordered terminal table."""
    if not rows:
        return
    print(
        tabulate(
            rows,
            headers=["Variable", "Neon database", "Neon console"],
            tablefmt="fancy_grid",
            stralign="left",
        )
    )


def _request_json(
    method: str,
    path: str,
    api_key: str,
    body: dict[str, Any] | None = None,
    query: dict[str, str] | None = None,
) -> tuple[int, Any]:
    url = NEON_API + path
    if query:
        url += "?" + urllib.parse.urlencode(query)
    # Validate URL scheme
    if not url.lower().startswith(("http://", "https://")):
        raise ValueError(f"Invalid URL scheme: {url}")
    data_bytes: bytes | None = None
    headers = {
        "Accept": "application/json",
        "Authorization": f"Bearer {api_key}",
    }
    if body is not None:
        data_bytes = json.dumps(body).encode("utf-8")
        headers["Content-Type"] = "application/json"

    last_err: Exception | None = None
    for attempt in range(8):
        req = urllib.request.Request(url, data=data_bytes, headers=headers, method=method)
        try:
            with urllib.request.urlopen(req, timeout=60) as resp:  # noqa: S310 — controlled Neon URL
                raw = resp.read().decode("utf-8")
                http_status = resp.status
            if not raw:
                return http_status, None
            return http_status, json.loads(raw)
        except urllib.error.HTTPError as e:
            payload = e.read().decode("utf-8", errors="replace")
            try:
                parsed = json.loads(payload) if payload else {}
            except json.JSONDecodeError:
                parsed = {"raw": payload}
            if e.code in (408, 409, 422):
                return e.code, parsed
            if e.code in (423, 429, 500, 502, 503, 504):
                last_err = NeonTransientError(f"HTTP {e.code}: {parsed}")
            else:
                raise RuntimeError(f"Neon API error {e.code}: {parsed}") from e
        except NeonTransientError as e:
            last_err = e
        except urllib.error.URLError as e:
            last_err = NeonTransientError(str(e))
        sleep_s = min(32.0, 2.0**attempt) + random.random()
        time.sleep(sleep_s)
    if last_err:
        raise last_err
    raise RuntimeError("Neon request failed after retries")


def find_project_id_by_name(api_key: str, name: str) -> str | None:
    """Exact name match via paginated GET /projects (no server-side `search` query)."""
    cursor: str | None = None
    seen_cursors: set[str] = set()
    while True:
        q: dict[str, str] = {"limit": "100"}
        if cursor:
            q["cursor"] = cursor
        status, data = _request_json("GET", "/projects", api_key, query=q)
        if status != 200 or not isinstance(data, dict):
            raise RuntimeError(f"unexpected list projects response: {status} {data}")
        batch = data.get("projects") or []
        for p in batch:
            if p.get("name") == name:
                pid = p.get("id")
                if isinstance(pid, str):
                    return pid
        raw_cursor = (data.get("pagination") or {}).get("cursor")
        if not raw_cursor or not isinstance(raw_cursor, str):
            break
        # Neon sometimes returns the same cursor with an empty batch; without a guard this loops forever.
        if raw_cursor == cursor:
            _neon_msg(
                "list-projects pagination returned the same cursor again; "
                "stopping search (project not found in this pass)."
            )
            break
        if raw_cursor in seen_cursors:
            _neon_msg("list-projects pagination repeated a cursor; stopping project search.")
            break
        seen_cursors.add(raw_cursor)
        cursor = raw_cursor
    return None


def create_project(api_key: str, name: str, region_id: str) -> dict[str, Any]:
    body = {"project": {"name": name, "region_id": region_id}}
    status, data = _request_json("POST", "/projects", api_key, body=body)
    if status not in (200, 201) or not isinstance(data, dict):
        raise RuntimeError(f"create project failed: {status} {data}")
    return data


def list_branches(api_key: str, project_id: str) -> list[dict[str, Any]]:
    status, data = _request_json("GET", f"/projects/{project_id}/branches", api_key)
    if status != 200 or not isinstance(data, dict):
        raise RuntimeError(f"list branches failed: {status} {data}")
    return list(data.get("branches") or [])


def default_branch_id(branches: list[dict[str, Any]]) -> str:
    for b in branches:
        if b.get("default") is True:
            bid = b.get("id")
            if isinstance(bid, str):
                return bid
    if branches:
        bid = branches[0].get("id")
        if isinstance(bid, str):
            return bid
    raise RuntimeError("no branch id found for project")


def list_databases(api_key: str, project_id: str, branch_id: str) -> list[dict[str, Any]]:
    status, data = _request_json(
        "GET",
        f"/projects/{project_id}/branches/{branch_id}/databases",
        api_key,
    )
    if status != 200 or not isinstance(data, dict):
        raise RuntimeError(f"list databases failed: {status} {data}")
    return list(data.get("databases") or [])


def list_branch_roles(api_key: str, project_id: str, branch_id: str) -> list[dict[str, Any]]:
    status, data = _request_json(
        "GET",
        f"/projects/{project_id}/branches/{branch_id}/roles",
        api_key,
    )
    if status != 200 or not isinstance(data, dict):
        raise RuntimeError(f"list roles failed: {status} {data}")
    return list(data.get("roles") or [])


def pick_owner_role_from_branch(roles: list[dict[str, Any]]) -> str | None:
    for r in roles:
        if r.get("protected") is True:
            continue
        name = r.get("name")
        if isinstance(name, str) and name.strip():
            return name.strip()
    return None


def create_database(
    api_key: str,
    project_id: str,
    branch_id: str,
    db_name: str,
    owner_name: str,
) -> None:
    owner = (owner_name or "").strip()
    if not owner:
        raise RuntimeError(
            "Neon create_database: empty owner_name (branch has no usable owner_name on databases "
            "and no fallback role)."
        )
    body = {"database": {"name": db_name, "owner_name": owner}}
    status, data = _request_json(
        "POST",
        f"/projects/{project_id}/branches/{branch_id}/databases",
        api_key,
        body=body,
    )
    if status in (200, 201, 409):
        return
    raise RuntimeError(f"create database {db_name!r} failed: {status} {data}")


def get_connection_uri(
    api_key: str,
    project_id: str,
    branch_id: str,
    database_name: str,
    role_name: str,
) -> str:
    q = {
        "database_name": database_name,
        "role_name": role_name,
        "branch_id": branch_id,
    }
    status, data = _request_json(
        "GET",
        f"/projects/{project_id}/connection_uri",
        api_key,
        query=q,
    )
    if status != 200 or not isinstance(data, dict):
        raise RuntimeError(f"connection_uri failed: {status} {data}")
    uri = data.get("uri") or data.get("connection_uri")
    if isinstance(uri, str) and uri.startswith("postgres"):
        return uri
    raise RuntimeError(f"unexpected connection_uri payload: {data}")


def resolve_role_for_db(databases: list[dict[str, Any]], db_name: str, fallback: str) -> str:
    for d in databases:
        if d.get("name") == db_name:
            owner = d.get("owner_name")
            if isinstance(owner, str) and owner:
                return owner
    return fallback


def wait_for_connection_uri(
    api_key: str,
    project_id: str,
    branch_id: str,
    database_name: str,
    role_name: str,
    timeout_s: float = 180.0,
) -> str:
    deadline = time.time() + timeout_s
    last: str | None = None
    while time.time() < deadline:
        try:
            return get_connection_uri(api_key, project_id, branch_id, database_name, role_name)
        except RuntimeError as e:
            last = str(e)
            # New databases / computes can briefly return 404 until Neon finishes provisioning.
            if "404" not in str(e) and "423" not in str(e):
                raise
        except NeonTransientError as e:
            last = str(e)
        time.sleep(3.0)
    raise RuntimeError(f"timed out waiting for Neon connection string ({database_name}): {last}")


def _line_key(line: str) -> str | None:
    s = line.strip()
    if not s or s.startswith("#") or "=" not in s:
        return None
    k = s.split("=", 1)[0].strip()
    if k.startswith("export "):
        k = k[7:].strip()
    return k or None


def merge_write_env(
    example_path: Path,
    env_path: Path,
    updates: dict[str, str],
) -> None:
    """Apply updates; preserve comments and unrelated variables."""
    existing = _read_text(env_path)
    template = _read_text(example_path) or ""
    lines = dotenv_lines(existing) if existing is not None else dotenv_lines(template)

    new_lines: list[str] = []
    seen_update_keys: set[str] = set()
    for line in lines:
        k = _line_key(line)
        if k is not None and k in updates:
            new_lines.append(f"{k}={updates[k]}")
            seen_update_keys.add(k)
        else:
            new_lines.append(line)

    for k, v in updates.items():
        if k not in seen_update_keys:
            new_lines.append(f"{k}={v}")

    env_path.write_text("\n".join(new_lines).rstrip() + "\n", encoding="utf-8")


def provision_neon(
    api_key: str,
    project_name: str,
    region_id: str,
) -> dict[str, str]:
    _neon_msg(f"contacting API (project name {project_name!r})…")
    project_id = find_project_id_by_name(api_key, project_name)
    role_fallback = "neondb_owner"
    if project_id is not None:
        _neon_msg("reusing existing Neon project…")
    if project_id is None:
        _neon_msg("creating Neon project (can take up to a minute)…")
        created = create_project(api_key, project_name, region_id)
        project = created.get("project") or {}
        project_id = project.get("id")
        if not isinstance(project_id, str):
            raise RuntimeError(f"create project response missing id: {created}")
        roles = created.get("roles") or []
        if roles and isinstance(roles[0], dict):
            rn = roles[0].get("name")
            if isinstance(rn, str) and rn:
                role_fallback = rn
        dbs_created = created.get("databases") or []
        branch_id: str | None = None
        if dbs_created and isinstance(dbs_created[0], dict):
            bid = dbs_created[0].get("branch_id")
            if isinstance(bid, str):
                branch_id = bid
        if branch_id is None:
            time.sleep(2.0)
            branch_id = default_branch_id(list_branches(api_key, project_id))
        time.sleep(3.0)
    else:
        branches = list_branches(api_key, project_id)
        branch_id = default_branch_id(branches)

    dbs = list_databases(api_key, project_id, branch_id)
    has_db_owner = False
    for d in dbs:
        on = d.get("owner_name")
        if isinstance(on, str) and on.strip():
            role_fallback = on.strip()
            has_db_owner = True
            break
    if not has_db_owner:
        picked = pick_owner_role_from_branch(list_branch_roles(api_key, project_id, branch_id))
        if picked:
            role_fallback = picked
    existing_names = {d.get("name") for d in dbs if isinstance(d.get("name"), str)}

    for _env_key, db_name in SERVICE_DBS:
        if db_name not in existing_names:
            _neon_msg(f"creating database {db_name!r}…")
            create_database(api_key, project_id, branch_id, db_name, role_fallback)
    dbs = list_databases(api_key, project_id, branch_id)

    urls: dict[str, str] = {}
    _neon_msg("fetching connection strings…")
    for env_key, db_name in SERVICE_DBS:
        role = resolve_role_for_db(dbs, db_name, role_fallback)
        uri = wait_for_connection_uri(api_key, project_id, branch_id, db_name, role)
        urls[env_key] = uri

    meta = {
        "RAILS_CORE_NEON_PROJECT_ID": project_id,
        "RAILS_CORE_NEON_BRANCH_ID": branch_id,
        "RAILS_CORE_NEON_PROJECT_NAME": project_name,
    }
    urls.update(meta)
    return urls


def load_env_file_keys(repo_root: Path) -> dict[str, str]:
    env_path = repo_root / ".env"
    text = _read_text(env_path)
    if text is None:
        return {}
    return parse_dotenv(text)


def resolve_api_key(repo_root: Path) -> str | None:
    k = os.environ.get("NEON_API_KEY", "").strip()
    if k:
        return k
    return load_env_file_keys(repo_root).get("NEON_API_KEY", "").strip() or None


def clear_database_urls(example_path: Path, env_path: Path) -> None:
    if not env_path.is_file():
        raise FileNotFoundError(f"missing {env_path}")
    example_vals = parse_dotenv(_read_text(example_path) or "")
    updates = {key: example_vals[key] for key, _ in SERVICE_DBS if key in example_vals}
    if len(updates) != len(SERVICE_DBS):
        raise RuntimeError(".env.example is missing one or more DATABASE_URL template keys")
    merge_write_env(example_path, env_path, updates)


def strip_neon_metadata_lines(env_path: Path) -> None:
    drop = {
        "RAILS_CORE_NEON_PROJECT_ID",
        "RAILS_CORE_NEON_BRANCH_ID",
        "RAILS_CORE_NEON_PROJECT_NAME",
    }
    text = _read_text(env_path)
    if text is None:
        return
    kept: list[str] = []
    for line in dotenv_lines(text):
        k = _line_key(line)
        if k is not None and k in drop:
            continue
        kept.append(line)
    env_path.write_text("\n".join(kept).rstrip() + "\n", encoding="utf-8")


def delete_neon_project(api_key: str, project_id: str) -> None:
    status, data = _request_json("DELETE", f"/projects/{project_id}", api_key)
    if status not in (200, 202, 204):
        raise RuntimeError(f"delete project failed: {status} {data}")


def action_clear_env(repo_root: Path) -> int:
    try:
        clear_database_urls(repo_root / ".env.example", repo_root / ".env")
    except Exception as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    print(
        "Cleared USERS_/ACCOUNTS_/LEDGER_/AUDIT_DATABASE_URL in .env (other keys unchanged)."
    )
    return 0


def action_purge_neon(repo_root: Path) -> int:
    if os.environ.get("CONFIRM_PURGE_NEON", "").strip().lower() != "yes":
        print(
            "error: refusing to delete Neon project without CONFIRM_PURGE_NEON=yes in the environment.",
            file=sys.stderr,
        )
        return 1
    api_key = resolve_api_key(repo_root)
    if not api_key:
        print("error: NEON_API_KEY not set (environment or .env).", file=sys.stderr)
        return 1
    env_path = repo_root / ".env"
    file_env = load_env_file_keys(repo_root)
    project_id = file_env.get("RAILS_CORE_NEON_PROJECT_ID", "").strip()
    if not project_id:
        print("error: RAILS_CORE_NEON_PROJECT_ID missing from .env (nothing to delete).", file=sys.stderr)
        return 1
    try:
        delete_neon_project(api_key, project_id)
        clear_database_urls(repo_root / ".env.example", env_path)
        strip_neon_metadata_lines(env_path)
    except Exception as e:
        print(f"error: Neon purge failed: {e}", file=sys.stderr)
        return 1
    print("Neon project deleted and local .env database URLs reset to .env.example placeholders.")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="Neon bootstrap / .env helper for rails-core")
    ap.add_argument("--repo-root", type=Path, required=True)
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--clear-env", action="store_true", help="Reset DATABASE_URL entries to .env.example placeholders")
    mode.add_argument("--purge-neon", action="store_true", help="Delete Neon project from RAILS_CORE_NEON_PROJECT_ID")
    args = ap.parse_args()
    repo_root: Path = args.repo_root.resolve()
    env_path = repo_root / ".env"
    example_path = repo_root / ".env.example"

    if args.clear_env:
        return action_clear_env(repo_root)
    if args.purge_neon:
        return action_purge_neon(repo_root)

    file_env = load_env_file_keys(repo_root)
    api_key = resolve_api_key(repo_root)
    project_name = (
        os.environ.get("RAILS_CORE_NEON_PROJECT_NAME", "").strip()
        or file_env.get("RAILS_CORE_NEON_PROJECT_NAME", "").strip()
        or "rails-core-dev"
    )
    region_id = (
        os.environ.get("NEON_REGION_ID", "").strip()
        or file_env.get("NEON_REGION_ID", "").strip()
        or "aws-us-east-1"
    )

    if api_key:
        try:
            updates = provision_neon(api_key, project_name, region_id)
            merge_write_env(example_path, env_path, updates)
        except Exception as e:
            print(f"error: Neon provisioning failed: {e}", file=sys.stderr)
            return 1
        print("")
        print("These are your dev credentials. They are safe but auto-generated for this environment.")
        final = load_env_file_keys(repo_root)
        project_id = final.get("RAILS_CORE_NEON_PROJECT_ID", "").strip()
        branch_id = final.get("RAILS_CORE_NEON_BRANCH_ID", "").strip()
        bitly = (
            os.environ.get("BITLY_ACCESS_TOKEN", "").strip()
            or final.get("BITLY_ACCESS_TOKEN", "").strip()
        ) or None
        bitly_group = (
            os.environ.get("BITLY_GROUP_GUID", "").strip()
            or final.get("BITLY_GROUP_GUID", "").strip()
        ) or None
        if bitly and not bitly_group:
            bitly_group = _bitly_fetch_default_group_guid(bitly)
        allow_public = not _env_truthy(final, "NEON_CONSOLE_NO_PUBLIC_SHORTENER")
        cred_rows: list[tuple[str, str, str]] = []
        for env_key, db_name in SERVICE_DBS:
            u = final.get(env_key, "").strip()
            if not u:
                continue
            if project_id and branch_id:
                console_url = neon_console_branch_database_url(project_id, branch_id, db_name)
                console_url = shorten_https_url(
                    console_url,
                    bitly,
                    group_guid=bitly_group,
                    allow_public_shortener=allow_public,
                )
            else:
                console_url = "—"
            cred_rows.append((env_key, db_name, console_url))
        _print_dev_credentials_table(cred_rows)
        print("")
        print("Neon project:", project_name)
        if project_id:
            proj_console = neon_console_project_url(project_id)
            print(
                "Neon project (console):",
                shorten_https_url(
                    proj_console,
                    bitly,
                    group_guid=bitly_group,
                    allow_public_shortener=allow_public,
                ),
            )
        print("Compose can start; database URLs were written to .env (merged with your other variables).")
        return 0

    if urls_configured(file_env):
        print("Using existing database URLs from .env (set NEON_API_KEY to auto-provision Neon).")
        return 0

    print(
        "error: no NEON_API_KEY and .env is missing usable database URLs.\n"
        "  Option A: export NEON_API_KEY=... then re-run make bootstrap\n"
        "  Option B: cp .env.example .env and set USERS_DATABASE_URL, ACCOUNTS_DATABASE_URL, "
        "LEDGER_DATABASE_URL, AUDIT_DATABASE_URL",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
