use crate::models::ValidatorSnapshot;
use crate::rpc::RpcClient;
use anyhow::Result;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
struct VoteAccountsResponse {
    current: Vec<VoteAccount>,
    delinquent: Vec<VoteAccount>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VoteAccount {
    vote_pubkey: String,
    node_pubkey: String,
    #[serde(deserialize_with = "deserialize_u64_from_string_or_num")]
    activated_stake: u64,
    commission: u8,
    epoch_credits: Vec<[u64; 3]>,
}

pub async fn fetch_vote_accounts(
    client: &RpcClient,
    epoch: u64,
    slot_captured: u64,
) -> Result<Vec<ValidatorSnapshot>> {
    let params = json!([{
        "votePubkey": null,
        "commitment": "confirmed",
        "keepUnstakedDelinquents": true,
    }]);
    let response: VoteAccountsResponse = client.rpc_call("getVoteAccounts", params).await?;

    let mut snapshots = Vec::with_capacity(response.current.len() + response.delinquent.len());
    snapshots.extend(
        response
            .current
            .iter()
            .map(|acct| account_to_snapshot(acct, epoch, slot_captured, false)),
    );
    snapshots.extend(
        response
            .delinquent
            .iter()
            .map(|acct| account_to_snapshot(acct, epoch, slot_captured, true)),
    );

    Ok(snapshots)
}

fn account_to_snapshot(
    account: &VoteAccount,
    epoch: u64,
    slot_captured: u64,
    delinquent: bool,
) -> ValidatorSnapshot {
    let (credits, prior_credits) = latest_epoch_credits(&account.epoch_credits);
    let credits_delta = credits.saturating_sub(prior_credits);
    let skip_rate = if credits == 0 {
        0.0
    } else {
        (1.0 - (credits_delta as f64 / credits as f64)).clamp(0.0, 1.0)
    };

    ValidatorSnapshot {
        vote_pubkey: account.vote_pubkey.clone(),
        identity: account.node_pubkey.clone(),
        epoch,
        slot_captured,
        activated_stake_sol: lamports_to_sol(account.activated_stake),
        commission_pct: account.commission,
        vote_credits_epoch: credits,
        vote_credits_prior_epoch: prior_credits,
        skip_rate,
        delinquent,
        version: None,
        dc_location: None,
        superminority_member: false,
    }
}

fn latest_epoch_credits(credits: &[[u64; 3]]) -> (u64, u64) {
    credits
        .last()
        .map(|entry| (entry[1], entry[2]))
        .unwrap_or((0, 0))
}

fn lamports_to_sol(lamports: u64) -> f64 {
    lamports as f64 / 1_000_000_000.0
}

fn deserialize_u64_from_string_or_num<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Visitor;

    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = u64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a u64 or a string that parses to u64")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if value < 0 {
                return Err(E::custom("negative stake amount"));
            }
            Ok(value as u64)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            value
                .parse::<u64>()
                .map_err(|e| E::custom(format!("invalid u64 string `{value}`: {e}")))
        }
    }

    deserializer.deserialize_any(Visitor)
}
