import { bootstrapTenant } from "./lib/bootstrap.js";

/**
 * URL presets: K6_TARGET=docker|staging|prod
 * - docker: http://127.0.0.1:8080/{users,accounts} (compose gateway)
 * - staging|prod: set K6_GATEWAY_URL to scheme+host (no path), e.g. https://api.staging.example.com
 *
 * Credentials: by default k6 setup() bootstraps a throwaway business + API key.
 * To use existing secrets instead: K6_SKIP_BOOTSTRAP=true and K6_API_KEY + K6_ORGANIZATION_ID.
 */
export function resolveTarget() {
  const target = (__ENV.K6_TARGET || "docker").toLowerCase();
  if (target === "prod" && (__ENV.K6_ALLOW_PROVISION_ON_PROD || "").toLowerCase() !== "true") {
    throw new Error(
      "K6_TARGET=prod refuses automated register by default. Use staging, or set K6_ALLOW_PROVISION_ON_PROD=true if you really intend to create businesses on prod.",
    );
  }

  const gateway = (__ENV.K6_GATEWAY_URL || "").trim().replace(/\/$/, "");
  if (gateway) {
    return {
      target,
      usersBase: `${gateway}/users`,
      accountsBase: `${gateway}/accounts`,
    };
  }

  if (target === "staging" || target === "prod") {
    throw new Error(
      `K6_TARGET=${target} requires K6_GATEWAY_URL (e.g. https://api.staging.example.com — no trailing slash).`,
    );
  }

  // docker (default)
  return {
    target,
    usersBase: "http://127.0.0.1:8080/users",
    accountsBase: "http://127.0.0.1:8080/accounts",
  };
}

export function buildRuntimeConfig() {
  const { usersBase, accountsBase, target } = resolveTarget();
  const environment = (__ENV.K6_ENVIRONMENT || "sandbox").toLowerCase();
  const syntheticIpPrefix = (__ENV.K6_SYNTHETIC_IP_PREFIX || "203.0.113").trim();

  if (environment !== "sandbox" && environment !== "production") {
    throw new Error("K6_ENVIRONMENT must be sandbox or production.");
  }

  const skip = (__ENV.K6_SKIP_BOOTSTRAP || "").toLowerCase() === "true";
  const apiKeyEnv = (__ENV.K6_API_KEY || "").trim();
  const orgEnv = (__ENV.K6_ORGANIZATION_ID || "").trim();

  if (skip) {
    if (!apiKeyEnv) {
      throw new Error("K6_SKIP_BOOTSTRAP=true requires K6_API_KEY.");
    }
    return {
      target,
      usersBase: usersBase.replace(/\/$/, ""),
      accountsBase: accountsBase.replace(/\/$/, ""),
      apiKey: apiKeyEnv,
      organizationId: orgEnv || null,
      environment,
      syntheticIpPrefix,
      bootstrapped: false,
    };
  }

  if (apiKeyEnv) {
    // Explicit key: do not register; still allow missing org (optional on account create).
    return {
      target,
      usersBase: usersBase.replace(/\/$/, ""),
      accountsBase: accountsBase.replace(/\/$/, ""),
      apiKey: apiKeyEnv,
      organizationId: orgEnv || null,
      environment,
      syntheticIpPrefix,
      bootstrapped: false,
    };
  }

  const base = {
    target,
    usersBase: usersBase.replace(/\/$/, ""),
    accountsBase: accountsBase.replace(/\/$/, ""),
    environment,
    syntheticIpPrefix,
  };
  const creds = bootstrapTenant(base);
  return {
    ...base,
    apiKey: creds.apiKey,
    organizationId: creds.organizationId,
    environment: creds.environment || environment,
    syntheticIpPrefix,
    bootstrapped: true,
  };
}
