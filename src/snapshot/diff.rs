use crate::models::{ValidatorDelta, ValidatorSnapshot};
use crate::rpc::StakeAccountRecord;
use std::collections::HashMap;

pub fn compute_deltas(
    from_epoch_snapshots: &[ValidatorSnapshot],
    to_epoch_snapshots: &[ValidatorSnapshot],
) -> Vec<ValidatorDelta> {
    if from_epoch_snapshots.is_empty() || to_epoch_snapshots.is_empty() {
        return Vec::new();
    }

    let from_epoch = from_epoch_snapshots[0].epoch;
    let to_epoch = to_epoch_snapshots[0].epoch;

    let from_index: HashMap<&str, &ValidatorSnapshot> = from_epoch_snapshots
        .iter()
        .map(|snapshot| (snapshot.vote_pubkey.as_str(), snapshot))
        .collect();

    let mut deltas = Vec::new();
    for current in to_epoch_snapshots {
        let Some(previous) = from_index.get(current.vote_pubkey.as_str()) else {
            continue;
        };

        let stake_change = current.activated_stake_sol - previous.activated_stake_sol;
        let stake_change_pct = if previous.activated_stake_sol.abs() < f64::EPSILON {
            0.0
        } else {
            stake_change / previous.activated_stake_sol
        };
        deltas.push(ValidatorDelta {
            vote_pubkey: current.vote_pubkey.clone(),
            epoch_from: from_epoch,
            epoch_to: to_epoch,
            stake_change_sol: stake_change,
            stake_change_pct,
            commission_change: current.commission_pct as i8 - previous.commission_pct as i8,
            vote_credit_delta: current.vote_credits_epoch as i64
                - previous.vote_credits_epoch as i64,
            skip_rate_delta: current.skip_rate - previous.skip_rate,
        });
    }

    deltas.sort_by(|a, b| {
        b.stake_change_sol
            .abs()
            .total_cmp(&a.stake_change_sol.abs())
    });
    deltas
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StakeFlowType {
    Reallocated,
    EnteredSet,
    ExitedSet,
    Increased,
    Decreased,
}

#[derive(Debug, Clone)]
pub struct StakeFlowDiff {
    pub stake_pubkey: String,
    pub epoch_from: u64,
    pub epoch_to: u64,
    pub from_vote_pubkey: Option<String>,
    pub to_vote_pubkey: Option<String>,
    pub staker: Option<String>,
    pub withdrawer: Option<String>,
    pub stake_sol: f64,
    pub flow_type: StakeFlowType,
    pub authority_changed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ValidatorFlowPressure {
    pub outbound_sol: f64,
    pub inbound_sol: f64,
    pub reallocated_outbound_sol: f64,
    pub entered_sol: f64,
    pub exited_sol: f64,
}

impl ValidatorFlowPressure {
    pub fn net_outbound_sol(&self) -> f64 {
        (self.outbound_sol - self.inbound_sol).max(0.0)
    }
}

pub fn compute_stake_flow_diffs(
    epoch_from: u64,
    epoch_to: u64,
    from_accounts: &[StakeAccountRecord],
    to_accounts: &[StakeAccountRecord],
) -> Vec<StakeFlowDiff> {
    let from_index: HashMap<&str, &StakeAccountRecord> = from_accounts
        .iter()
        .map(|record| (record.stake_pubkey.as_str(), record))
        .collect();
    let to_index: HashMap<&str, &StakeAccountRecord> = to_accounts
        .iter()
        .map(|record| (record.stake_pubkey.as_str(), record))
        .collect();

    let mut keys = from_index.keys().copied().collect::<Vec<_>>();
    for key in to_index.keys().copied() {
        if !from_index.contains_key(key) {
            keys.push(key);
        }
    }

    let mut diffs = Vec::new();
    for stake_pubkey in keys {
        match (from_index.get(stake_pubkey), to_index.get(stake_pubkey)) {
            (Some(from_record), Some(to_record)) => {
                diffs.extend(diff_existing_stake_account(
                    epoch_from,
                    epoch_to,
                    from_record,
                    to_record,
                ));
            }
            (Some(from_record), None) => {
                if from_record.delegated_stake_sol > 0.0 {
                    diffs.push(StakeFlowDiff {
                        stake_pubkey: from_record.stake_pubkey.clone(),
                        epoch_from,
                        epoch_to,
                        from_vote_pubkey: from_record.delegated_vote_pubkey.clone(),
                        to_vote_pubkey: None,
                        staker: from_record.staker.clone(),
                        withdrawer: from_record.withdrawer.clone(),
                        stake_sol: from_record.delegated_stake_sol,
                        flow_type: StakeFlowType::ExitedSet,
                        authority_changed: false,
                    });
                }
            }
            (None, Some(to_record)) => {
                if to_record.delegated_stake_sol > 0.0 {
                    diffs.push(StakeFlowDiff {
                        stake_pubkey: to_record.stake_pubkey.clone(),
                        epoch_from,
                        epoch_to,
                        from_vote_pubkey: None,
                        to_vote_pubkey: to_record.delegated_vote_pubkey.clone(),
                        staker: to_record.staker.clone(),
                        withdrawer: to_record.withdrawer.clone(),
                        stake_sol: to_record.delegated_stake_sol,
                        flow_type: StakeFlowType::EnteredSet,
                        authority_changed: false,
                    });
                }
            }
            (None, None) => {}
        }
    }

    diffs.sort_by(|a, b| b.stake_sol.total_cmp(&a.stake_sol));
    diffs
}

pub fn summarize_flow_pressure(
    diffs: &[StakeFlowDiff],
) -> HashMap<String, ValidatorFlowPressure> {
    let mut pressure_by_vote = HashMap::<String, ValidatorFlowPressure>::new();

    for diff in diffs {
        match diff.flow_type {
            StakeFlowType::Reallocated => {
                if let Some(from_vote) = diff.from_vote_pubkey.as_deref() {
                    let pressure = pressure_by_vote.entry(from_vote.to_string()).or_default();
                    pressure.outbound_sol += diff.stake_sol;
                    pressure.reallocated_outbound_sol += diff.stake_sol;
                }
                if let Some(to_vote) = diff.to_vote_pubkey.as_deref() {
                    let pressure = pressure_by_vote.entry(to_vote.to_string()).or_default();
                    pressure.inbound_sol += diff.stake_sol;
                }
            }
            StakeFlowType::EnteredSet => {
                if let Some(to_vote) = diff.to_vote_pubkey.as_deref() {
                    let pressure = pressure_by_vote.entry(to_vote.to_string()).or_default();
                    pressure.inbound_sol += diff.stake_sol;
                    pressure.entered_sol += diff.stake_sol;
                }
            }
            StakeFlowType::ExitedSet => {
                if let Some(from_vote) = diff.from_vote_pubkey.as_deref() {
                    let pressure = pressure_by_vote.entry(from_vote.to_string()).or_default();
                    pressure.outbound_sol += diff.stake_sol;
                    pressure.exited_sol += diff.stake_sol;
                }
            }
            StakeFlowType::Increased => {
                if let Some(to_vote) = diff.to_vote_pubkey.as_deref() {
                    let pressure = pressure_by_vote.entry(to_vote.to_string()).or_default();
                    pressure.inbound_sol += diff.stake_sol;
                    pressure.entered_sol += diff.stake_sol;
                }
            }
            StakeFlowType::Decreased => {
                if let Some(from_vote) = diff.from_vote_pubkey.as_deref() {
                    let pressure = pressure_by_vote.entry(from_vote.to_string()).or_default();
                    pressure.outbound_sol += diff.stake_sol;
                    pressure.exited_sol += diff.stake_sol;
                }
            }
        }
    }

    pressure_by_vote
}

fn diff_existing_stake_account(
    epoch_from: u64,
    epoch_to: u64,
    from_record: &StakeAccountRecord,
    to_record: &StakeAccountRecord,
) -> Vec<StakeFlowDiff> {
    let from_vote = from_record.delegated_vote_pubkey.as_deref();
    let to_vote = to_record.delegated_vote_pubkey.as_deref();
    let from_stake = from_record.delegated_stake_sol.max(0.0);
    let to_stake = to_record.delegated_stake_sol.max(0.0);
    let authority_changed = from_record.staker != to_record.staker
        || from_record.withdrawer != to_record.withdrawer;

    let mut diffs = Vec::new();
    if from_vote == to_vote {
        let delta = to_stake - from_stake;
        let magnitude = delta.abs();
        if magnitude > f64::EPSILON {
            let flow_type = if delta > 0.0 {
                StakeFlowType::Increased
            } else {
                StakeFlowType::Decreased
            };
            diffs.push(StakeFlowDiff {
                stake_pubkey: to_record.stake_pubkey.clone(),
                epoch_from,
                epoch_to,
                from_vote_pubkey: from_record.delegated_vote_pubkey.clone(),
                to_vote_pubkey: to_record.delegated_vote_pubkey.clone(),
                staker: to_record.staker.clone(),
                withdrawer: to_record.withdrawer.clone(),
                stake_sol: magnitude,
                flow_type,
                authority_changed,
            });
        }
        return diffs;
    }

    let reallocated = from_stake.min(to_stake);
    if reallocated > f64::EPSILON {
        diffs.push(StakeFlowDiff {
            stake_pubkey: to_record.stake_pubkey.clone(),
            epoch_from,
            epoch_to,
            from_vote_pubkey: from_record.delegated_vote_pubkey.clone(),
            to_vote_pubkey: to_record.delegated_vote_pubkey.clone(),
            staker: to_record.staker.clone(),
            withdrawer: to_record.withdrawer.clone(),
            stake_sol: reallocated,
            flow_type: StakeFlowType::Reallocated,
            authority_changed,
        });
    }

    if from_stake > to_stake {
        let exited = from_stake - to_stake;
        if exited > f64::EPSILON {
            diffs.push(StakeFlowDiff {
                stake_pubkey: from_record.stake_pubkey.clone(),
                epoch_from,
                epoch_to,
                from_vote_pubkey: from_record.delegated_vote_pubkey.clone(),
                to_vote_pubkey: None,
                staker: from_record.staker.clone(),
                withdrawer: from_record.withdrawer.clone(),
                stake_sol: exited,
                flow_type: StakeFlowType::ExitedSet,
                authority_changed,
            });
        }
    } else if to_stake > from_stake {
        let entered = to_stake - from_stake;
        if entered > f64::EPSILON {
            diffs.push(StakeFlowDiff {
                stake_pubkey: to_record.stake_pubkey.clone(),
                epoch_from,
                epoch_to,
                from_vote_pubkey: None,
                to_vote_pubkey: to_record.delegated_vote_pubkey.clone(),
                staker: to_record.staker.clone(),
                withdrawer: to_record.withdrawer.clone(),
                stake_sol: entered,
                flow_type: StakeFlowType::EnteredSet,
                authority_changed,
            });
        }
    }

    diffs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(stake_pubkey: &str, vote: Option<&str>, stake_sol: f64) -> StakeAccountRecord {
        StakeAccountRecord {
            stake_pubkey: stake_pubkey.to_string(),
            delegated_vote_pubkey: vote.map(ToString::to_string),
            staker: Some("staker".to_string()),
            withdrawer: Some("withdrawer".to_string()),
            delegated_stake_sol: stake_sol,
            activation_epoch: Some(1),
            deactivation_epoch: None,
            state: Some("delegated".to_string()),
        }
    }

    #[test]
    fn computes_reallocation_and_residual_flows() {
        let from = vec![record("stake-1", Some("vote-a"), 120.0)];
        let to = vec![record("stake-1", Some("vote-b"), 100.0)];
        let diffs = compute_stake_flow_diffs(10, 11, &from, &to);
        assert_eq!(diffs.len(), 2);
        assert!(diffs
            .iter()
            .any(|diff| diff.flow_type == StakeFlowType::Reallocated && diff.stake_sol == 100.0));
        assert!(diffs
            .iter()
            .any(|diff| diff.flow_type == StakeFlowType::ExitedSet && diff.stake_sol == 20.0));
    }

    #[test]
    fn summarizes_flow_pressure_by_validator() {
        let from = vec![
            record("stake-a", Some("vote-x"), 80.0),
            record("stake-b", Some("vote-y"), 40.0),
        ];
        let to = vec![
            record("stake-a", Some("vote-z"), 80.0),
            record("stake-b", Some("vote-y"), 30.0),
        ];
        let diffs = compute_stake_flow_diffs(20, 21, &from, &to);
        let pressure = summarize_flow_pressure(&diffs);
        let x = pressure.get("vote-x").expect("vote-x must be present");
        let y = pressure.get("vote-y").expect("vote-y must be present");
        let z = pressure.get("vote-z").expect("vote-z must be present");
        assert_eq!(x.outbound_sol, 80.0);
        assert_eq!(z.inbound_sol, 80.0);
        assert_eq!(y.outbound_sol, 10.0);
    }
}
