pub mod epoch_info;
pub mod rate_limiter;
pub mod stake_accounts;
pub mod vote_accounts;

use crate::config::RpcConfig;
use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::debug;

pub use epoch_info::EpochInfo;
pub use stake_accounts::StakeAccountRecord;

#[derive(Clone)]
pub struct RpcClient {
    endpoint: String,
    http: Client,
    limiter: Arc<rate_limiter::RateLimiter>,
}

impl RpcClient {
    pub fn new(endpoint: String, requests_per_second: u32) -> Self {
        Self {
            endpoint,
            http: Client::new(),
            limiter: Arc::new(rate_limiter::RateLimiter::new(requests_per_second)),
        }
    }

    pub fn from_config(config: &RpcConfig) -> Self {
        Self::new(config.url.clone(), config.requests_per_second)
    }

    pub async fn get_epoch_info(&self) -> Result<EpochInfo> {
        epoch_info::fetch_epoch_info(self).await
    }

    pub async fn get_vote_snapshots(
        &self,
        epoch: u64,
        slot_captured: u64,
    ) -> Result<Vec<crate::models::ValidatorSnapshot>> {
        vote_accounts::fetch_vote_accounts(self, epoch, slot_captured).await
    }

    pub async fn get_stake_accounts(&self) -> Result<Vec<StakeAccountRecord>> {
        stake_accounts::fetch_stake_accounts(self).await
    }

    pub(crate) async fn rpc_call<T>(&self, method: &str, params: Value) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.limiter.acquire().await;
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        debug!(method, "sending RPC request");
        let response = self
            .http
            .post(&self.endpoint)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("RPC request failed for method {method}"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .with_context(|| format!("failed to read RPC response body for method {method}"))?;
        if !status.is_success() {
            self.limiter.on_error().await;
            return Err(anyhow!(
                "RPC HTTP failure for {method}: status={} body={}",
                status,
                body
            ));
        }

        let envelope: RpcEnvelope<T> =
            serde_json::from_str(&body).with_context(|| format!("invalid JSON for {method}"))?;
        match (envelope.result, envelope.error) {
            (Some(result), None) => {
                self.limiter.on_success().await;
                Ok(result)
            }
            (_, Some(err)) => {
                self.limiter.on_error().await;
                Err(anyhow!(
                    "RPC method {method} returned error {} ({})",
                    err.code,
                    err.message
                ))
            }
            (None, None) => {
                self.limiter.on_error().await;
                Err(anyhow!(
                    "RPC method {method} returned neither result nor error"
                ))
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct RpcEnvelope<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}
