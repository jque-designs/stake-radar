use crate::models::{PoolQueuePosition, QueueDirection, StakePoolId};

#[derive(Debug, Clone)]
pub struct PoolScore {
    pub vote_pubkey: String,
    pub score: f64,
    pub projected_delegation_sol: f64,
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

    let mut sorted = scores.to_vec();
    sorted.sort_by(|a, b| b.score.total_cmp(&a.score));

    let idx = sorted
        .iter()
        .position(|entry| entry.vote_pubkey == your_vote_pubkey)?;
    let rank = (idx + 1) as u32;
    let score = sorted[idx].score;
    let estimated = sorted[idx].projected_delegation_sol;

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
