use crate::rpc::RpcClient;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakeAccountRecord {
    pub stake_pubkey: String,
    pub delegated_vote_pubkey: Option<String>,
    pub staker: Option<String>,
    pub withdrawer: Option<String>,
    pub delegated_stake_sol: f64,
    pub activation_epoch: Option<u64>,
    pub deactivation_epoch: Option<u64>,
    pub state: Option<String>,
}

pub async fn fetch_stake_accounts(client: &RpcClient) -> Result<Vec<StakeAccountRecord>> {
    let params = json!([
        "Stake11111111111111111111111111111111111111",
        {
            "commitment": "confirmed",
            "encoding": "jsonParsed",
            "filters": [{"dataSize": 200}]
        }
    ]);

    let entries: Vec<Value> = client.rpc_call("getProgramAccounts", params).await?;
    let mut records = Vec::new();
    for entry in entries {
        if let Some(record) = parse_stake_account_entry(&entry) {
            records.push(record);
        }
    }
    info!(records = records.len(), "fetched stake accounts");
    Ok(records)
}

fn parse_stake_account_entry(entry: &Value) -> Option<StakeAccountRecord> {
    let stake_pubkey = entry.get("pubkey")?.as_str()?.to_string();
    let parsed = entry
        .get("account")?
        .get("data")?
        .get("parsed")
        .unwrap_or(&Value::Null);
    let state = parsed
        .get("type")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let info = parsed.get("info").unwrap_or(&Value::Null);
    let meta = info.get("meta").unwrap_or(&Value::Null);
    let authorized = meta.get("authorized").unwrap_or(&Value::Null);

    let staker = authorized
        .get("staker")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let withdrawer = authorized
        .get("withdrawer")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    let delegation = info
        .get("stake")
        .and_then(|stake| stake.get("delegation"))
        .or_else(|| info.get("delegation"))
        .unwrap_or(&Value::Null);

    let delegated_vote_pubkey = delegation
        .get("voter")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let delegated_stake_sol = value_to_u64(delegation.get("stake")).map_or(0.0, lamports_to_sol);
    let activation_epoch = value_to_u64(delegation.get("activationEpoch"));
    let deactivation_epoch = value_to_u64(delegation.get("deactivationEpoch"));

    Some(StakeAccountRecord {
        stake_pubkey,
        delegated_vote_pubkey,
        staker,
        withdrawer,
        delegated_stake_sol,
        activation_epoch,
        deactivation_epoch,
        state,
    })
}

fn value_to_u64(value: Option<&Value>) -> Option<u64> {
    match value {
        Some(Value::Number(number)) => number.as_u64().or_else(|| {
            number
                .as_i64()
                .and_then(|v| if v >= 0 { Some(v as u64) } else { None })
        }),
        Some(Value::String(text)) => text.parse::<u64>().ok(),
        _ => None,
    }
}

fn lamports_to_sol(lamports: u64) -> f64 {
    lamports as f64 / 1_000_000_000.0
}
