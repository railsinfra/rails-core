import http from "k6/http";
import { check } from "k6";

/**
 * Create a platform user via users-service POST /api/v1/users (SDK, API key).
 * Returns user_id (UUID string) on success.
 */
export function createSdkUser(cfg, vu, iter, email, password) {
  const url = `${cfg.usersBase}/api/v1/users`;
  const res = http.post(
    url,
    JSON.stringify({
      email,
      first_name: "K6",
      last_name: `Vu${vu}`,
      password,
    }),
    {
      headers: {
        "Content-Type": "application/json",
        "X-API-Key": cfg.apiKey,
        "X-Environment": cfg.environment,
        "X-Correlation-Id": `k6-user-${vu}-${iter}`,
      },
    },
  );
  const ok = check(res, {
    "create user 200": (r) => r.status === 200,
  });
  if (!ok || res.status !== 200) {
    throw new Error(`createSdkUser failed: status=${res.status} body=${String(res.body).slice(0, 500)}`);
  }
  const body = res.json();
  if (!body.user_id) {
    throw new Error(`createSdkUser: missing user_id in ${String(res.body).slice(0, 500)}`);
  }
  return String(body.user_id);
}
