/**
 * Central env for k6 scripts. Defaults target the repo gateway (docker compose :8080).
 *
 * Override for staging/prod by setting K6_USERS_BASE_URL and K6_ACCOUNTS_BASE_URL
 * to your BFF or gateway paths that expose the same API prefixes as nginx here:
 *   /users  -> users-service
 *   /accounts -> accounts-service
 */
export function getConfig() {
  const usersBase = (__ENV.K6_USERS_BASE_URL || "http://127.0.0.1:8080/users").replace(/\/$/, "");
  const accountsBase = (__ENV.K6_ACCOUNTS_BASE_URL || "http://127.0.0.1:8080/accounts").replace(/\/$/, "");
  const apiKey = __ENV.K6_API_KEY || "";
  const environment = (__ENV.K6_ENVIRONMENT || "sandbox").toLowerCase();
  const organizationId = (__ENV.K6_ORGANIZATION_ID || "").trim();
  const syntheticIpPrefix = (__ENV.K6_SYNTHETIC_IP_PREFIX || "203.0.113").trim();

  if (!apiKey) {
    throw new Error("K6_API_KEY is required (sandbox API key from your business).");
  }
  if (environment !== "sandbox" && environment !== "production") {
    throw new Error("K6_ENVIRONMENT must be sandbox or production.");
  }

  return {
    usersBase,
    accountsBase,
    apiKey,
    environment,
    organizationId: organizationId || null,
    syntheticIpPrefix,
  };
}
