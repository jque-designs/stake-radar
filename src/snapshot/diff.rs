use crate::models::{ValidatorDelta, ValidatorSnapshot};
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
