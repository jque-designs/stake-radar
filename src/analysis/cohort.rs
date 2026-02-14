use crate::models::{
    Cohort, CohortFlow, CohortId, CommissionBucket, StakeTierBucket, ValidatorSnapshot,
};
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
