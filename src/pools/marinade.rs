use crate::analysis::queue::PoolScore;
use anyhow::Result;
use reqwest::Client;
use std::collections::HashMap;

pub async fn fetch_scores(client: &Client) -> Result<Vec<PoolScore>> {
    let validators_url = "https://validators-api.marinade.finance/validators";
    let scoring_url = "https://scoring.marinade.finance/v1/scores";

    let validators_json = super::shared_fetch_json(client, validators_url).await?;
    let scoring_json = super::shared_fetch_json(client, scoring_url).await?;

    let mut merged: HashMap<String, PoolScore> = HashMap::new();
    for score in super::shared_normalize_scores(&validators_json)
        .into_iter()
        .chain(super::shared_normalize_scores(&scoring_json))
    {
        merged
            .entry(score.vote_pubkey.clone())
            .and_modify(|existing| {
                if score.score > existing.score {
                    existing.score = score.score;
                }
                if score.projected_delegation_sol > 0.0 {
                    existing.projected_delegation_sol = score.projected_delegation_sol;
                }
            })
            .or_insert(score);
    }

    let mut scores: Vec<PoolScore> = merged.into_values().collect();
    scores.sort_by(|a, b| b.score.total_cmp(&a.score));
    Ok(scores)
}
