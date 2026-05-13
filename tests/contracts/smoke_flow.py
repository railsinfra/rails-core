#!/usr/bin/env python3
"""
Cross-service contract: users → accounts (incl. ledger via deposit) → ledger HTTP read.

Requires a running gateway (default http://127.0.0.1:8080) and reachable databases.
"""
from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request
import uuid


def gateway_base() -> str:
    return os.environ.get("GATEWAY_URL", "http://127.0.0.1:8080").rstrip("/")


def request_json(
    method: str,
    url: str,
    *,
    headers: dict[str, str] | None = None,
    json_body: dict | None = None,
    timeout: int = 120,
) -> tuple[int, dict]:
    h = dict(headers or {})
    data = None
    if json_body is not None:
        data = json.dumps(json_body).encode("utf-8")
        h.setdefault("Content-Type", "application/json")
    req = urllib.request.Request(url, data=data, method=method, headers=h)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            text = resp.read().decode()
            body = json.loads(text) if text.strip() else {}
            return resp.getcode() or 200, body
    except urllib.error.HTTPError as e:
        text = e.read().decode()
        try:
            body = json.loads(text) if text.strip() else {}
        except json.JSONDecodeError:
            body = {"_raw": text}
        raise RuntimeError(f"HTTP {e.code} {method} {url}: {body}") from e

def generate_suffix_and_emails() -> tuple[str, str, str]:
    suffix = uuid.uuid4().hex[:12]
    return (
        suffix,
        f"contract-{suffix}@example.com",
        f"holder-{suffix}@example.com",
    )

def register_business(base: str, admin_email: str, suffix: str) -> dict:
    reg_url = f"{base}/users/api/v1/business/register"
    _, reg = request_json(
        "POST",
        reg_url,
        json_body={
            "name": f"Contract Co {suffix}",
            "website": "https://example.com",
            "admin_first_name": "Contract",
            "admin_last_name": "Test",
            "admin_email": admin_email,
            "admin_password": "SecurePass123!",
        },
    )
    return reg

def validate_registration(reg: dict) -> tuple[str, str, str, str] | int:
    access = reg.get("access_token")
    env_id = str(reg.get("selected_environment_id", ""))
    business_id = str(reg.get("business_id", ""))
    admin_user_id = str(reg.get("admin_user_id", ""))
    missing = []
    if not access:
        missing.append("access token")
    if not business_id:
        missing.append("business_id")
    if not admin_user_id:
        missing.append("admin_user_id")
    if missing:
        print(f"FAIL register: missing {', '.join(missing)}", file=sys.stderr)
        return 1
    print(f"OK  users register business_id={business_id} admin_user_id={admin_user_id}")
    return access, env_id, business_id, admin_user_id

def create_api_key(base: str, access: str, env_id: str) -> tuple[str, str] | int:
    key_url = f"{base}/users/api/v1/api-keys"
    _, key_body = request_json(
        "POST",
        key_url,
        headers={
            "Authorization": f"Bearer {access}",
            "X-Environment-Id": env_id,
        },
        json_body={"environment_id": env_id},
    )
    api_key_id = str(key_body.get("api_key_id", ""))
    api_key = key_body.get("api_key", "")
    if not api_key_id or not api_key:
        print("FAIL api-key: missing id or key", file=sys.stderr)
        return 2
    print(f"OK  users api-key id={api_key_id}")
    return api_key_id, api_key

def main() -> int:
    base = gateway_base()
    suffix, admin_email, holder_email = generate_suffix_and_emails()

    print(f"Using gateway {base}")

    reg = register_business(base, admin_email, suffix)
    result = validate_registration(reg)
    if isinstance(result, int):
        return result
    access, env_id, business_id, admin_user_id = result

    api_key_result = create_api_key(base, access, env_id)
    if isinstance(api_key_result, int):
        return api_key_result
    api_key_id, api_key = api_key_result

    # Continue with further steps...
    )
    api_key = key_body.get("key")
    if not api_key:
        print("FAIL api key response missing plaintext key", file=sys.stderr)
        return 1
    print("OK  users api key created")

    # 3) SDK user, then account for that existing user
    user_url = f"{base}/users/api/v1/users"
    _, user_body = request_json(
        "POST",
        user_url,
        headers={
            "X-API-Key": api_key,
            "X-Environment": "sandbox",
        },
        json_body={
            "email": holder_email,
            "first_name": "H",
            "last_name": "older",
            "password": f"Passw0rd-{suffix}!",
        },
    )
    expected_user_id = str(user_body.get("user_id") or "")
    if not expected_user_id:
        print("FAIL sdk user response missing user_id", file=sys.stderr)
        return 1
    print(f"OK  users sdk user created user_id={expected_user_id}")

    acc_url = f"{base}/accounts/api/v1/accounts"
    _, account = request_json(
        "POST",
        acc_url,
        headers={
            "X-API-Key": api_key,
            "X-Environment": "sandbox",
        },
        json_body={
            "account_type": "checking",
            "email": holder_email,
            "first_name": "H",
            "last_name": "older",
        },
    )
    account_id = str(account.get("id") or "")
    org_id = str(account.get("organization_id") or "")
    holder_id = account.get("holder_id")
    user_id = account.get("user_id")
    if not account_id or org_id != business_id:
        print(
            f"FAIL account: id={account_id!r} organization_id={org_id!r} expected business {business_id}",
            file=sys.stderr,
        )
        return 1
    if holder_id is not None or user_id != expected_user_id:
        print(
            f"FAIL account: expected user_id={expected_user_id!r} and holder_id=None, got holder_id={holder_id!r} user_id={user_id!r}",
            file=sys.stderr,
        )
        return 1
    print(f"OK  accounts create account_id={account_id} holder_id={holder_id} user_id={user_id}")

    # 4) Deposit (posts through accounts → ledger gRPC)
    dep_url = f"{base}/accounts/api/v1/accounts/{account_id}/deposit"
    _, dep = request_json(
        "POST",
        dep_url,
        headers={
            "X-Environment": "sandbox",
            "Idempotency-Key": f"contract-deposit-{suffix}",
        },
        json_body={"amount": 10_000},
    )
    if "transaction" not in dep or "account" not in dep:
        print(f"FAIL deposit response shape: {list(dep.keys())}", file=sys.stderr)
        return 1
    print("OK  accounts deposit (ledger mutation)")

    # 5) Ledger HTTP: list entries for this external account
    entries_url = f"{base}/ledger/api/v1/ledger/entries?account_id={account_id}&per_page=20"
    _, entries = request_json(
        "GET",
        entries_url,
        headers={
            "Authorization": f"Bearer {access}",
            "X-Environment": "sandbox",
        },
    )
    rows = entries.get("data")
    if not isinstance(rows, list) or not rows:
        print(f"FAIL ledger entries empty or invalid: {entries!r}", file=sys.stderr)
        return 1
    ext_ids = {str(r.get("external_account_id")) for r in rows}
    if account_id not in ext_ids:
        print(f"FAIL ledger entries missing account_id {account_id} in {ext_ids}", file=sys.stderr)
        return 1
    print(f"OK  ledger entries (n={len(rows)}) reference account {account_id}")

    # Gateway path sanity: all three prefixes served something versioned above
    print("OK  contract flow complete")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
