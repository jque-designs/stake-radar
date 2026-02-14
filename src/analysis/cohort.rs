use crate::models::{
    Cohort, CohortFlow, CohortId, CommissionBucket, StakeTierBucket, ValidatorSnapshot,
};
use crate::snapshot::{StakeFlowDiff, StakeFlowType};
use std::collections::HashMap;

pub fn build_stake_tier_cohorts(snapshots: &[ValidatorSnapshot]) -> Vec<Cohort> {
    let mut buckets: HashMap<StakeTierBucket, Vec<&ValidatorSnapshot>> = HashMap::new();
    for snapshot in snapshots {
        buckets
            .entry(snapshot.stake_tier_bucket())
            .or_default()
            .push(snapshot);
    }

    let mut cohorts = Vec::new();
    for (bucket, members) in buckets {
        let total_stake: f64 = members.iter().map(|s| s.activated_stake_sol).sum();
        let avg_commission = if members.is_empty() {
            0.0
        } else {
            members.iter().map(|s| s.commission_pct as f64).sum::<f64>() / members.len() as f64
        };
        cohorts.push(Cohort {
            id: CohortId::StakeTier(bucket),
            label: bucket.to_string(),
            member_count: members.len() as u32,
            total_stake_sol: total_stake,
            avg_commission,
        });
    }

    cohorts.sort_by(|a, b| b.total_stake_sol.total_cmp(&a.total_stake_sol));
    cohorts
}

pub fn build_commission_cohorts(snapshots: &[ValidatorSnapshot]) -> Vec<Cohort> {
    let mut buckets: HashMap<CommissionBucket, Vec<&ValidatorSnapshot>> = HashMap::new();
    for snapshot in snapshots {
        buckets
            .entry(snapshot.commission_bucket())
            .or_default()
            .push(snapshot);
    }

    let mut cohorts = Vec::new();
    for (bucket, members) in buckets {
        let total_stake: f64 = members.iter().map(|s| s.activated_stake_sol).sum();
        let avg_commission = if members.is_empty() {
            0.0
        } else {
            members.iter().map(|s| s.commission_pct as f64).sum::<f64>() / members.len() as f64
        };
        cohorts.push(Cohort {
            id: CohortId::Commission(bucket),
            label: bucket.to_string(),
            member_count: members.len() as u32,
            total_stake_sol: total_stake,
            avg_commission,
        });
    }

    cohorts.sort_by(|a, b| b.total_stake_sol.total_cmp(&a.total_stake_sol));
    cohorts
}

pub fn compute_cohort_flows(
    from_epoch: u64,
    to_epoch: u64,
    from_snapshots: &[ValidatorSnapshot],
    to_snapshots: &[ValidatorSnapshot],
) -> Vec<CohortFlow> {
    let from_index: HashMap<&str, &ValidatorSnapshot> = from_snapshots
        .iter()
        .map(|snapshot| (snapshot.vote_pubkey.as_str(), snapshot))
        .collect();
    let to_index: HashMap<&str, &ValidatorSnapshot> = to_snapshots
        .iter()
        .map(|snapshot| (snapshot.vote_pubkey.as_str(), snapshot))
        .collect();

    let mut flow_map: HashMap<(CohortId, CohortId), f64> = HashMap::new();
    for (vote_pubkey, from_snapshot) in from_index {
        let Some(to_snapshot) = to_index.get(vote_pubkey) else {
            continue;
        };
        let from_cohort = CohortId::StakeTier(from_snapshot.stake_tier_bucket());
        let to_cohort = CohortId::StakeTier(to_snapshot.stake_tier_bucket());
        if from_cohort == to_cohort {
            continue;
        }

        let flow =
            ((from_snapshot.activated_stake_sol + to_snapshot.activated_stake_sol) / 2.0).max(0.0);
        *flow_map.entry((from_cohort, to_cohort)).or_insert(0.0) += flow;
    }

    let mut flows = flow_map
        .into_iter()
        .map(|((from_cohort, to_cohort), flow_sol)| CohortFlow {
            from_cohort,
            to_cohort,
            flow_sol,
            epoch_range: (from_epoch, to_epoch),
            delegator_count: 0,
        })
        .collect::<Vec<_>>();
    flows.sort_by(|a, b| b.flow_sol.total_cmp(&a.flow_sol));
    flows
}

