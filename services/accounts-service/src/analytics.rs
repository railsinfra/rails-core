use serde_json::{json, Map, Value};

const POSTHOG_API_KEY_ENV: &str = "POSTHOG_API_KEY";
const POSTHOG_HOST_ENV: &str = "POSTHOG_HOST";
const DEFAULT_POSTHOG_HOST: &str = "https://app.posthog.com";

pub async fn capture_event(event: &str, distinct_id: &str, properties: Value) {
    let Some(api_key) = std::env::var(POSTHOG_API_KEY_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        tracing::debug!(event, "PostHog not configured; skipping capture");
        return;
    };

    let host = std::env::var(POSTHOG_HOST_ENV)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_POSTHOG_HOST.to_string());

    let mut props = match properties {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    props.insert("service".to_string(), Value::String("accounts".to_string()));

    let payload = json!({
        "api_key": api_key,
        "event": event,
        "distinct_id": distinct_id,
        "properties": Value::Object(props),
    });

    let result = reqwest::Client::new()
        .post(format!("{host}/capture"))
        .json(&payload)
        .send()
        .await;

    match result {
        Ok(response) if response.status().is_success() => {}
        Ok(response) => {
            tracing::warn!(
                event,
                status = response.status().as_u16(),
                "PostHog capture failed"
            );
        }
        Err(error) => {
            tracing::warn!(event, error = %error, "PostHog capture failed");
        }
    }
}

pub fn spawn_capture_event(event: &'static str, distinct_id: String, properties: Value) {
    tokio::spawn(async move {
        capture_event(event, &distinct_id, properties).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[tokio::test]
    async fn capture_event_noops_without_api_key() {
        let _lock = env_lock();
        std::env::remove_var(POSTHOG_API_KEY_ENV);
        capture_event("account_creation_attempted", "user-1", json!({})).await;
    }

    #[tokio::test]
    async fn capture_event_posts_to_configured_host() {
        let _lock = env_lock();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let body_seen = Arc::new(Mutex::new(String::default()));
        let body_seen_thread = body_seen.clone();
        let join = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0_u8; 4096];
            let n = stream.read(&mut buf).expect("read");
            *body_seen_thread.lock().expect("lock") =
                String::from_utf8_lossy(&buf[..n]).to_string();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}")
                .expect("write");
        });

        std::env::set_var(POSTHOG_API_KEY_ENV, "phc_test");
        std::env::set_var(POSTHOG_HOST_ENV, format!("http://{addr}"));
        capture_event("account_created", "user-1", json!({"result":"ok"})).await;
        join.join().expect("server thread");
        let body = body_seen.lock().expect("lock").clone();
        assert!(body.contains("account_created"), "{body}");
        assert!(body.contains("phc_test"), "{body}");
        assert!(body.contains("accounts"), "{body}");
        std::env::remove_var(POSTHOG_API_KEY_ENV);
        std::env::remove_var(POSTHOG_HOST_ENV);
    }

    #[tokio::test]
    async fn capture_event_accepts_non_object_properties_and_non_success_response() {
        let _lock = env_lock();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let body_seen = Arc::new(Mutex::new(String::default()));
        let body_seen_thread = body_seen.clone();
        let join = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0_u8; 4096];
            let n = stream.read(&mut buf).expect("read");
            *body_seen_thread.lock().expect("lock") =
                String::from_utf8_lossy(&buf[..n]).to_string();
            stream
                .write_all(b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 2\r\n\r\n{}")
                .expect("write");
        });

        std::env::set_var(POSTHOG_API_KEY_ENV, "phc_test");
        std::env::set_var(POSTHOG_HOST_ENV, format!("http://{addr}"));
        capture_event("account_creation_failed", "user-1", json!("not-an-object")).await;
        join.join().expect("server thread");
        let body = body_seen.lock().expect("lock").clone();
        assert!(body.contains("account_creation_failed"), "{body}");
        assert!(body.contains("accounts"), "{body}");
        std::env::remove_var(POSTHOG_API_KEY_ENV);
        std::env::remove_var(POSTHOG_HOST_ENV);
    }

    #[tokio::test]
    async fn capture_event_logs_and_returns_on_transport_failure() {
        let _lock = env_lock();
        std::env::set_var(POSTHOG_API_KEY_ENV, "phc_test");
        std::env::set_var(POSTHOG_HOST_ENV, "http://127.0.0.1:1");
        capture_event("account_creation_failed", "user-1", json!({})).await;
        std::env::remove_var(POSTHOG_API_KEY_ENV);
        std::env::remove_var(POSTHOG_HOST_ENV);
    }
}
