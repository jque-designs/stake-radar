use crate::config::AlertRulesConfig;
use crate::models::{
    Alert, AlertCategory, AlertSeverity, CaptureDifficulty, DecayOpportunity, GamingSignal,
    PoolQueuePosition, ThreatProfile, ThreatTier,
};
use chrono::Utc;
use uuid::Uuid;

pub fn evaluate_rules(
    rules: &AlertRulesConfig,
    threats: &[ThreatProfile],
    opportunities: &[DecayOpportunity],
    queue_positions: &[PoolQueuePosition],
    gaming_signals: &[GamingSignal],
) -> Vec<Alert> {
    let mut alerts = Vec::new();

    if rules.threat_critical {
        for threat in threats
            .iter()
            .filter(|threat| threat.threat_tier == ThreatTier::Critical)
        {
            alerts.push(Alert {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                severity: AlertSeverity::Critical,
                category: AlertCategory::ThreatApproaching,
                title: "Critical overtake risk detected".to_string(),
                body: format!(
                    "{} may overtake within {} epochs (velocity {:.2} SOL/epoch)",
                    threat.vote_pubkey,
                    threat
                        .epochs_to_overtake
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    threat.velocity_sol_per_epoch
                ),
                related_validator: Some(threat.vote_pubkey.clone()),
                metadata: serde_json::to_value(threat).unwrap_or_else(|_| serde_json::json!({})),
            });
        }
    }

    if rules.opportunity_easy {
        for opportunity in opportunities
            .iter()
            .filter(|opportunity| opportunity.capture_difficulty == CaptureDifficulty::Easy)
        {
            alerts.push(Alert {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                severity: AlertSeverity::Warning,
                category: AlertCategory::OpportunityWindow,
                title: "Easy capture opportunity".to_string(),
                body: format!(
                    "{} lost {:.0} SOL over {} epochs",
                    opportunity.vote_pubkey,
                    opportunity.stake_lost_sol,
                    opportunity.loss_duration_epochs
                ),
                related_validator: Some(opportunity.vote_pubkey.clone()),
                metadata: serde_json::to_value(opportunity)
                    .unwrap_or_else(|_| serde_json::json!({})),
            });
        }
    }

    if rules.queue_drop {
        for queue in queue_positions
            .iter()
            .filter(|queue| queue.direction == crate::models::QueueDirection::Falling)
        {
            alerts.push(Alert {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                severity: AlertSeverity::Warning,
                category: AlertCategory::QueuePositionDrop,
                title: "Stake pool queue position dropped".to_string(),
                body: format!(
                    "{} rank moved to #{} of {} in {}",
                    queue.pool, queue.rank, queue.total_eligible, queue.pool
                ),
                related_validator: None,
                metadata: serde_json::to_value(queue).unwrap_or_else(|_| serde_json::json!({})),
            });
        }
    }

    if rules.gaming_detected {
        for signal in gaming_signals {
            alerts.push(Alert {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                severity: AlertSeverity::Info,
                category: AlertCategory::GamingDetected,
                title: "Adversarial signal detected".to_string(),
                body: format!(
                    "{}: {} (confidence {:.2})",
                    signal.vote_pubkey, signal.signal_type, signal.confidence
                ),
                related_validator: Some(signal.vote_pubkey.clone()),
                metadata: serde_json::to_value(signal).unwrap_or_else(|_| serde_json::json!({})),
            });
        }
    }

    alerts
}
