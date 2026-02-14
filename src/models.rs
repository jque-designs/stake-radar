use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// A point-in-time snapshot of a single validator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSnapshot {
    pub vote_pubkey: String,
    pub identity: String,
    pub epoch: u64,
    pub slot_captured: u64,
    pub activated_stake_sol: f64,
    pub commission_pct: u8,
    pub vote_credits_epoch: u64,
    pub vote_credits_prior_epoch: u64,
    pub skip_rate: f64,
    pub delinquent: bool,
    pub version: Option<String>,
    pub dc_location: Option<String>,
    pub superminority_member: bool,
}

impl ValidatorSnapshot {
    pub fn stake_tier_bucket(&self) -> StakeTierBucket {
        match self.activated_stake_sol {
            x if x > 1_000_000.0 => StakeTierBucket::Whale,
            x if x > 200_000.0 => StakeTierBucket::Large,
            x if x > 50_000.0 => StakeTierBucket::Mid,
            x if x > 10_000.0 => StakeTierBucket::Small,
            _ => StakeTierBucket::Micro,
        }
    }

    pub fn commission_bucket(&self) -> CommissionBucket {
        match self.commission_pct {
            0 => CommissionBucket::Zero,
            1..=5 => CommissionBucket::Low,
            6..=10 => CommissionBucket::Standard,
            _ => CommissionBucket::High,
        }
    }
}

/// Delta between two epoch snapshots for one validator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorDelta {
    pub vote_pubkey: String,
    pub epoch_from: u64,
    pub epoch_to: u64,
    pub stake_change_sol: f64,
    pub stake_change_pct: f64,
    pub commission_change: i8,
    pub vote_credit_delta: i64,
    pub skip_rate_delta: f64,
}

/// Threat assessment for a specific validator relative to you
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatProfile {
    pub vote_pubkey: String,
    pub current_stake_sol: f64,
    pub velocity_sol_per_epoch: f64,
    pub acceleration: f64,
    pub epochs_to_overtake: Option<u32>,
    pub threat_tier: ThreatTier,
    pub primary_stake_source: StakeSource,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ThreatTier {
    Critical,
    Rising,
    Watching,
    Neutral,
}

impl ThreatTier {
    pub fn severity_rank(&self) -> u8 {
        match self {
            ThreatTier::Critical => 0,
            ThreatTier::Rising => 1,
            ThreatTier::Watching => 2,
            ThreatTier::Neutral => 3,
        }
    }
}

impl fmt::Display for ThreatTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            ThreatTier::Critical => "critical",
            ThreatTier::Rising => "rising",
            ThreatTier::Watching => "watching",
            ThreatTier::Neutral => "neutral",
        };
        write!(f, "{text}")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum StakeSource {
    NativeStake,
    MarinadePool,
    JPool,
    BlazeStake,
    JitoPool,
    Sanctum,
    Mixed,
    Unknown,
}

/// An opportunity — a validator bleeding stake
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayOpportunity {
    pub vote_pubkey: String,
    pub stake_lost_sol: f64,
    pub stake_lost_pct: f64,
    pub loss_duration_epochs: u32,
    pub probable_cause: DecayCause,
    pub estimated_orphan_stake_sol: f64,
    pub capture_difficulty: CaptureDifficulty,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DecayCause {
    HighSkipRate,
    CommissionIncrease,
    Delinquency,
    VersionLag,
    DatacenterConcentration,
    StakePoolDelisting,
    Unknown,
}

impl fmt::Display for DecayCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            DecayCause::HighSkipRate => "high-skip-rate",
            DecayCause::CommissionIncrease => "commission-increase",
            DecayCause::Delinquency => "delinquency",
            DecayCause::VersionLag => "version-lag",
            DecayCause::DatacenterConcentration => "datacenter-concentration",
            DecayCause::StakePoolDelisting => "stake-pool-delisting",
            DecayCause::Unknown => "unknown",
        };
        write!(f, "{text}")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CaptureDifficulty {
    Easy,
    Medium,
    Hard,
}

impl fmt::Display for CaptureDifficulty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            CaptureDifficulty::Easy => "easy",
            CaptureDifficulty::Medium => "medium",
            CaptureDifficulty::Hard => "hard",
        };
        write!(f, "{text}")
    }
}

/// Inferred queue position within a stake pool's delegation algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolQueuePosition {
    pub pool: StakePoolId,
    pub your_score: f64,
    pub rank: u32,
    pub total_eligible: u32,
    pub direction: QueueDirection,
    pub score_gap_to_next: f64,
    pub estimated_delegation_sol: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum QueueDirection {
    Rising,
    Stable,
    Falling,
}

