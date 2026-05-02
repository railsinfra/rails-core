import http from "k6/http";

/**
 * Registers a throwaway business and mints a sandbox API key (admin JWT from register response).
 * No prior API key or org id required.
 */
export function bootstrapTenant(base) {
  const ts = Date.now();
  const adminEmail = `k6-auto-${ts}@example.com`;
  const adminPassword = `K6Auto_${ts}_pw_A1!`;
  const regBody = {
    name: `k6 auto ${ts}`,
    website: null,
    admin_first_name: "K6",
    admin_last_name: "Auto",
    admin_email: adminEmail,
    admin_password: adminPassword,
  };
  const headers = { "Content-Type": "application/json" };
  const internal = (__ENV.K6_INTERNAL_SERVICE_TOKEN || "").trim();
  if (internal) {
    headers["x-internal-service-token"] = internal;
  }

  const regRes = http.post(`${base.usersBase}/api/v1/business/register`, JSON.stringify(regBody), {
    headers,
  });
  if (regRes.status !== 200) {
    throw new Error(
      `bootstrap register failed: status=${regRes.status} body=${String(regRes.body).slice(0, 800)}`,
    );
  }
  const reg = regRes.json();
  const businessId = reg.business_id;
  const accessToken = reg.access_token;
  const environments = reg.environments || [];
  const sandbox = environments.find((e) => e.type === "sandbox");
  if (!sandbox || !sandbox.id) {
    throw new Error(`bootstrap: no sandbox environment in register response: ${String(regRes.body).slice(0, 500)}`);
  }
  const sandboxEnvId = sandbox.id;

  const keyHeaders = {
    "Content-Type": "application/json",
    Authorization: `Bearer ${accessToken}`,
    "X-Environment-Id": sandboxEnvId,
  };
  const keyRes = http.post(
    `${base.usersBase}/api/v1/api-keys`,
    JSON.stringify({ environment_id: sandboxEnvId }),
    { headers: keyHeaders },
  );
  if (keyRes.status !== 200) {
    throw new Error(
      `bootstrap api-key failed: status=${keyRes.status} body=${String(keyRes.body).slice(0, 800)}`,
    );
  }
  const keyJson = keyRes.json();
  const apiKey = keyJson.key;
  if (!apiKey) {
    throw new Error(`bootstrap: missing api key in response: ${String(keyRes.body).slice(0, 500)}`);
  }

  return {
    apiKey,
    organizationId: businessId,
    environment: "sandbox",
    sandboxEnvId,
  };
}
