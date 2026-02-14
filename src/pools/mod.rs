pub mod blazestake;
pub mod jito;
pub mod jpool;
pub mod marinade;
pub mod sanctum;

use crate::analysis::queue::PoolScore;
use crate::models::StakePoolId;
use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;

pub async fn fetch_pool_scores(pool: StakePoolId, client: &Client) -> Result<Vec<PoolScore>> {
    match pool {
        StakePoolId::Marinade => marinade::fetch_scores(client).await,
        StakePoolId::JPool => jpool::fetch_scores(client).await,
        StakePoolId::BlazeStake => blazestake::fetch_scores(client).await,
        StakePoolId::Jito => jito::fetch_scores(client).await,
        StakePoolId::Sanctum => sanctum::fetch_scores(client).await,
    }
}

async fn fetch_json(client: &Client, url: &str) -> Result<Value> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to query pool API {url}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .with_context(|| format!("failed reading response body for {url}"))?;
    if !status.is_success() {
        anyhow::bail!("pool API {url} failed with status {status}: {body}");
    }
    let json: Value =
        serde_json::from_str(&body).with_context(|| format!("invalid JSON payload from {url}"))?;
    Ok(json)
}

fn normalize_scores(value: &Value) -> Vec<PoolScore> {
    let mut out = Vec::new();
    collect_scores_recursive(value, &mut out);
    out
}

fn collect_scores_recursive(value: &Value, out: &mut Vec<PoolScore>) {
    match value {
        Value::Array(items) => {
            for item in items {
                if let Some(score) = parse_validator_score(item) {
                    out.push(score);
                } else {
                    collect_scores_recursive(item, out);
                }
            }
        }
        Value::Object(map) => {
            if let Some(score) = parse_validator_score(value) {
                out.push(score);
                return;
            }
            for nested in map.values() {
                collect_scores_recursive(nested, out);
            }
        }
        _ => {}
    }
}

fn parse_validator_score(value: &Value) -> Option<PoolScore> {
    let obj = value.as_object()?;
    let vote_pubkey = pick_string(
        obj,
        &[
            "votePubkey",
            "vote_pubkey",
            "voteAddress",
            "vote_account",
            "vote",
            "validatorVoteAddress",
        ],
    )?;
    let score = pick_f64(
        obj,
        &["score", "validatorScore", "rankingScore", "totalScore"],
    )?;
    let projected_delegation_sol = pick_f64(
        obj,
        &[
            "projectedDelegationSol",
            "delegatedStakeSol",
            "delegatedStake",
        ],
    )
    .unwrap_or(0.0);
    Some(PoolScore {
        vote_pubkey,
        score,
        projected_delegation_sol,
    })
}

fn pick_string(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| map.get(*key))
        .and_then(|value| value.as_str().map(ToString::to_string))
}

fn pick_f64(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| map.get(*key)).and_then(as_f64)
}

fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(num) => num.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

pub(super) use fetch_json as shared_fetch_json;
pub(super) use normalize_scores as shared_normalize_scores;
