use crate::models::{GamingSignal, GamingType, ValidatorSnapshot};
use crate::rpc::StakeAccountRecord;
use std::collections::HashMap;

pub fn detect_gaming_signals(
    histories: &HashMap<String, Vec<ValidatorSnapshot>>,
    stake_accounts: &[StakeAccountRecord],
    min_confidence: f64,
) -> Vec<GamingSignal> {
    let mut signals_by_key: HashMap<(String, String), GamingSignal> = HashMap::new();
    let stake_flows = build_stake_flow_summary(histories, stake_accounts);

    for (vote_pubkey, history) in histories {
        if history.len() < 4 {
            continue;
        }
        let latest_epoch = history.last().map(|s| s.epoch).unwrap_or(0);

        if let Some(signal) = detect_commission_sniping(vote_pubkey, history, latest_epoch) {
            merge_signal(&mut signals_by_key, signal, min_confidence);
        }
        if let Some(signal) = detect_credit_manipulation(vote_pubkey, history, latest_epoch) {
            merge_signal(&mut signals_by_key, signal, min_confidence);
        }
        if let Some(flow) = stake_flows.get(vote_pubkey.as_str()) {
            if let Some(signal) =
                detect_self_stake_inflation(vote_pubkey, history, flow, latest_epoch)
            {
                merge_signal(&mut signals_by_key, signal, min_confidence);
            }
        }
    }

    for signal in detect_sybil_clusters(&stake_flows) {
        merge_signal(&mut signals_by_key, signal, min_confidence);
    }

    let mut signals = signals_by_key.into_values().collect::<Vec<_>>();
    signals.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    signals
}

fn merge_signal(
    target: &mut HashMap<(String, String), GamingSignal>,
    signal: GamingSignal,
    min_confidence: f64,
) {
    if signal.confidence < min_confidence {
        return;
    }
    let key = (signal.vote_pubkey.clone(), signal.signal_type.to_string());
    target
        .entry(key)
        .and_modify(|existing| {
            if signal.confidence > existing.confidence {
                *existing = signal.clone();
            }
        })
        .or_insert(signal);
}

fn detect_commission_sniping(
    vote_pubkey: &str,
    history: &[ValidatorSnapshot],
    first_detected_epoch: u64,
) -> Option<GamingSignal> {
    if history.len() < 4 {
        return None;
    }

    for index in 1..history.len() {
        let prev = &history[index - 1];
        let curr = &history[index];
        let drop = prev.commission_pct as i16 - curr.commission_pct as i16;
        if drop < 2 {
            continue;
        }
        for next_index in (index + 1)..=usize::min(index + 2, history.len() - 1) {
            let rebound = history[next_index].commission_pct as i16 - curr.commission_pct as i16;
            if rebound < 2 {
                continue;
            }
            let stake_gain =
                (history[next_index].activated_stake_sol - curr.activated_stake_sol).max(0.0);
            let confidence =
                (0.55 + ((drop + rebound) as f64 / 20.0) + (stake_gain / 200_000.0)).min(0.96);
            return Some(GamingSignal {
                vote_pubkey: vote_pubkey.to_string(),
                signal_type: GamingType::CommissionSniping,
                confidence,
                evidence: vec![
                    format!(
                        "Commission dropped {}% -> {}% then rebounded to {}% within {} epochs",
                        prev.commission_pct,
                        curr.commission_pct,
                        history[next_index].commission_pct,
                        history[next_index].epoch.saturating_sub(curr.epoch)
                    ),
                    format!(
                        "Stake gained {:.0} SOL during low-commission period",
                        stake_gain
                    ),
                ],
                first_detected_epoch,
            });
        }
    }

    None
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
    flow: &StakeFlowSummary,
    first_detected_epoch: u64,
) -> Option<GamingSignal> {
    let latest = history.last()?;
    let baseline = history
        .iter()
        .rev()
        .take(4)
        .last()
        .map(|snapshot| snapshot.activated_stake_sol)
        .unwrap_or_else(|| {
            history
                .first()
                .map(|snapshot| snapshot.activated_stake_sol)
                .unwrap_or(0.0)
        });

    let recent_growth = (latest.activated_stake_sol - baseline).max(0.0);
    let growth_ratio = if baseline > 0.0 {
        recent_growth / baseline
    } else {
        0.0
    };
    let identity_ratio = if flow.total_delegated_stake_sol > 0.0 {
        flow.identity_linked_stake_sol / flow.total_delegated_stake_sol
    } else {
        0.0
    };

    if flow.identity_linked_stake_sol >= 500.0 && identity_ratio > 0.30 && recent_growth > 0.0 {
        let confidence =
            (0.45 + identity_ratio * 0.35 + growth_ratio.min(1.0) * 0.30).clamp(0.0, 0.98);
        Some(GamingSignal {
            vote_pubkey: vote_pubkey.to_string(),
            signal_type: GamingType::SelfStakeInflation,
            confidence,
            evidence: vec![
                format!(
                    "{:.0} SOL ({:.1}%) of delegated stake appears identity-linked",
                    flow.identity_linked_stake_sol,
                    identity_ratio * 100.0
                ),
                format!(
                    "Recent stake growth over last window: +{:.0} SOL ({:.1}%)",
                    recent_growth,
                    growth_ratio * 100.0
                ),
            ],
            first_detected_epoch,
        })
    } else {
        None
    }
}

