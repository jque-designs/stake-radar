use crate::models::{PoolQueuePosition, QueueDirection, StakePoolId};

#[derive(Debug, Clone)]
pub struct PoolScore {
    pub vote_pubkey: String,
    pub score: f64,
    pub raw_score: Option<f64>,
    pub delegated_stake_sol: f64,
    pub projected_delegation_sol: f64,
    pub commission_pct: Option<f64>,
    pub performance_score: Option<f64>,
    pub decentralization_score: Option<f64>,
    pub eligible: bool,
    pub source: String,
}

pub fn infer_queue_position(
    your_vote_pubkey: &str,
    pool: StakePoolId,
    scores: &[PoolScore],
    prior_rank: Option<u32>,
) -> Option<PoolQueuePosition> {
    if scores.is_empty() {
        return None;
    }

    let mut sorted = scores
        .iter()
        .filter(|entry| entry.eligible)
        .cloned()
        .collect::<Vec<_>>();
    if sorted.is_empty() {
        sorted = scores.to_vec();
    }
    sorted.sort_by(|a, b| b.score.total_cmp(&a.score));

    let idx = sorted
        .iter()
        .position(|entry| entry.vote_pubkey == your_vote_pubkey)?;
    let rank = (idx + 1) as u32;
    let score = sorted[idx].score;
    let estimated = estimate_delegation(&sorted[idx], rank as usize, sorted.len());

    let score_gap_to_next = if idx == 0 {
        0.0
    } else {
        (sorted[idx - 1].score - sorted[idx].score).max(0.0)
    };

    let direction = match prior_rank {
        Some(previous) if rank < previous => QueueDirection::Rising,
        Some(previous) if rank > previous => QueueDirection::Falling,
        _ => QueueDirection::Stable,
    };

    Some(PoolQueuePosition {
        pool,
        your_score: score,
        rank,
        total_eligible: sorted.len() as u32,
        direction,
        score_gap_to_next,
        estimated_delegation_sol: estimated,
    })
}

pub fn rank_scores(scores: &[PoolScore]) -> Vec<PoolScore> {
    let mut ranked = scores.to_vec();
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
    ranked
}

fn estimate_delegation(entry: &PoolScore, rank: usize, total: usize) -> f64 {
    if entry.projected_delegation_sol > 0.0 {
        return entry.projected_delegation_sol;
    }
    if entry.delegated_stake_sol <= 0.0 {
        return 0.0;
    }

    let percentile = if total == 0 {
        0.0
    } else {
        1.0 - ((rank.saturating_sub(1)) as f64 / total as f64)
    };
    // Conservative fallback: higher-ranked validators likely receive larger next-cycle increments.
    let growth_factor = 0.01 + (0.04 * percentile.clamp(0.0, 1.0));
    entry.delegated_stake_sol * growth_factor
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_score(vote: &str, score: f64, eligible: bool) -> PoolScore {
        PoolScore {
            vote_pubkey: vote.to_string(),
            score,
            raw_score: Some(score),
            delegated_stake_sol: 10_000.0,
            projected_delegation_sol: 0.0,
            commission_pct: Some(5.0),
            performance_score: Some(0.9),
            decentralization_score: Some(0.8),
            eligible,
            source: "test".to_string(),
        }
    }

    #[test]
    fn prefers_eligible_set_for_ranking() {
        let scores = vec![
            make_score("a", 0.95, true),
            make_score("you", 0.80, true),
            make_score("b", 0.99, false),
        ];
        let position = infer_queue_position("you", StakePoolId::Marinade, &scores, Some(3))
            .expect("position must exist");
        assert_eq!(position.rank, 2);
        assert_eq!(position.total_eligible, 2);
        assert_eq!(position.direction, QueueDirection::Rising);
    }

    #[test]
    fn delegation_estimate_falls_back_when_projection_missing() {
        let scores = vec![make_score("you", 0.8, true)];
        let position = infer_queue_position("you", StakePoolId::JPool, &scores, None)
            .expect("position must exist");
        assert!(position.estimated_delegation_sol > 0.0);
    }
}
