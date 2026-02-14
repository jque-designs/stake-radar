use crate::models::{StakeSource, ThreatProfile, ThreatTier, ValidatorSnapshot};
use std::collections::HashMap;

pub fn analyze_threats(
    histories: &HashMap<String, Vec<ValidatorSnapshot>>,
    your_vote_pubkey: &str,
    overtake_horizon_epochs: u32,
) -> Vec<ThreatProfile> {
    let Some(your_history) = histories.get(your_vote_pubkey) else {
        return Vec::new();
    };
    let Some(your_current) = your_history.last() else {
        return Vec::new();
    };

    let your_stake = your_current.activated_stake_sol;
    let min_peer_stake = your_stake * 0.5;
    let max_peer_stake = your_stake * 1.5;

    let mut profiles = Vec::new();
    for (vote_pubkey, history) in histories {
        if vote_pubkey == your_vote_pubkey || history.len() < 3 {
            continue;
        }
        let Some(current) = history.last() else {
            continue;
        };
        if !(min_peer_stake..=max_peer_stake).contains(&current.activated_stake_sol) {
            continue;
        }

        let velocity = weighted_velocity(history, 0.85);
        let acceleration = acceleration(history);
        let eta = epochs_to_overtake(your_stake, current.activated_stake_sol, velocity);
        let threat_tier = classify_threat(velocity, eta, overtake_horizon_epochs);
        profiles.push(ThreatProfile {
            vote_pubkey: vote_pubkey.clone(),
            current_stake_sol: current.activated_stake_sol,
            velocity_sol_per_epoch: velocity,
            acceleration,
            epochs_to_overtake: eta,
            threat_tier,
            primary_stake_source: StakeSource::Unknown,
        });
    }

    profiles.sort_by(|a, b| {
        a.threat_tier
            .severity_rank()
            .cmp(&b.threat_tier.severity_rank())
            .then_with(|| match (a.epochs_to_overtake, b.epochs_to_overtake) {
                (Some(a_eta), Some(b_eta)) => a_eta.cmp(&b_eta),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => b
                    .velocity_sol_per_epoch
                    .total_cmp(&a.velocity_sol_per_epoch),
            })
    });
    profiles
}

pub fn weighted_velocity(history: &[ValidatorSnapshot], lambda: f64) -> f64 {
    if history.len() < 2 {
        return 0.0;
    }

    let mut weighted_x = 0.0;
    let mut weighted_y = 0.0;
    let mut weighted_xx = 0.0;
    let mut weighted_xy = 0.0;
    let mut total_weight = 0.0;
    let n = history.len();

    for (i, snapshot) in history.iter().enumerate() {
        let x = i as f64;
        let y = snapshot.activated_stake_sol;
        let age = (n - 1 - i) as i32;
        let weight = lambda.powi(age);
        total_weight += weight;
        weighted_x += weight * x;
        weighted_y += weight * y;
        weighted_xx += weight * x * x;
        weighted_xy += weight * x * y;
    }

    let numerator = total_weight * weighted_xy - weighted_x * weighted_y;
    let denominator = total_weight * weighted_xx - weighted_x * weighted_x;
    if denominator.abs() < f64::EPSILON {
        0.0
    } else {
        numerator / denominator
    }
}

pub fn acceleration(history: &[ValidatorSnapshot]) -> f64 {
    if history.len() < 5 {
        return 0.0;
    }
    let split = history.len() / 2;
    if split < 2 || history.len() - split < 2 {
        return 0.0;
    }
    let older_velocity = weighted_velocity(&history[..split], 0.85);
    let newer_velocity = weighted_velocity(&history[split..], 0.85);
    newer_velocity - older_velocity
}

fn epochs_to_overtake(your_stake: f64, their_stake: f64, their_velocity: f64) -> Option<u32> {
    if their_stake >= your_stake || their_velocity <= 0.0 {
        return None;
    }
    let epochs = (your_stake - their_stake) / their_velocity;
    if epochs.is_finite() && epochs >= 0.0 {
        Some(epochs.ceil() as u32)
    } else {
        None
    }
}

fn classify_threat(velocity: f64, eta: Option<u32>, horizon: u32) -> ThreatTier {
    if let Some(epochs) = eta {
        if epochs <= 3 {
            return ThreatTier::Critical;
        }
        if epochs <= 10 {
            return ThreatTier::Rising;
        }
        if epochs <= horizon && velocity > 0.0 {
            return ThreatTier::Watching;
        }
        return ThreatTier::Neutral;
    }

    if velocity > 0.0 {
        ThreatTier::Watching
    } else {
        ThreatTier::Neutral
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ValidatorSnapshot;

    fn snap(vote: &str, epoch: u64, stake: f64, commission: u8) -> ValidatorSnapshot {
        ValidatorSnapshot {
            vote_pubkey: vote.to_string(),
            identity: "identity".to_string(),
            epoch,
            slot_captured: 0,
            activated_stake_sol: stake,
            commission_pct: commission,
            vote_credits_epoch: 100,
            vote_credits_prior_epoch: 95,
            skip_rate: 0.05,
            delinquent: false,
            version: None,
            dc_location: None,
            superminority_member: false,
        }
    }

    #[test]
    fn weighted_velocity_is_positive_for_uptrend() {
        let history = vec![
            snap("a", 1, 100.0, 5),
            snap("a", 2, 110.0, 5),
            snap("a", 3, 121.0, 5),
            snap("a", 4, 133.0, 5),
        ];
        let velocity = weighted_velocity(&history, 0.85);
        assert!(velocity > 0.0);
    }

    #[test]
    fn detects_critical_eta() {
        let mut histories: HashMap<String, Vec<ValidatorSnapshot>> = HashMap::new();
        histories.insert(
            "you".to_string(),
            vec![
                snap("you", 1, 100.0, 5),
                snap("you", 2, 100.0, 5),
                snap("you", 3, 100.0, 5),
            ],
        );
        histories.insert(
            "them".to_string(),
            vec![
                snap("them", 1, 90.0, 5),
                snap("them", 2, 94.0, 5),
                snap("them", 3, 98.0, 5),
            ],
        );

        let threats = analyze_threats(&histories, "you", 15);
        assert_eq!(threats.len(), 1);
        assert_eq!(threats[0].threat_tier, ThreatTier::Critical);
    }
}