pub fn compute_cohort_flows_from_stake_diffs(
    from_epoch: u64,
    to_epoch: u64,
    from_snapshots: &[ValidatorSnapshot],
    to_snapshots: &[ValidatorSnapshot],
    stake_diffs: &[StakeFlowDiff],
) -> Vec<CohortFlow> {
    let from_cohorts = vote_to_stake_cohort(from_snapshots);
    let to_cohorts = vote_to_stake_cohort(to_snapshots);
    let mut flow_map: HashMap<(CohortId, CohortId), (f64, u32)> = HashMap::new();

    for diff in stake_diffs {
        if diff.stake_sol <= 0.0 {
            continue;
        }

        let (from_vote, to_vote) = match diff.flow_type {
            StakeFlowType::Reallocated => (
                diff.from_vote_pubkey.as_deref(),
                diff.to_vote_pubkey.as_deref(),
            ),
            StakeFlowType::EnteredSet => (None, diff.to_vote_pubkey.as_deref()),
            StakeFlowType::ExitedSet => (diff.from_vote_pubkey.as_deref(), None),
            StakeFlowType::Increased => (None, diff.to_vote_pubkey.as_deref()),
            StakeFlowType::Decreased => (diff.from_vote_pubkey.as_deref(), None),
        };

        let from_cohort = from_vote
            .and_then(|vote| from_cohorts.get(vote))
            .cloned()
            .unwrap_or_else(|| CohortId::Custom("external".to_string()));
        let to_cohort = to_vote
            .and_then(|vote| to_cohorts.get(vote))
            .cloned()
            .unwrap_or_else(|| CohortId::Custom("external".to_string()));

        if from_cohort == to_cohort {
            continue;
        }

        let entry = flow_map.entry((from_cohort, to_cohort)).or_insert((0.0, 0));
        entry.0 += diff.stake_sol;
        entry.1 += 1;
    }

    let mut flows = flow_map
        .into_iter()
        .map(
            |((from_cohort, to_cohort), (flow_sol, delegator_count))| CohortFlow {
                from_cohort,
                to_cohort,
                flow_sol,
                epoch_range: (from_epoch, to_epoch),
                delegator_count,
            },
        )
        .collect::<Vec<_>>();
    flows.sort_by(|a, b| b.flow_sol.total_cmp(&a.flow_sol));
    flows
}

fn vote_to_stake_cohort(snapshots: &[ValidatorSnapshot]) -> HashMap<String, CohortId> {
    snapshots
        .iter()
        .map(|snapshot| {
            (
                snapshot.vote_pubkey.clone(),
                CohortId::StakeTier(snapshot.stake_tier_bucket()),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{StakeFlowDiff, StakeFlowType};

    fn snapshot(vote: &str, epoch: u64, stake: f64) -> ValidatorSnapshot {
        ValidatorSnapshot {
            vote_pubkey: vote.to_string(),
            identity: format!("id-{vote}"),
            epoch,
            slot_captured: 0,
            activated_stake_sol: stake,
            commission_pct: 5,
            vote_credits_epoch: 100,
            vote_credits_prior_epoch: 95,
            skip_rate: 0.01,
            delinquent: false,
            version: None,
            dc_location: None,
            superminority_member: false,
        }
    }

    #[test]
    fn builds_cohort_flows_from_stake_diffs() {
        let from_snapshots = vec![
            snapshot("vote-small", 100, 20_000.0),
            snapshot("vote-mid", 100, 80_000.0),
        ];
        let to_snapshots = vec![
            snapshot("vote-small", 101, 18_000.0),
            snapshot("vote-mid", 101, 85_000.0),
        ];
        let diffs = vec![StakeFlowDiff {
            stake_pubkey: "stake-a".to_string(),
            epoch_from: 100,
            epoch_to: 101,
            from_vote_pubkey: Some("vote-small".to_string()),
            to_vote_pubkey: Some("vote-mid".to_string()),
            staker: Some("staker".to_string()),
            withdrawer: Some("withdrawer".to_string()),
            stake_sol: 2_000.0,
            flow_type: StakeFlowType::Reallocated,
            authority_changed: false,
        }];

        let flows =
            compute_cohort_flows_from_stake_diffs(100, 101, &from_snapshots, &to_snapshots, &diffs);
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].flow_sol, 2_000.0);
        assert_eq!(flows[0].delegator_count, 1);
    }
}
