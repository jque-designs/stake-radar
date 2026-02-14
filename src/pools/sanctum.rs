use crate::analysis::queue::PoolScore;
use anyhow::Result;
use reqwest::Client;

pub async fn fetch_scores(client: &Client) -> Result<Vec<PoolScore>> {
    let url = "https://sanctum-s-api.fly.dev/v1/validator/list";
    let payload = super::fetch_json(client, url).await?;
    let scores = super::normalize_scores(
        &payload,
        "sanctum",
        super::ScoreWeights::new(0.40, 0.25, 0.20, 0.10, 0.05),
    );
    Ok(super::merge_scores(scores))
}
