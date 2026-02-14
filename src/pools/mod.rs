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
use std::collections::HashMap;

pub async fn fetch_pool_scores(pool: StakePoolId, client: &Client) -> Result<Vec<PoolScore>> {
    match pool {
        StakePoolId::Marinade => marinade::fetch_scores(client).await,
        StakePoolId::JPool => jpool::fetch_scores(client).await,
        StakePoolId::BlazeStake => blazestake::fetch_scores(client).await,
        StakePoolId::Jito => jito::fetch_scores(client).await,
        StakePoolId::Sanctum => sanctum::fetch_scores(client).await,
    }
}

pub(super) async fn fetch_json(client: &Client, url: &str) -> Result<Value> {
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

#[derive(Debug, Clone, Copy)]
pub(super) struct ScoreWeights {
    pub raw: f64,
    pub performance: f64,
    pub commission: f64,
    pub decentralization: f64,
    pub stake_balance: f64,
}

impl ScoreWeights {
    pub(super) const fn new(
        raw: f64,
        performance: f64,
        commission: f64,
        decentralization: f64,
        stake_balance: f64,
    ) -> Self {
        Self {
            raw,
            performance,
            commission,
            decentralization,
            stake_balance,
        }
    }
}

pub(super) fn normalize_scores(
    value: &Value,
    source: &str,
    weights: ScoreWeights,
) -> Vec<PoolScore> {
    let mut out = Vec::new();
    collect_scores_recursive(value, source, weights, &mut out);
    out
}

pub(super) fn merge_scores(entries: Vec<PoolScore>) -> Vec<PoolScore> {
    let mut merged: HashMap<String, PoolScore> = HashMap::new();
    for entry in entries {
        merged
            .entry(entry.vote_pubkey.clone())
            .and_modify(|existing| merge_score_entry(existing, &entry))
            .or_insert(entry);
    }
    let mut out = merged.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

fn merge_score_entry(existing: &mut PoolScore, incoming: &PoolScore) {
    if incoming.score > existing.score {
        existing.score = incoming.score;
    }
    if incoming.raw_score.unwrap_or(0.0) > existing.raw_score.unwrap_or(0.0) {
        existing.raw_score = incoming.raw_score;
    }
    if incoming.projected_delegation_sol > existing.projected_delegation_sol {
        existing.projected_delegation_sol = incoming.projected_delegation_sol;
    }
    if incoming.delegated_stake_sol > existing.delegated_stake_sol {
        existing.delegated_stake_sol = incoming.delegated_stake_sol;
    }
    if existing.commission_pct.is_none() {
        existing.commission_pct = incoming.commission_pct;
    }
    if existing.performance_score.is_none() {
        existing.performance_score = incoming.performance_score;
    }
    if existing.decentralization_score.is_none() {
        existing.decentralization_score = incoming.decentralization_score;
    }
    existing.eligible = existing.eligible && incoming.eligible;
    if !existing.source.contains(&incoming.source) {
        existing.source = format!("{},{}", existing.source, incoming.source);
    }
}

fn collect_scores_recursive(
    value: &Value,
    source: &str,
    weights: ScoreWeights,
    out: &mut Vec<PoolScore>,
) {
    match value {
        Value::Array(items) => {
            for item in items {
                if let Some(score) = parse_validator_score(item, source, weights) {
                    out.push(score);
                } else {
                    collect_scores_recursive(item, source, weights, out);
                }
            }
        }
        Value::Object(map) => {
            if let Some(score) = parse_validator_score(value, source, weights) {
                out.push(score);
                return;
            }
            for nested in map.values() {
                collect_scores_recursive(nested, source, weights, out);
            }
        }
        _ => {}
    }
}

fn parse_validator_score(value: &Value, source: &str, weights: ScoreWeights) -> Option<PoolScore> {
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
            "voteAccount",
            "validator_vote",
        ],
    )?;
    let raw_score = pick_f64(
        obj,
        &[
            "score",
            "validatorScore",
            "rankingScore",
            "totalScore",
            "marinadeScore",
            "compositeScore",
        ],
    );
    let projected_delegation_sol = pick_f64(
        obj,
        &[
            "projectedDelegationSol",
            "delegatedStakeSol",
            "delegatedStake",
            "estimatedDelegationSol",
        ],
    )
    .unwrap_or(0.0);
    let delegated_stake_sol = pick_f64(
        obj,
        &[
            "stakeSol",
            "delegatedStakeSol",
            "delegatedStake",
            "activatedStake",
            "stake",
        ],
    )
    .unwrap_or(0.0);
    let commission_pct = pick_f64(obj, &["commission", "commissionPct", "validatorCommission"]);
    let performance_score =
        pick_f64(obj, &["performance", "performanceScore", "uptime", "apy", "yield"]);
    let decentralization_score = pick_f64(
        obj,
        &[
            "decentralization",
            "decentralizationScore",
            "infraDiversityScore",
            "asnDiversityScore",
        ],
    );
    let eligible = pick_bool(obj, &["eligible", "isEligible", "active"]).unwrap_or(true);

    let score = compute_weighted_score(
        raw_score,
        performance_score,
        commission_pct,
        decentralization_score,
        delegated_stake_sol,
        weights,
    );

    Some(PoolScore {
        vote_pubkey,
        score,
        raw_score,
        delegated_stake_sol,
        projected_delegation_sol,
        commission_pct,
        performance_score,
        decentralization_score,
        eligible,
        source: source.to_string(),
    })
}

fn compute_weighted_score(
    raw_score: Option<f64>,
    performance_score: Option<f64>,
    commission_pct: Option<f64>,
    decentralization_score: Option<f64>,
    delegated_stake_sol: f64,
    weights: ScoreWeights,
) -> f64 {
    let mut total_weight = 0.0;
    let mut score = 0.0;

    if let Some(raw) = raw_score {
        let normalized = normalize_ratio(raw);
        score += normalized * weights.raw;
        total_weight += weights.raw;
    }
    if let Some(perf) = performance_score {
        let normalized = normalize_ratio(perf);
        score += normalized * weights.performance;
        total_weight += weights.performance;
    }
    if let Some(commission) = commission_pct {
        let normalized = (1.0 - (commission / 100.0)).clamp(0.0, 1.0);
        score += normalized * weights.commission;
        total_weight += weights.commission;
    }
    if let Some(decent) = decentralization_score {
        let normalized = normalize_ratio(decent);
        score += normalized * weights.decentralization;
        total_weight += weights.decentralization;
    }

    if delegated_stake_sol > 0.0 {
        let concentration_penalty = 1.0 / (1.0 + (delegated_stake_sol / 200_000.0));
        score += concentration_penalty * weights.stake_balance;
        total_weight += weights.stake_balance;
    }

    if total_weight > 0.0 {
        score / total_weight
    } else {
        0.0
    }
}

fn normalize_ratio(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    if value <= 0.0 {
        return 0.0;
    }
    if value <= 1.0 {
        return value;
    }
    (value / 100.0).clamp(0.0, 1.0)
}

fn pick_string(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| map.get(*key))
        .and_then(|value| value.as_str().map(ToString::to_string))
}

fn pick_f64(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| map.get(*key)).and_then(as_f64)
}

fn pick_bool(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| map.get(*key)).and_then(as_bool)
}

fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(num) => num.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::String(text) => match text.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        Value::Number(number) => number.as_i64().map(|v| v > 0),
        _ => None,
    }
}