impl fmt::Display for QueueDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            QueueDirection::Rising => "rising",
            QueueDirection::Stable => "stable",
            QueueDirection::Falling => "falling",
        };
        write!(f, "{text}")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum StakePoolId {
    Marinade,
    JPool,
    BlazeStake,
    Jito,
    Sanctum,
}

impl fmt::Display for StakePoolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            StakePoolId::Marinade => "marinade",
            StakePoolId::JPool => "jpool",
            StakePoolId::BlazeStake => "blazestake",
            StakePoolId::Jito => "jito",
            StakePoolId::Sanctum => "sanctum",
        };
        write!(f, "{text}")
    }
}

impl std::str::FromStr for StakePoolId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "marinade" => Ok(StakePoolId::Marinade),
            "jpool" => Ok(StakePoolId::JPool),
            "blaze" | "blazestake" => Ok(StakePoolId::BlazeStake),
            "jito" => Ok(StakePoolId::Jito),
            "sanctum" => Ok(StakePoolId::Sanctum),
            _ => Err(format!("unsupported stake pool: {s}")),
        }
    }
}

/// Validator cohort for cross-group flow analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cohort {
    pub id: CohortId,
    pub label: String,
    pub member_count: u32,
    pub total_stake_sol: f64,
    pub avg_commission: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CohortId {
    StakeTier(StakeTierBucket),
    Region(String),
    Commission(CommissionBucket),
    Custom(String),
}

impl fmt::Display for CohortId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CohortId::StakeTier(bucket) => write!(f, "stake-tier:{bucket}"),
            CohortId::Region(region) => write!(f, "region:{region}"),
            CohortId::Commission(bucket) => write!(f, "commission:{bucket}"),
            CohortId::Custom(name) => write!(f, "custom:{name}"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum StakeTierBucket {
    Whale,
    Large,
    Mid,
    Small,
    Micro,
}

impl fmt::Display for StakeTierBucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            StakeTierBucket::Whale => "whale",
            StakeTierBucket::Large => "large",
            StakeTierBucket::Mid => "mid",
            StakeTierBucket::Small => "small",
            StakeTierBucket::Micro => "micro",
        };
        write!(f, "{text}")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CommissionBucket {
    Zero,
    Low,
    Standard,
    High,
}

impl fmt::Display for CommissionBucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            CommissionBucket::Zero => "zero",
            CommissionBucket::Low => "low",
            CommissionBucket::Standard => "standard",
            CommissionBucket::High => "high",
        };
        write!(f, "{text}")
    }
}

/// Directional stake flow between two cohorts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CohortFlow {
    pub from_cohort: CohortId,
    pub to_cohort: CohortId,
    pub flow_sol: f64,
    pub epoch_range: (u64, u64),
    pub delegator_count: u32,
}

/// Adversarial gaming signal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GamingSignal {
    pub vote_pubkey: String,
    pub signal_type: GamingType,
    pub confidence: f64,
    pub evidence: Vec<String>,
    pub first_detected_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GamingType {
    SelfStakeInflation,
    CommissionSniping,
    VoteCreditManipulation,
    SybilCluster,
    StakePoolCriteriaTuning,
}

impl fmt::Display for GamingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            GamingType::SelfStakeInflation => "self-stake-inflation",
            GamingType::CommissionSniping => "commission-sniping",
            GamingType::VoteCreditManipulation => "vote-credit-manipulation",
            GamingType::SybilCluster => "sybil-cluster",
            GamingType::StakePoolCriteriaTuning => "pool-criteria-tuning",
        };
        write!(f, "{text}")
    }
}

/// Alert fired by the engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub severity: AlertSeverity,
    pub category: AlertCategory,
    pub title: String,
    pub body: String,
    pub related_validator: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertSeverity {
    Critical,
    Warning,
    Info,
}

impl fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            AlertSeverity::Critical => "critical",
            AlertSeverity::Warning => "warning",
            AlertSeverity::Info => "info",
        };
        write!(f, "{text}")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertCategory {
    ThreatApproaching,
    OpportunityWindow,
    QueuePositionDrop,
    GamingDetected,
    CohortShift,
}

impl fmt::Display for AlertCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            AlertCategory::ThreatApproaching => "threat-approaching",
            AlertCategory::OpportunityWindow => "opportunity-window",
            AlertCategory::QueuePositionDrop => "queue-position-drop",
            AlertCategory::GamingDetected => "gaming-detected",
            AlertCategory::CohortShift => "cohort-shift",
        };
        write!(f, "{text}")
    }
}

pub fn short_pubkey(pubkey: &str) -> String {
    if pubkey.len() <= 10 {
        return pubkey.to_string();
    }
    format!("{}..{}", &pubkey[..4], &pubkey[pubkey.len() - 4..])
}
