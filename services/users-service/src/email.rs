//! Email service using Resend API
//! Simple, secure email delivery for password reset flows

use crate::config::Config;
use serde_json::{json, Value};
use tracing::{debug, error, warn};

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[derive(Clone)]
pub struct EmailService {
    api_key: Option<String>,
    from_email: String,
    from_name: String,
    base_url: String,
    beta_notification_email: String,
    frontend_base_url: String,
}

impl EmailService {
    pub fn new(config: &Config) -> Self {
        Self {
            api_key: config.resend_api_key.clone(),
            from_email: config.resend_from_email.clone(),
            from_name: config.resend_from_name.clone(),
            base_url: config.resend_base_url.clone(),
            beta_notification_email: config.resend_beta_notification_email.clone(),
            frontend_base_url: config.frontend_base_url.clone(),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    /// Send password reset email
    /// Returns Ok(()) on success, Err on failure
    /// Failures are logged but don't expose user existence
    pub async fn send_password_reset(&self, email: &str, token: &str) -> Result<(), String> {
        if !self.is_configured() {
            warn!("Resend API key not configured. Skipping password reset email.");
            return Err("Email service not configured".to_string());
        }

        let reset_url = format!("{}/reset-password?token={}", self.frontend_base_url, token);
        let escaped_reset_url = escape_html(&reset_url);
        
        // Simple text-first email template
        let subject = "Reset Your Password";
        let text_body = format!(
            "You requested a password reset for your Rails account.\n\n\
            Click the link below to reset your password:\n\n\
            {}\n\n\
            This link will expire in 1 hour.\n\n\
            If you didn't request this, you can safely ignore this email.\n\n\
            — Rails Financial Infrastructure",
            reset_url
        );
        
        let html_body = format!(
            "<!DOCTYPE html>\n\
            <html>\n\
            <head><meta charset=\"utf-8\"></head>\n\
            <body style=\"font-family: system-ui, sans-serif; line-height: 1.6; color: #333;\">\n\
            <p>You requested a password reset for your Rails account.</p>\n\
            <p><a href=\"{}\" style=\"display: inline-block; padding: 12px 24px; background: #000; color: #fff; text-decoration: none; border-radius: 6px;\">Reset Password</a></p>\n\
            <p style=\"font-size: 14px; color: #666;\">This link will expire in 1 hour.</p>\n\
            <p style=\"font-size: 14px; color: #666;\">If you didn't request this, you can safely ignore this email.</p>\n\
            <hr style=\"border: none; border-top: 1px solid #eee; margin: 24px 0;\">\n\
            <p style=\"font-size: 12px; color: #999;\">— Rails Financial Infrastructure</p>\n\
            </body>\n\
            </html>",
            escaped_reset_url
        );

        let payload = json!({
            "from": format!("{} <{}>", self.from_name, self.from_email),
            "to": [email],
            "subject": subject,
            "text": text_body,
            "html": html_body,
        });

        self.send_email(payload).await?;
        debug!("Password reset email sent successfully");
        Ok(())
    }

    /// Send private beta application email to internal notification address
    pub async fn send_beta_application(
        &self,
        name: &str,
        email: &str,
        company: &str,
        use_case: &str,
    ) -> Result<(), String> {
        if !self.is_configured() {
            warn!("Resend API key not configured. Skipping beta application email.");
            return Err("Email service not configured".to_string());
        }

        let subject = "We've received your private beta application";
        let escaped_name = escape_html(name);
        let escaped_company = escape_html(company);
        let escaped_use_case = escape_html(use_case);
        let text_body = format!(
            "Thanks for applying to the Rails private beta.\n\n\
            We've received your application and will follow up shortly.\n\n\
            Summary:\n\
            Name: {}\n\
            Company: {}\n\
            Primary Use Case:\n{}\n\n\
            — Rails Financial Infrastructure",
            name, company, use_case
        );
        let html_body = format!(
            "<!DOCTYPE html>\n\
            <html>\n\
            <head><meta charset=\"utf-8\"></head>\n\
            <body style=\"font-family: system-ui, sans-serif; line-height: 1.6; color: #333;\">\n\
            <h2>Thanks for applying to the Rails private beta</h2>\n\
            <p>We've received your application and will follow up shortly.</p>\n\
            <p><strong>Summary</strong></p>\n\
            <p><strong>Name:</strong> {}</p>\n\
            <p><strong>Company:</strong> {}</p>\n\
            <p><strong>Primary Use Case:</strong></p>\n\
            <p style=\"white-space: pre-wrap;\">{}</p>\n\
            <hr style=\"border: none; border-top: 1px solid #eee; margin: 24px 0;\">\n\
            <p style=\"font-size: 12px; color: #999;\">— Rails Financial Infrastructure</p>\n\
            </body>\n\
            </html>",
            escaped_name, escaped_company, escaped_use_case
        );

        let payload = json!({
            "from": format!("{} <{}>", self.from_name, self.from_email),
            "to": [email],
            "reply_to": self.beta_notification_email,
            "subject": subject,
            "text": text_body,
            "html": html_body,
        });

        self.send_email(payload).await?;
        debug!("Beta application email sent successfully");
        Ok(())
    }

    async fn send_email(&self, payload: Value) -> Result<(), String> {
        let client = reqwest::Client::new();
        let api_key = self.api_key.as_ref().unwrap();

        match client
            .post(format!("{}/emails", self.base_url))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    Ok(())
                } else {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_default();
                    error!(
                        "Resend API error: status={}, response={}",
                        status, error_text
                    );
                    Err(format!("Failed to send email: {}", status))
                }
            }
            Err(e) => {
                error!("Failed to send email: {}", e);
                Err(format!("Network error: {}", e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{escape_html, EmailService};
    use crate::config::Config;
    use httpmock::Method::POST;
    use httpmock::MockServer;

    #[test]
    fn escape_html_replaces_entities() {
        let input = r#"Tom & Jerry < "quote" 'single'"#;
        let expected = "Tom &amp; Jerry &lt; &quot;quote&quot; &#39;single&#39;";

        assert_eq!(escape_html(input), expected);
    }

    #[tokio::test]
    async fn send_password_reset_hits_resend_when_configured() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/emails");
                then.status(200).body(r#"{"id":"1"}"#);
            })
            .await;

        let cfg = Config {
            database_url: "postgresql://stub".into(),
            server_addr: "0.0.0.0:0".into(),
            grpc_port: 50051,
            accounts_grpc_url: "http://127.0.0.1:1".into(),
            audit_grpc_url: "http://127.0.0.1:1".into(),
            sentry_dsn: None,
            environment: "test".into(),
            resend_api_key: Some("secret".into()),
            resend_from_email: "from@example.com".into(),
            resend_from_name: "Rails".into(),
            resend_base_url: server.base_url(),
            resend_beta_notification_email: "beta@example.com".into(),
            frontend_base_url: "http://localhost:5173".into(),
        };
        let svc = EmailService::new(&cfg);
        assert!(svc.is_configured());
        svc
            .send_password_reset("user@example.com", "tok")
            .await
            .expect("email ok");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn send_password_reset_not_configured_returns_error() {
        let cfg = Config {
            database_url: "postgresql://stub".into(),
            server_addr: "0.0.0.0:0".into(),
            grpc_port: 50051,
            accounts_grpc_url: "http://127.0.0.1:1".into(),
            audit_grpc_url: "http://127.0.0.1:1".into(),
            sentry_dsn: None,
            environment: "test".into(),
            resend_api_key: None,
            resend_from_email: "from@example.com".into(),
            resend_from_name: "Rails".into(),
            resend_base_url: "https://api.resend.com".into(),
            resend_beta_notification_email: "beta@example.com".into(),
            frontend_base_url: "http://localhost:5173".into(),
        };
        let svc = EmailService::new(&cfg);
        assert!(!svc.is_configured());
        let err = svc
            .send_password_reset("user@example.com", "tok")
            .await
            .expect_err("not configured");
        assert!(err.contains("not configured"));
    }

    #[tokio::test]
    async fn send_beta_application_hits_resend_when_configured() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/emails");
                then.status(200).body(r#"{"id":"2"}"#);
            })
            .await;

        let cfg = Config {
            database_url: "postgresql://stub".into(),
            server_addr: "0.0.0.0:0".into(),
            grpc_port: 50051,
            accounts_grpc_url: "http://127.0.0.1:1".into(),
            audit_grpc_url: "http://127.0.0.1:1".into(),
            sentry_dsn: None,
            environment: "test".into(),
            resend_api_key: Some("secret".into()),
            resend_from_email: "from@example.com".into(),
            resend_from_name: "Rails".into(),
            resend_base_url: server.base_url(),
            resend_beta_notification_email: "notify@example.com".into(),
            frontend_base_url: "http://localhost:5173".into(),
        };
        let svc = EmailService::new(&cfg);
        svc
            .send_beta_application("Pat", "pat@example.com", "Co", "Use case")
            .await
            .expect("beta email ok");
        mock.assert_async().await;
    }
}
