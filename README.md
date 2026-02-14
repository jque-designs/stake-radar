# STAKE-RADAR: Competitive Intelligence Radar for Solana Validators

> **Status:** Spec v1.0 — Codex-ready
> **Crate:** `stake-radar`
> **Binary:** `stake-radar`
> **License:** MIT/Apache-2.0

## What This Is

A real-time competitive intelligence engine that treats the Solana validator set as a **contested market**. Stake-Radar doesn't just show you scores — it detects **threats**, surfaces **opportunities**, and exposes **gaming** before it costs you delegations.

No validator tool does this today. Existing dashboards show static snapshots. Stake-Radar models the validator landscape as a living battlefield with directional flows, cohort dynamics, and adversarial detection.

---

## Novel Differentiators

1. **Threat Velocity Scoring** — Not "who has more stake" but "who is accelerating toward your tier at a rate that will overtake you in N epochs."
2. **Decay Opportunity Funnel** — Validators losing stake aren't just declining; their delegators are *actively seeking new homes*. Stake-Radar identifies these orphaned-stake windows in real time.
3. **Stake Pool Queue Inference** — Reverse-engineers your position in Marinade/JPool/BlazeStake scoring by simulating their delegation algorithms against current on-chain state.
4. **Adversarial Gaming Detection** — Detects patterns consistent with self-stake inflation, commission sniping, vote-credit manipulation, and sybil validator clusters.
5. **Cross-Cohort Flow Analysis** — Tracks stake movement *between* validator archetypes (small independent → large institutional, high-commission → low-commission) revealing market-level trends before they hit individual validators.

---

## Architecture

```
stake-radar/
├── Cargo.toml
├── src/
│   ├── main.rs                  # CLI entrypoint
│   ├── lib.rs                   # Public API surface
│   ├── config.rs                # TOML config + CLI arg merge
│   ├── rpc/
│   │   ├── mod.rs
│   │   ├── vote_accounts.rs     # getVoteAccounts poller
│   │   ├── stake_accounts.rs    # getProgramAccounts for stake
│   │   ├── epoch_info.rs        # Epoch boundary detection
│   │   └── rate_limiter.rs      # Adaptive RPC throttle
│   ├── pools/
│   │   ├── mod.rs
│   │   ├── marinade.rs          # Marinade scoring + delegation sim
│   │   ├── jpool.rs             # JPool delegation queue model
│   │   ├── blazestake.rs        # BlazeStake criteria tracker
│   │   ├── jito.rs              # JitoSOL stake distribution
│   │   └── sanctum.rs           # Sanctum LST validator sets
│   ├── snapshot/
│   │   ├── mod.rs
│   │   ├── store.rs             # SQLite epoch-keyed snapshots
│   │   ├── diff.rs              # Epoch-over-epoch delta engine
│   │   └── migrations.rs        # Schema versioning
│   ├── analysis/
│   │   ├── mod.rs
│   │   ├── threat.rs            # Threat velocity computation
│   │   ├── opportunity.rs       # Decay funnel + orphan stake
│   │   ├── cohort.rs            # Validator clustering + flow
│   │   ├── queue.rs             # Stake pool queue inference
│   │   └── adversarial.rs       # Gaming/sybil detection
│   ├── alert/
│   │   ├── mod.rs
│   │   ├── engine.rs            # Rule evaluation loop
│   │   ├── rules.rs             # Alert rule definitions
│   │   └── sink.rs              # Webhook, Discord, stdout
│   └── output/
│       ├── mod.rs
│       ├── table.rs             # Terminal table rendering
│       ├── json.rs              # Structured JSON output
│       └── csv.rs               # CSV export
```

### Key Dependencies

```toml
[dependencies]
solana-client = "2.1"
solana-sdk = "2.1"
solana-account-decoder = "2.1"
borsh = "1.5"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
clap = { version = "4", features = ["derive"] }
comfy-table = "7"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```

---

## Data Types

### Core Domain Models

