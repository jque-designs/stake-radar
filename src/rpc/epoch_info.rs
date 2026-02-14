use crate::rpc::RpcClient;
use anyhow::Result;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpochInfo {
    pub epoch: u64,
    pub slot_index: u64,
    pub slots_in_epoch: u64,
    pub absolute_slot: u64,
    pub block_height: Option<u64>,
    pub transaction_count: Option<u64>,
}

pub async fn fetch_epoch_info(client: &RpcClient) -> Result<EpochInfo> {
    client.rpc_call("getEpochInfo", json!([])).await
}
