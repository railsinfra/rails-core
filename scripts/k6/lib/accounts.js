import http from "k6/http";
import { check } from "k6";
import { moneyHeaders } from "./http.js";

/**
 * Legacy path: account is tied to users-service user_id (required).
 * organization_id should be the business / org UUID when your stack expects it.
 */
export function createAccountForUser(cfg, vu, iter, userId) {
  const url = `${cfg.accountsBase}/api/v1/accounts`;
  const payload = {
    account_type: "checking",
    user_id: userId,
    currency: "USD",
  };
  if (cfg.organizationId) {
    payload.organization_id = cfg.organizationId;
  }
  const res = http.post(url, JSON.stringify(payload), {
    headers: {
      "Content-Type": "application/json",
      "X-Environment": cfg.environment,
      "X-Correlation-Id": `k6-acct-${vu}-${iter}`,
    },
  });
  const ok = check(res, {
    "create account 201": (r) => r.status === 201,
  });
  if (!ok || res.status !== 201) {
    throw new Error(`createAccountForUser failed: status=${res.status} body=${String(res.body).slice(0, 500)}`);
  }
  const body = res.json();
  const id = body.id || body.account?.id;
  if (!id) {
    throw new Error(`createAccountForUser: missing account id in ${String(res.body).slice(0, 500)}`);
  }
  return String(id);
}

export function deposit(cfg, vu, iter, accountId, amountCents) {
  const url = `${cfg.accountsBase}/api/v1/accounts/${accountId}/deposit`;
  const res = http.post(
    url,
    JSON.stringify({
      amount: amountCents,
      description: "k6 deposit",
    }),
    { headers: moneyHeaders(cfg, vu, iter) },
  );
  check(res, { "deposit 200": (r) => r.status === 200 }) ||
    (function () {
      throw new Error(`deposit failed: status=${res.status} body=${String(res.body).slice(0, 500)}`);
    })();
}