```rust
/// A point-in-time snapshot of a single validator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSnapshot {
    pub vote_pubkey: Pubkey,
    pub identity: Pubkey,
    pub epoch: u64,
    pub slot_captured: u64,
    pub activated_stake_sol: f64,
    pub commission_pct: u8,
    pub vote_credits_epoch: u64,
    pub vote_credits_prior_epoch: u64,
    pub skip_rate: f64,
    pub delinquent: bool,
    pub version: Option<String>,
    pub dc_location: Option<String>,           // ASN / datacenter identifier
    pub superminority_member: bool,
}

/// Delta between two epoch snapshots for one validator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorDelta {
    pub vote_pubkey: Pubkey,
    pub epoch_from: u64,
    pub epoch_to: u64,
    pub stake_change_sol: f64,
    pub stake_change_pct: f64,
    pub commission_change: i8,                 // signed: +1 means raised
    pub vote_credit_delta: i64,
    pub skip_rate_delta: f64,
}

/// Threat assessment for a specific validator relative to you
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatProfile {
    pub vote_pubkey: Pubkey,
    pub current_stake_sol: f64,
    pub velocity_sol_per_epoch: f64,           // linear regression slope
    pub acceleration: f64,                     // second derivative
    pub epochs_to_overtake: Option<u32>,       // None = diverging
    pub threat_tier: ThreatTier,
    pub primary_stake_source: StakeSource,     // where growth is coming from
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ThreatTier {
    Critical,   // overtakes within 3 epochs
    Rising,     // overtakes within 10 epochs
    Watching,   // positive velocity, same cohort
    Neutral,    // different cohort or diverging
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub vote_pubkey: Pubkey,
    pub stake_lost_sol: f64,
    pub stake_lost_pct: f64,
    pub loss_duration_epochs: u32,
    pub probable_cause: DecayCause,
    pub estimated_orphan_stake_sol: f64,       // stake likely seeking new home
    pub capture_difficulty: CaptureDifficulty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecayCause {
    HighSkipRate,
    CommissionIncrease,
    Delinquency,
    VersionLag,
    DatacenterConcentration,
    StakePoolDelisting,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CaptureDifficulty {
    Easy,    // native stake, no pool loyalty
    Medium,  // pool-managed, but pool is also exiting
    Hard,    // institutional, likely moving to known target
}

/// Inferred queue position within a stake pool's delegation algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolQueuePosition {
    pub pool: StakePoolId,
    pub your_score: f64,                       // reconstructed scoring
    pub rank: u32,                             // out of eligible set
    pub total_eligible: u32,
    pub direction: QueueDirection,
    pub score_gap_to_next: f64,                // how far to move up one rank
    pub estimated_delegation_sol: f64,         // projected next-epoch delegation
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum QueueDirection {
    Rising,
    Stable,
    Falling,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum StakePoolId {
    Marinade,
    JPool,
    BlazeStake,
    Jito,
    Sanctum,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum StakeTierBucket {
    Whale,       // >1M SOL
    Large,       // 200K–1M SOL
    Mid,         // 50K–200K SOL
    Small,       // 10K–50K SOL
    Micro,       // <10K SOL
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CommissionBucket {
    Zero,        // 0%
    Low,         // 1-5%
    Standard,    // 6-10%
    High,        // >10%
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
    pub vote_pubkey: Pubkey,
    pub signal_type: GamingType,
    pub confidence: f64,                       // 0.0–1.0
    pub evidence: Vec<String>,
    pub first_detected_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GamingType {
    SelfStakeInflation,    // identity-linked wallets staking to self
    CommissionSniping,     // dropping commission right before snapshot, raising after
    VoteCreditManipulation,
    SybilCluster,          // multiple validators, shared infra + funding
    StakePoolCriteriaTuning, // adjusting metrics to barely meet thresholds
}

/// Alert fired by the engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: uuid::Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub severity: AlertSeverity,
    pub category: AlertCategory,
    pub title: String,
    pub body: String,
    pub related_validator: Option<Pubkey>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity { Critical, Warning, Info }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertCategory {
    ThreatApproaching,
    OpportunityWindow,
    QueuePositionDrop,
    GamingDetected,
    CohortShift,
}
```

---

## Data Sources & API Calls

### On-Chain (Solana RPC)

| RPC Method | Purpose | Frequency |
|---|---|---|
| `getVoteAccounts` | Full validator set: stake, commission, credits, delinquency | Every epoch + hourly |
| `getProgramAccounts` (Stake program) | Individual stake account enumeration for flow tracking | Every epoch |
| `getEpochInfo` | Current epoch + slot for boundary detection | Every 30s |
| `getVersion` | Node version for each validator | Every epoch |
| `getClusterNodes` | IP/gossip for datacenter inference | Every epoch |

### Stake Pool APIs

