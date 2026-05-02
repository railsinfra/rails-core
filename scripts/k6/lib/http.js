/**
 * Headers for money routes: Idempotency-Key is required on deposit/withdraw/transfer.
 * Optional X-Forwarded-For simulates distinct clients when ACCOUNTS_TRUSTED_PROXY_IPS trusts the gateway hop.
 */
export function moneyHeaders(cfg, vu, iter, extra = {}) {
  const idem = `k6-${vu}-${iter}-${Date.now()}`;
  const h = {
    "Content-Type": "application/json",
    "X-Environment": cfg.environment,
    "X-Forwarded-For": syntheticForwardedFor(cfg, vu),
    "Idempotency-Key": idem,
    "X-Correlation-Id": idem,
  };
  return Object.assign(h, extra);
}

function syntheticForwardedFor(cfg, vu) {
  const n = ((vu - 1) % 250) + 1;
  return `${cfg.syntheticIpPrefix}.${n}`;
}
