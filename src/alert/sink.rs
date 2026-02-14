use crate::config::AlertsConfig;
use crate::models::Alert;
use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::json;
use tracing::warn;

pub async fn dispatch_alerts(
    config: &AlertsConfig,
    alerts: &[Alert],
    client: &Client,
) -> Result<()> {
    if alerts.is_empty() {
        return Ok(());
    }

    if config.enable_stdout {
        for alert in alerts {
            println!(
                "[{}][{}] {} — {}",
                alert.severity, alert.category, alert.title, alert.body
            );
        }
    }

    if !config.discord_webhook.trim().is_empty() {
        let content = alerts
            .iter()
            .map(|alert| format!("**{}**: {}", alert.title, alert.body))
            .collect::<Vec<_>>()
            .join("\n");
        let payload = json!({ "content": content });
        if let Err(err) = client
            .post(&config.discord_webhook)
            .json(&payload)
            .send()
            .await
        {
            warn!("failed to deliver Discord alert payload: {err:#}");
        }
    }

    if !config.telegram_bot_token.trim().is_empty() && !config.telegram_chat_id.trim().is_empty() {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            config.telegram_bot_token
        );
        let text = alerts
            .iter()
            .map(|alert| format!("[{}] {}\n{}", alert.severity, alert.title, alert.body))
            .collect::<Vec<_>>()
            .join("\n\n");
        client
            .post(url)
            .json(&json!({
                "chat_id": config.telegram_chat_id,
                "text": text,
            }))
            .send()
            .await
            .context("failed to deliver telegram alert payload")?;
    }

    Ok(())
}
