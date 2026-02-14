use crate::models::{GamingSignal, GamingType, ValidatorSnapshot};
use std::collections::HashMap;

pub fn detect_gaming_signals(
    histories: &HashMap<String, Vec<ValidatorSnapshot>>,
    min_confidence: f64,
) -> Vec<GamingSignal> {
    let mut signals = Vec::new();

    for (vote_pubkey, history) in histories {
        if history.len() < 4 {
            continue;
        }
        let latest_epoch = history.last().map(|s| s.epoch).unwrap_or(0);

        if let Some(signal) = detect_commission_sniping(vote_pubkey, history, latest_epoch) {
            if signal.confidence >= min_confidence {
                signals.push(signal);
            }
        }

        if let Some(signal) = detect_credit_manipulation(vote_pubkey, history, latest_epoch) {
            if signal.confidence >= min_confidence {
                signals.push(signal);
            }
        }

        if let Some(signal) = detect_self_stake_inflation(vote_pubkey, history, latest_epoch) {
            if signal.confidence >= min_confidence {
                signals.push(signal);
            }
        }
    }

    signals.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    signals
}

fn detect_commission_sniping(
    vote_pubkey: &str,
    history: &[ValidatorSnapshot],
    first_detected_epoch: u64,
) -> Option<GamingSignal> {
    let mut lowered = false;
    let mut raised_after = false;
    for pair in history.windows(2) {
        let prev = &pair[0];
        let curr = &pair[1];
        if curr.commission_pct < prev.commission_pct {
            lowered = true;
        }
        if lowered && curr.commission_pct > prev.commission_pct {
            raised_after = true;
            break;
        }
    }

    if lowered && raised_after {
        Some(GamingSignal {
            vote_pubkey: vote_pubkey.to_string(),
            signal_type: GamingType::CommissionSniping,
            confidence: 0.78,
            evidence: vec![
                "Commission lowered and then raised in short epoch window".to_string(),
                "Pattern is consistent with pool snapshot timing optimization".to_string(),
            ],
            first_detected_epoch,
        })
    } else {
        None
    }
}

fn detect_credit_manipulation(
    vote_pubkey: &str,
    history: &[ValidatorSnapshot],
    first_detected_epoch: u64,
) -> Option<GamingSignal> {
    let mut largest_jump: f64 = 0.0;
    for pair in history.windows(2) {
        let prev = &pair[0];
        let curr = &pair[1];
        if prev.vote_credits_epoch == 0 {
            continue;
        }
        let ratio = curr.vote_credits_epoch as f64 / prev.vote_credits_epoch as f64;
        largest_jump = largest_jump.max((ratio - 1.0).abs());
    }

    if largest_jump > 0.55 {
        Some(GamingSignal {
            vote_pubkey: vote_pubkey.to_string(),
            signal_type: GamingType::VoteCreditManipulation,
            confidence: 0.66 + (largest_jump.min(1.0) * 0.2),
            evidence: vec![format!(
                "Epoch-to-epoch vote credit variation was unusually high ({:.2}x)",
                largest_jump + 1.0
            )],
            first_detected_epoch,
        })
    } else {
        None
    }
}

fn detect_self_stake_inflation(
    vote_pubkey: &str,
    history: &[ValidatorSnapshot],
    first_detected_epoch: u64,
) -> Option<GamingSignal> {
    let mut growth_events = 0u32;
    let mut growth_sol = 0.0;
    for pair in history.windows(2) {
        let prev = &pair[0];
        let curr = &pair[1];
        let delta = curr.activated_stake_sol - prev.activated_stake_sol;
        if delta > 0.0 && curr.commission_pct == 0 {
            growth_events += 1;
            growth_sol += delta;
        }
    }

    let baseline = history
        .first()
        .map(|snapshot| snapshot.activated_stake_sol)
        .unwrap_or(0.0);
    let growth_ratio = if baseline > 0.0 {
        growth_sol / baseline
    } else {
        0.0
    };

    if growth_events >= 2 && growth_ratio > 0.30 {
        Some(GamingSignal {
            vote_pubkey: vote_pubkey.to_string(),
            signal_type: GamingType::SelfStakeInflation,
            confidence: (0.6 + growth_ratio).min(0.95),
            evidence: vec![
                format!(
                    "Detected {growth_events} concentrated stake inflow events at 0% commission"
                ),
                format!(
                    "Total growth represented {:.1}% of baseline stake",
                    growth_ratio * 100.0
                ),
            ],
            first_detected_epoch,
        })
    } else {
        None
    }
}