fn detect_sybil_clusters(stake_flows: &HashMap<&str, StakeFlowSummary>) -> Vec<GamingSignal> {
    let mut authority_to_votes: HashMap<&str, Vec<(&str, f64)>> = HashMap::new();
    for flow in stake_flows.values() {
        for authority in &flow.authorities {
            authority_to_votes
                .entry(authority.as_str())
                .or_default()
                .push((flow.vote_pubkey.as_str(), flow.total_delegated_stake_sol));
        }
    }

    let mut signals = Vec::new();
    for (authority, linked_votes) in authority_to_votes {
        let mut unique_votes = linked_votes
            .iter()
            .map(|(vote, _)| vote.to_string())
            .collect::<Vec<_>>();
        unique_votes.sort();
        unique_votes.dedup();
        if unique_votes.len() < 3 {
            continue;
        }

        let combined_stake = linked_votes.iter().map(|(_, stake)| *stake).sum::<f64>();
        if combined_stake < 10_000.0 {
            continue;
        }
        let confidence =
            (0.55 + (unique_votes.len() as f64 / 10.0) + (combined_stake / 1_000_000.0).min(0.2))
                .min(0.97);

        for vote in &unique_votes {
            signals.push(GamingSignal {
                vote_pubkey: vote.clone(),
                signal_type: GamingType::SybilCluster,
                confidence,
                evidence: vec![
                    format!(
                        "Shared authority {} across {} validators",
                        authority,
                        unique_votes.len()
                    ),
                    format!(
                        "Cluster combined delegated stake: {:.0} SOL",
                        combined_stake
                    ),
                    format!("Cluster peers: {}", unique_votes.join(", ")),
                ],
                first_detected_epoch: 0,
            });
        }
    }
    signals
}

#[derive(Debug, Clone)]
struct StakeFlowSummary {
    vote_pubkey: String,
    total_delegated_stake_sol: f64,
    identity_linked_stake_sol: f64,
    authorities: Vec<String>,
}

