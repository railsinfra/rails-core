use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::Request;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct RateLimitConfig {
    pub window: Duration,
    pub max: u32,
}

struct RateLimitWindow {
    start: Instant,
    count: u32,
}

pub struct RateLimiter {
    store: Mutex<HashMap<String, RateLimitWindow>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
            config,
        }
    }

    pub fn allow(&self, client_key: &str) -> bool {
        let mut store = self.store.lock().expect("rate limiter lock poisoned");
        let now = Instant::now();
        let entry = store
            .entry(client_key.to_string())
            .or_insert(RateLimitWindow { start: now, count: 0 });

        if now.duration_since(entry.start) > self.config.window {
            entry.start = now;
            entry.count = 0;
        }

        if entry.count >= self.config.max {
            return false;
        }

        entry.count += 1;
        true
    }

    /// Clears all clients (used by unit tests; production uses process restart).
    #[cfg(test)]
    pub fn reset(&self) {
        self.store.lock().expect("rate limiter lock poisoned").clear();
    }
}

pub fn extract_client_key(req: &Request<Body>, trusted_proxy_env: &str) -> String {
    if let Some(peer) = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip())
    {
        let trusted_proxies = trusted_proxy_ips(trusted_proxy_env);
        let peer_ip = peer.to_string();
        if trusted_proxies.contains(&peer) {
            if let Some(forwarded_ip) = extract_forwarded_ip(req, peer, &trusted_proxies) {
                return forwarded_ip;
            }
            if let Some(real_ip) = extract_real_ip(req) {
                return real_ip;
            }
        }
        return peer_ip;
    }

    extract_real_ip(req).unwrap_or_else(|| "unknown".to_string())
}

fn extract_forwarded_ip(
    req: &Request<Body>,
    peer: IpAddr,
    trusted_proxies: &[IpAddr],
) -> Option<String> {
    let forwarded_ips = parse_forwarded_for(req)?;
    let last_hop = forwarded_ips.last().copied()?;
    if last_hop != peer && !trusted_proxies.contains(&last_hop) {
        return None;
    }

    forwarded_ips
        .iter()
        .rev()
        .find(|ip| !trusted_proxies.contains(ip))
        .map(|ip| ip.to_string())
}

fn parse_forwarded_for(req: &Request<Body>) -> Option<Vec<IpAddr>> {
    let forwarded_for = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())?;
    let mut ips = Vec::new();
    for value in forwarded_for.split(',') {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(ip) = trimmed.parse::<IpAddr>() {
            ips.push(ip);
        }
    }
    if ips.is_empty() {
        return None;
    }
    Some(ips)
}

fn extract_real_ip(req: &Request<Body>) -> Option<String> {
    let real_ip = req
        .headers()
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())?
        .trim()
        .to_string();
    if real_ip.is_empty() {
        return None;
    }
    real_ip.parse::<IpAddr>().ok().map(|ip| ip.to_string())
}

fn trusted_proxy_ips(var_name: &str) -> Vec<IpAddr> {
    std::env::var(var_name)
        .ok()
        .unwrap_or_default()
        .split(',')
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .filter_map(|value| value.parse::<IpAddr>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{extract_client_key, RateLimitConfig, RateLimiter};
    use crate::test_support::global_test_lock;
    use axum::body::Body;
    use axum::http::Request;
    use std::time::Duration;

    #[test]
    fn rate_limit_window_resets_after_elapsed() {
        let limiter = RateLimiter::new(RateLimitConfig {
            window: Duration::from_millis(40),
            max: 1,
        });
        assert!(limiter.allow("c1"));
        assert!(!limiter.allow("c1"));
        std::thread::sleep(Duration::from_millis(60));
        assert!(limiter.allow("c1"));
    }

    #[test]
    static USERS_TRUSTED_PROXY_IPS: &str = "USERS_TRUSTED_PROXY_IPS";
    fn extract_client_key_trusted_proxy_falls_back_to_x_real_ip() {
        let _l = global_test_lock();
        std::env::set_var(USERS_TRUSTED_PROXY_IPS, "127.0.0.1");
        let mut req = Request::builder()
            .uri("/api/v1/auth/login")
            .header("x-real-ip", "198.51.100.33")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            std::net::SocketAddr::from(([127, 0, 0, 1], 8080)),
        ));
        assert_eq!(
            extract_client_key(&req, "USERS_TRUSTED_PROXY_IPS"),
            "198.51.100.33"
        );
    }

    #[test]
    fn extract_client_key_forwarded_for_skips_empty_segments() {
        let _l = global_test_lock();
        std::env::set_var(USERS_TRUSTED_PROXY_IPS, "127.0.0.1");
        let mut req = Request::builder()
            .uri("/api/v1/auth/login")
            .header("x-forwarded-for", " , 203.0.113.10 , 127.0.0.1 ")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            std::net::SocketAddr::from(([127, 0, 0, 1], 8080)),
        ));
        assert_eq!(
            extract_client_key(&req, USERS_TRUSTED_PROXY_IPS),
            "203.0.113.10"
        );
    }

    #[test]
    fn extract_client_key_real_ip_ignores_invalid_ip_string() {
        let _l = global_test_lock();
        std::env::remove_var(USERS_TRUSTED_PROXY_IPS);
        let req = Request::builder()
            .uri("/api/v1/auth/login")
            .header("x-real-ip", "not-an-ip")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_client_key(&req, "USERS_TRUSTED_PROXY_IPS"), "unknown");
    }

    #[test]
    static USERS_TRUSTED_PROXY_IPS: &str = "USERS_TRUSTED_PROXY_IPS";

    fn extract_client_key_falls_back_to_x_real_ip_without_connect_info() {
        let _l = global_test_lock();
        std::env::remove_var(USERS_TRUSTED_PROXY_IPS);
        let req = Request::builder()
            .uri("/api/v1/auth/login")
            .header("x-real-ip", "198.51.100.22")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            extract_client_key(&req, USERS_TRUSTED_PROXY_IPS),
            "198.51.100.22"
        );
    }

    #[test]
    static USERS_TRUSTED_PROXY_IPS: &str = "USERS_TRUSTED_PROXY_IPS";

    fn extract_client_key_returns_unknown_when_no_ip_hints() {
        let _l = global_test_lock();
        std::env::remove_var(USERS_TRUSTED_PROXY_IPS);
        let req = Request::builder()
            .uri("/api/v1/auth/login")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            extract_client_key(&req, USERS_TRUSTED_PROXY_IPS),
            "unknown"
        );
    }
}
