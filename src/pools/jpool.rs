use crate::analysis::queue::PoolScore;
use anyhow::Result;
use reqwest::Client;

pub async fn fetch_scores(client: &Client) -> Result<Vec<PoolScore>> {
    let url = "https://api.jpool.one/validators";
    let payload = super::fetch_json(client, url).await?;
    let scores = super::normalize_scores(
        &payload,
        "jpool",
        super::ScoreWeights::new(0.45, 0.25, 0.20, 0.05, 0.05),
    );
    Ok(super::merge_scores(scores))
}
