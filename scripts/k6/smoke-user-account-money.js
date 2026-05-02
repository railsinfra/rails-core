/**
 * Smoke: auto-provision tenant (register + API key) → SDK user → account (user_id) → deposit.
 *
 * Run: `make k6-smoke` (loads repo `.env` for optional INTERNAL_SERVICE_TOKEN_ALLOWLIST token).
 */
import { buildRuntimeConfig } from "./config.js";
import { createSdkUser } from "./lib/users.js";
import { createAccountForUser, deposit } from "./lib/accounts.js";

export const options = {
  vus: 1,
  iterations: 1,
};

export function setup() {
  return buildRuntimeConfig();
}

export default function (cfg) {
  const vu = __VU;
  const iter = __ITER;
  const email = `k6+vu${vu}+i${iter}+${Date.now()}@example.com`;
  const password = __ENV.K6_USER_PASSWORD || "password123!";

  const userId = createSdkUser(cfg, vu, iter, email, password);
  const accountId = createAccountForUser(cfg, vu, iter, userId);
  deposit(cfg, vu, iter, accountId, 100);
}
