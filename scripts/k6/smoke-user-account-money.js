/**
 * Smoke: one SDK user -> one checking account -> small deposit.
 * Maps accounts to users via user_id on POST /api/v1/accounts (legacy path).
 *
 * Run (from repo root, compose up, credentials exported):
 *   k6 run scripts/k6/smoke-user-account-money.js
 */
import { getConfig } from "./config.js";
import { createSdkUser } from "./lib/users.js";
import { createAccountForUser, deposit } from "./lib/accounts.js";

export const options = {
  vus: 1,
  iterations: 1,
};

const cfg = getConfig();

export default function () {
  const vu = __VU;
  const iter = __ITER;
  const email = `k6+vu${vu}+i${iter}+${Date.now()}@example.com`;
  const password = __ENV.K6_USER_PASSWORD || "password123!";

  const userId = createSdkUser(cfg, vu, iter, email, password);
  const accountId = createAccountForUser(cfg, vu, iter, userId);
  deposit(cfg, vu, iter, accountId, 100);
}
