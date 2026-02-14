use crate::analysis::queue::PoolScore;
use anyhow::Result;
use reqwest::Client;

pub async fn fetch_scores(client: &Client) -> Result<Vec<PoolScore>> {
    let url = "https://stake.solblaze.org/api/v1/cls_validators";
    let payload = super::fetch_json(client, url).await?;
    let scores = super::normalize_scores(
        &payload,
        "blazestake",
        super::ScoreWeights::new(0.35, 0.30, 0.20, 0.10, 0.05),
    );
    Ok(super::merge_scores(scores))
}