| Source | Endpoint | Data |
|---|---|---|
| **Marinade** | `https://validators-api.marinade.finance/validators` | Score, stake, eligible flag, algo weights |
| **Marinade** | `https://scoring.marinade.finance/v1/scores` | Raw scoring components |
| **JPool** | `https://api.jpool.one/validators` | Delegation set, scoring criteria |
| **BlazeStake** | `https://stake.solblaze.org/api/v1/cls_validators` | Eligible set + scoring |
| **Jito** | `https://kobe.mainnet.jito.network/api/v1/validators` | MEV commission, tip distribution |
| **Sanctum** | `https://sanctum-s-api.fly.dev/v1/validator/list` | LST validator sets |

### Supplementary

| Source | Purpose |
|---|---|
| **validators.app API** | Datacenter ASN mapping, version tracking |
| **StakeWiz API** | Validator metadata enrichment |
| Local SQLite | Historical snapshots for delta/regression |

---

## CLI Design

```
stake-radar — Competitive intelligence for Solana validators

USAGE:
    stake-radar [OPTIONS] <COMMAND>

COMMANDS:
    threats       List validators threatening your stake position
    opportunities Find validators losing stake (capture targets)
    queue         Show your inferred position in stake pool queues
    gaming        Detect adversarial behavior in the validator set
    cohorts       Analyze stake flows between validator cohorts
    snapshot      Capture current epoch state to local DB
    watch         Continuous monitoring with alert dispatch
    config        Manage configuration

OPTIONS:
    -v, --validator <VOTE_PUBKEY>    Your validator vote account
    -c, --config <PATH>             Config file [default: ~/.config/stake-radar/config.toml]
    -r, --rpc <URL>                 Solana RPC endpoint
    -o, --output <FORMAT>           Output: table, json, csv [default: table]
        --epochs <N>                Lookback window [default: 10]
```

### Example Commands

```bash
# Who is gaining on me?
stake-radar threats -v <VOTE_KEY> --tier critical,rising

# Who is bleeding stake I could capture?
stake-radar opportunities --min-stake 10000 --cause delinquency,commission

# Where do I rank in Marinade's next delegation round?
stake-radar queue -v <VOTE_KEY> --pool marinade,jpool

# Is anyone gaming the system near my tier?
stake-radar gaming --tier mid --confidence 0.7

# How is stake flowing between small and mid validators?
stake-radar cohorts --from small --to mid --epochs 20

# Run continuous radar with Discord alerts
stake-radar watch -v <VOTE_KEY> --alert-webhook https://discord.com/api/webhooks/...
```

### Sample Output: `threats`

```
╔══════════════════════════════════════════════════════════════════════════════╗
║                     STAKE RADAR — THREAT ASSESSMENT                        ║
║  Your stake: 156,230 SOL (rank #312)           Epoch: 742                  ║
╠══════════════════════════════════════════════════════════════════════════════╣
║ TIER     │ VALIDATOR        │ STAKE (SOL) │ VELOCITY   │ ETA OVERTAKE      ║
╠══════════╪══════════════════╪═════════════╪════════════╪═══════════════════╣
║ CRITICAL │ 7xKp..3nRd       │ 149,800     │ +2,140/ep  │ ~3 epochs         ║
║ CRITICAL │ Bm4Q..eFv2       │ 151,020     │ +1,890/ep  │ ~3 epochs         ║
║ RISING   │ 9pLs..kW7j       │ 138,500     │ +2,450/ep  │ ~7 epochs         ║
║ RISING   │ Ht6R..mN4x       │ 142,100     │ +1,620/ep  │ ~9 epochs         ║
║ WATCHING │ 3vFd..aQ8w       │ 128,900     │ +980/ep    │ ~28 epochs        ║
╚══════════╧══════════════════╧═════════════╧════════════╧═══════════════════╝
  Source breakdown for 7xKp..3nRd: 68% Marinade ↑, 22% native ↑, 10% JPool →
```

### Sample Output: `opportunities`

