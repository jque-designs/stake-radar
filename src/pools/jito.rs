use crate::analysis::queue::PoolScore;
use anyhow::Result;
use reqwest::Client;

pub async fn fetch_scores(client: &Client) -> Result<Vec<PoolScore>> {
    let url = "https://kobe.mainnet.jito.network/api/v1/validators";
    let payload = super::fetch_json(client, url).await?;
    let scores = super::normalize_scores(
        &payload,
        "jito",
        super::ScoreWeights::new(0.50, 0.20, 0.15, 0.10, 0.05),
    );
    Ok(super::merge_scores(scores))
}
