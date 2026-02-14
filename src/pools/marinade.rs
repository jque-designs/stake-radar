use crate::analysis::queue::PoolScore;
use anyhow::Result;
use reqwest::Client;

pub async fn fetch_scores(client: &Client) -> Result<Vec<PoolScore>> {
    let validators_url = "https://validators-api.marinade.finance/validators";
    let scoring_url = "https://scoring.marinade.finance/v1/scores";

    let validators_json = super::fetch_json(client, validators_url).await?;
    let scoring_json = super::fetch_json(client, scoring_url).await?;

    let validators = super::normalize_scores(
        &validators_json,
        "marinade-validators",
        super::ScoreWeights::new(0.30, 0.30, 0.20, 0.15, 0.05),
    );
    let scoring = super::normalize_scores(
        &scoring_json,
        "marinade-scores",
        super::ScoreWeights::new(0.60, 0.20, 0.10, 0.10, 0.0),
    );
    Ok(super::merge_scores(
        validators.into_iter().chain(scoring).collect(),
    ))
}