```
╔══════════════════════════════════════════════════════════════════════════════╗
║                  STAKE RADAR — DECAY OPPORTUNITIES                         ║
║  Scanning epoch range: 732–742                                             ║
╠══════════════════════════════════════════════════════════════════════════════╣
║ VALIDATOR        │ LOST (SOL) │ LOST %  │ CAUSE             │ DIFFICULTY   ║
╠══════════════════╪════════════╪═════════╪═══════════════════╪══════════════╣
║ QwR3..7mNp       │ -48,200    │ -31.2%  │ Delinquency       │ Easy         ║
║ 5tYk..xB9e       │ -22,100    │ -14.8%  │ Commission hike   │ Easy         ║
║ Lp8N..4vDs       │ -35,600    │ -19.1%  │ Datacenter conc.  │ Medium       ║
║ Wm2J..kR6a       │ -18,900    │ -11.3%  │ Version lag       │ Medium       ║
╚══════════════════╧════════════╧═════════╧═══════════════════╧══════════════╝
  ~124,800 SOL estimated orphaned across these 4 validators
```

---

## Config File

```toml
[validator]
vote_pubkey = "YourVoteAccountPubkeyHere"

[rpc]
url = "https://api.mainnet-beta.solana.com"
requests_per_second = 5

[snapshot]
db_path = "~/.local/share/stake-radar/snapshots.db"
auto_snapshot = true

[analysis]
lookback_epochs = 10
threat_overtake_horizon_epochs = 15
min_opportunity_stake_sol = 5000.0
gaming_confidence_threshold = 0.65

[cohorts]
custom = [
    { name = "my-peers", filter = "stake:100000-200000,commission:0-5" },
]

[alerts]
discord_webhook = ""
telegram_bot_token = ""
telegram_chat_id = ""
enable_stdout = true

[alerts.rules]
threat_critical = true
opportunity_easy = true
queue_drop = true
gaming_detected = true
```

---

## Analysis Algorithms

### Threat Velocity

For each validator in your stake tier (±50%), compute a weighted linear regression over the lookback window. Weight recent epochs more heavily (exponential decay λ=0.85). The slope is velocity (SOL/epoch); the change in slope is acceleration. Project forward: `epochs_to_overtake = (your_stake - their_stake) / their_velocity` when velocity is positive and their stake < yours.

### Decay Opportunity Scoring

Identify validators with negative stake delta over 3+ consecutive epochs. Classify cause by examining correlated signals (skip rate spike → delinquency, commission change event → commission hike, etc.). Estimate orphan stake as `lost_stake × (1 - pool_managed_fraction)` — pool-managed stake re-delegates automatically and is harder to capture.

### Stake Pool Queue Inference

Each pool publishes scoring criteria (or we reverse-engineer from observed delegation patterns). Reconstruct scoring locally: pull the full eligible set, compute scores, sort, find your rank. Compare against prior epochs to determine queue direction. For Marinade specifically, replicate their published algo (stake concentration, performance, commission, infra diversity).

### Adversarial Gaming Detection

- **Self-stake inflation**: Cross-reference stake account authorities with validator identity and known associated wallets. Flag when >30% of recent stake inflow traces to identity-adjacent accounts.
- **Commission sniping**: Track commission changes relative to stake pool snapshot timing. Flag validators who lower commission within 2 epochs of pool scoring and raise it within 2 epochs after.
- **Sybil clusters**: Cluster validators by shared infrastructure signals (same datacenter, correlated skip patterns, funding source overlap). Flag clusters where combined stake exceeds individual detection thresholds.

### Cohort Flow Analysis

Assign each validator to cohorts (stake tier, region, commission bucket). At each epoch boundary, compute net stake flow between every cohort pair. Aggregate delegator-level movements by tracing stake account authority changes and activation/deactivation across validators. Present as a directed flow graph.

---

## Implementation Phases

### Phase 1: Foundation
- RPC polling for `getVoteAccounts` with epoch boundary detection
- SQLite snapshot store with epoch-keyed validator records
- Basic delta computation (epoch-over-epoch)
- CLI skeleton with `snapshot` and `threats` commands
- Table output renderer

### Phase 2: Intelligence
- Threat velocity with weighted regression
- Decay opportunity scanner with cause classification
- Stake pool API integrations (Marinade, JPool, BlazeStake)
- Queue position inference for Marinade (published algo)
- JSON/CSV output

### Phase 3: Adversarial
- Stake account enumeration for flow tracing
- Self-stake inflation detection
- Commission sniping detection
- Sybil cluster analysis
- Gaming confidence scoring

### Phase 4: Operations
- `watch` mode with configurable polling interval
- Alert engine with rule evaluation
- Discord/Telegram webhook sinks
- Cohort flow analysis and visualization
- Config validation and migration tooling