fn build_stake_flow_summary<'a>(
    histories: &'a HashMap<String, Vec<ValidatorSnapshot>>,
    stake_accounts: &'a [StakeAccountRecord],
) -> HashMap<&'a str, StakeFlowSummary> {
    let mut latest_identity_by_vote = HashMap::new();
    for (vote, history) in histories {
        if let Some(latest) = history.last() {
            latest_identity_by_vote.insert(vote.as_str(), latest.identity.as_str());
        }
    }

    let mut by_vote: HashMap<&str, StakeFlowSummary> = HashMap::new();
    for account in stake_accounts {
        let Some(vote) = account.delegated_vote_pubkey.as_deref() else {
            continue;
        };
        let flow = by_vote.entry(vote).or_insert_with(|| StakeFlowSummary {
            vote_pubkey: vote.to_string(),
            total_delegated_stake_sol: 0.0,
            identity_linked_stake_sol: 0.0,
            authorities: Vec::new(),
        });
        flow.total_delegated_stake_sol += account.delegated_stake_sol;

        let staker = account.staker.as_deref();
        let withdrawer = account.withdrawer.as_deref();
        if let Some(authority) = staker {
            flow.authorities.push(authority.to_string());
        }
        if let Some(authority) = withdrawer {
            flow.authorities.push(authority.to_string());
        }

        if let Some(identity) = latest_identity_by_vote.get(vote) {
            if staker == Some(*identity) || withdrawer == Some(*identity) {
                flow.identity_linked_stake_sol += account.delegated_stake_sol;
            }
        }
    }

    by_vote
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::StakeAccountRecord;

    fn snap(
        vote: &str,
        identity: &str,
        epoch: u64,
        stake: f64,
        commission: u8,
    ) -> ValidatorSnapshot {
        ValidatorSnapshot {
            vote_pubkey: vote.to_string(),
            identity: identity.to_string(),
            epoch,
            slot_captured: 0,
            activated_stake_sol: stake,
            commission_pct: commission,
            vote_credits_epoch: 100,
            vote_credits_prior_epoch: 80,
            skip_rate: 0.02,
            delinquent: false,
            version: None,
            dc_location: None,
            superminority_member: false,
        }
    }

    fn record(vote: &str, staker: &str, withdrawer: &str, stake: f64) -> StakeAccountRecord {
        StakeAccountRecord {
            stake_pubkey: format!("stake-{vote}-{staker}"),
            delegated_vote_pubkey: Some(vote.to_string()),
            staker: Some(staker.to_string()),
            withdrawer: Some(withdrawer.to_string()),
            delegated_stake_sol: stake,
            activation_epoch: Some(1),
            deactivation_epoch: None,
            state: Some("delegated".to_string()),
        }
    }

    #[test]
    fn detects_self_stake_inflation_with_authority_links() {
        let mut histories = HashMap::new();
        histories.insert(
            "vote-a".to_string(),
            vec![
                snap("vote-a", "identity-a", 1, 10_000.0, 0),
                snap("vote-a", "identity-a", 2, 11_000.0, 0),
                snap("vote-a", "identity-a", 3, 12_500.0, 0),
                snap("vote-a", "identity-a", 4, 13_200.0, 0),
            ],
        );
        let stake_accounts = vec![
            record("vote-a", "identity-a", "identity-a", 5_000.0),
            record("vote-a", "identity-a", "identity-a", 1_500.0),
            record("vote-a", "other", "other", 3_000.0),
        ];
        let signals = detect_gaming_signals(&histories, &stake_accounts, 0.6);
        assert!(signals
            .iter()
            .any(|signal| matches!(signal.signal_type, GamingType::SelfStakeInflation)));
    }

    #[test]
    fn detects_sybil_cluster_from_shared_authority() {
        let mut histories = HashMap::new();
        histories.insert(
            "vote-1".to_string(),
            vec![
                snap("vote-1", "id-1", 1, 4_000.0, 5),
                snap("vote-1", "id-1", 2, 4_050.0, 5),
                snap("vote-1", "id-1", 3, 4_100.0, 5),
                snap("vote-1", "id-1", 4, 4_120.0, 5),
            ],
        );
        histories.insert(
            "vote-2".to_string(),
            vec![
                snap("vote-2", "id-2", 1, 5_000.0, 5),
                snap("vote-2", "id-2", 2, 5_010.0, 5),
                snap("vote-2", "id-2", 3, 5_020.0, 5),
                snap("vote-2", "id-2", 4, 5_030.0, 5),
            ],
        );
        histories.insert(
            "vote-3".to_string(),
            vec![
                snap("vote-3", "id-3", 1, 6_000.0, 5),
                snap("vote-3", "id-3", 2, 6_010.0, 5),
                snap("vote-3", "id-3", 3, 6_020.0, 5),
                snap("vote-3", "id-3", 4, 6_030.0, 5),
            ],
        );
        let stake_accounts = vec![
            record("vote-1", "shared-authority", "shared-authority", 4_500.0),
            record("vote-2", "shared-authority", "shared-authority", 5_500.0),
            record("vote-3", "shared-authority", "shared-authority", 6_500.0),
        ];
        let signals = detect_gaming_signals(&histories, &stake_accounts, 0.6);
        assert!(signals
            .iter()
            .any(|signal| matches!(signal.signal_type, GamingType::SybilCluster)));
    }
}
