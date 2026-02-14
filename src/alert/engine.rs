use crate::alert::{rules, sink};
use crate::config::AlertsConfig;
use crate::models::{Alert, DecayOpportunity, GamingSignal, PoolQueuePosition, ThreatProfile};
use anyhow::Result;
use reqwest::Client;

pub async fn run_alert_engine(
    config: &AlertsConfig,
    threats: &[ThreatProfile],
    opportunities: &[DecayOpportunity],
    queue_positions: &[PoolQueuePosition],
    gaming_signals: &[GamingSignal],
    client: &Client,
) -> Result<Vec<Alert>> {
    let alerts = rules::evaluate_rules(
        &config.rules,
        threats,
        opportunities,
        queue_positions,
        gaming_signals,
    );
    sink::dispatch_alerts(config, &alerts, client).await?;
    Ok(alerts)
}
