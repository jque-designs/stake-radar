use crate::analysis::queue::PoolScore;
use anyhow::Result;
use reqwest::Client;

pub async fn fetch_scores(client: &Client) -> Result<Vec<PoolScore>> {
    let url = "https://kobe.mainnet.jito.network/api/v1/validators";
    let payload = super::fetch_json(client, url).await?;
    let mut scores = super::normalize_scores(&payload);
    scores.sort_by(|a, b| b.score.total_cmp(&a.score));
    Ok(scores)
}
