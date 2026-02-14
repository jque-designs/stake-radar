use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub validator: ValidatorConfig,
    pub rpc: RpcConfig,
    pub snapshot: SnapshotConfig,
    pub analysis: AnalysisConfig,
    pub cohorts: CohortsConfig,
    pub alerts: AlertsConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            validator: ValidatorConfig::default(),
            rpc: RpcConfig::default(),
            snapshot: SnapshotConfig::default(),
            analysis: AnalysisConfig::default(),
            cohorts: CohortsConfig::default(),
            alerts: AlertsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidatorConfig {
    pub vote_pubkey: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    pub url: String,
    pub requests_per_second: u32,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            url: "https://api.mainnet-beta.solana.com".to_string(),
            requests_per_second: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    pub db_path: String,
    pub auto_snapshot: bool,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            db_path: "~/.local/share/stake-radar/snapshots.db".to_string(),
            auto_snapshot: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub lookback_epochs: u32,
    pub threat_overtake_horizon_epochs: u32,
    pub min_opportunity_stake_sol: f64,
    pub gaming_confidence_threshold: f64,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            lookback_epochs: 10,
            threat_overtake_horizon_epochs: 15,
            min_opportunity_stake_sol: 5000.0,
            gaming_confidence_threshold: 0.65,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CohortsConfig {
    #[serde(default)]
    pub custom: Vec<CustomCohort>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCohort {
    pub name: String,
    pub filter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertsConfig {
    #[serde(default)]
    pub discord_webhook: String,
    #[serde(default)]
    pub telegram_bot_token: String,
    #[serde(default)]
    pub telegram_chat_id: String,
    pub enable_stdout: bool,
    pub rules: AlertRulesConfig,
}

impl Default for AlertsConfig {
    fn default() -> Self {
        Self {
            discord_webhook: String::new(),
            telegram_bot_token: String::new(),
            telegram_chat_id: String::new(),
            enable_stdout: true,
            rules: AlertRulesConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRulesConfig {
    pub threat_critical: bool,
    pub opportunity_easy: bool,
    pub queue_drop: bool,
    pub gaming_detected: bool,
}

impl Default for AlertRulesConfig {
    fn default() -> Self {
        Self {
            threat_critical: true,
            opportunity_easy: true,
            queue_drop: true,
            gaming_detected: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConfigOverrides {
    pub vote_pubkey: Option<String>,
    pub rpc_url: Option<String>,
    pub lookback_epochs: Option<u32>,
}

impl AppConfig {
    pub fn default_path() -> PathBuf {
        match dirs::home_dir() {
            Some(home) => home.join(".config/stake-radar/config.toml"),
            None => PathBuf::from("./config.toml"),
        }
    }

    pub fn load(path: Option<PathBuf>) -> Result<(Self, PathBuf)> {
        let resolved_path = path.unwrap_or_else(Self::default_path);
        if !resolved_path.exists() {
            return Ok((Self::default(), resolved_path));
        }

        let raw = fs::read_to_string(&resolved_path)
            .with_context(|| format!("failed to read config {}", resolved_path.display()))?;
        let cfg: AppConfig = toml::from_str(&raw)
            .with_context(|| format!("failed to parse TOML config {}", resolved_path.display()))?;
        Ok((cfg, resolved_path))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }
        let encoded = toml::to_string_pretty(self).context("failed to serialize config to TOML")?;
        fs::write(path, encoded)
            .with_context(|| format!("failed to write config {}", path.display()))?;
        Ok(())
    }

    pub fn apply_overrides(&mut self, overrides: &ConfigOverrides) {
        if let Some(vote_pubkey) = &overrides.vote_pubkey {
            self.validator.vote_pubkey = Some(vote_pubkey.clone());
        }
        if let Some(rpc_url) = &overrides.rpc_url {
            self.rpc.url = rpc_url.clone();
        }
        if let Some(lookback) = overrides.lookback_epochs {
            self.analysis.lookback_epochs = lookback;
        }
    }

    pub fn snapshot_db_path(&self) -> PathBuf {
        expand_home(&self.snapshot.db_path)
    }
}

pub fn expand_home(raw: &str) -> PathBuf {
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(path_without_tilde) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(path_without_tilde);
        }
    }
    PathBuf::from(raw)
}
