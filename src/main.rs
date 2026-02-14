use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

use stake_radar::alert::engine::run_alert_engine;
use stake_radar::analysis;
use stake_radar::analysis::opportunity::detect_decay_opportunities;
use stake_radar::analysis::queue::infer_queue_position;
use stake_radar::analysis::threat::analyze_threats;
use stake_radar::config::{AppConfig, ConfigOverrides};
use stake_radar::models::StakePoolId;
use stake_radar::output;
use stake_radar::pools;
use stake_radar::rpc::RpcClient;
use stake_radar::snapshot::SnapshotStore;

#[derive(Debug, Parser)]
#[command(
    name = "stake-radar",
    version,
    about = "Competitive intelligence radar for Solana validators"
)]
struct Cli {
    #[arg(short = 'v', long = "validator")]
    validator: Option<String>,

    #[arg(short = 'c', long = "config")]
    config: Option<PathBuf>,

    #[arg(short = 'r', long = "rpc")]
    rpc: Option<String>,

    #[arg(short = 'o', long = "output", default_value = "table")]
    output: OutputFormat,

    #[arg(long = "epochs")]
    epochs: Option<u32>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Table,
    Json,
    Csv,
}

#[derive(Debug, Subcommand)]
enum Command {
    Threats {
        #[arg(long = "tier")]
        tier: Option<String>,
    },
    Opportunities {
        #[arg(long = "min-stake")]
        min_stake: Option<f64>,
        #[arg(long = "cause")]
        cause: Option<String>,
    },
    Queue {
        #[arg(long = "pool")]
        pool: Option<String>,
    },
    Gaming {
        #[arg(long = "tier")]
        #[allow(dead_code)]
        tier: Option<String>,
        #[arg(long = "confidence")]
        confidence: Option<f64>,
    },
    Cohorts {
        #[arg(long = "from")]
        from: Option<String>,
        #[arg(long = "to")]
        to: Option<String>,
    },
    Snapshot,
    Watch {
        #[arg(long = "interval-seconds", default_value_t = 60)]
        interval_seconds: u64,
        #[arg(long = "max-cycles")]
        max_cycles: Option<u32>,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Show,
    Init {
        #[arg(long = "force", default_value_t = false)]
        force: bool,
    },
    Path,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    let (mut config, config_path) = AppConfig::load(cli.config.clone())?;
    config.apply_overrides(&ConfigOverrides {
        vote_pubkey: cli.validator.clone(),
        rpc_url: cli.rpc.clone(),
        lookback_epochs: cli.epochs,
    });

    if let Command::Config { command } = &cli.command {
        return handle_config_command(command, &config, &config_path);
    }

    let mut store = SnapshotStore::open(&config.snapshot_db_path())?;
    let rpc = RpcClient::from_config(&config.rpc);
    let http = reqwest::Client::new();
    let lookback = cli.epochs.unwrap_or(config.analysis.lookback_epochs);

    match cli.command {
        Command::Snapshot => {
            let (epoch, rows) = capture_snapshot(&rpc, &mut store).await?;
            println!("Captured {rows} validator snapshots for epoch {epoch}");
        }
        Command::Threats { tier } => {
            let your_validator = require_validator(&config)?;
            let (epochs, histories) = load_histories(&store, lookback)?;
            ensure_non_empty_epochs(&epochs)?;

            let mut threats = analyze_threats(
                &histories,
                &your_validator,
                config.analysis.threat_overtake_horizon_epochs,
            );
            if let Some(tier_filter) = tier {
                let filter_set = parse_csv_set(&tier_filter);
                threats.retain(|threat| filter_set.contains(&threat.threat_tier.to_string()));
            }

            let your_stake = histories
                .get(&your_validator)
                .and_then(|history| history.last())
                .map(|snapshot| snapshot.activated_stake_sol)
                .unwrap_or_default();
            let latest_epoch = *epochs
                .first()
                .ok_or_else(|| anyhow!("no epochs available in snapshot store"))?;
            render_threats(&cli.output, &threats, your_stake, latest_epoch)?;
        }
        Command::Opportunities { min_stake, cause } => {
            let (epochs, histories) = load_histories(&store, lookback)?;
            ensure_non_empty_epochs(&epochs)?;
            let min_stake = min_stake.unwrap_or(config.analysis.min_opportunity_stake_sol);
            let mut opportunities = detect_decay_opportunities(&histories, min_stake);
            if let Some(cause_filter) = cause {
                let filter_set = parse_csv_set(&cause_filter);
                opportunities.retain(|opportunity| {
                    filter_set.contains(&opportunity.probable_cause.to_string())
                });
            }

            let epoch_from = *epochs.last().unwrap_or(&0);
            let epoch_to = *epochs.first().unwrap_or(&0);
            render_opportunities(&cli.output, &opportunities, epoch_from, epoch_to)?;
        }
        Command::Queue { pool } => {
            let your_validator = require_validator(&config)?;
            let pools = parse_pools(pool.as_deref().unwrap_or("marinade"))?;
            let mut positions = Vec::new();
            for pool_id in pools {
                match pools::fetch_pool_scores(pool_id, &http).await {
                    Ok(scores) => {
                        if let Some(position) =
                            infer_queue_position(&your_validator, pool_id, &scores, None)
                        {
                            positions.push(position);
                        }
                    }
                    Err(err) => {
                        warn!("failed to fetch pool data for {pool_id}: {err:#}");
                    }
                }
            }
            if positions.is_empty() {
                warn!("no queue positions were inferred for the selected pools");
            }
            render_queue(&cli.output, &positions)?;
        }
        Command::Gaming { confidence, .. } => {
            let (_, histories) = load_histories(&store, lookback)?;
            let threshold = confidence.unwrap_or(config.analysis.gaming_confidence_threshold);
            let signals = analysis::adversarial::detect_gaming_signals(&histories, threshold);
            render_gaming(&cli.output, &signals)?;
        }
        Command::Cohorts { from, to } => {
            let epochs = store.recent_epochs(lookback)?;
            ensure_non_empty_epochs(&epochs)?;
            if epochs.len() < 2 {
                return Err(anyhow!(
                    "at least two snapshot epochs are needed for cohort flow analysis"
                ));
            }

            let from_epoch = *epochs.last().expect("len checked");
            let to_epoch = *epochs.first().expect("len checked");
            let from_snapshots = store.load_epoch_snapshots(from_epoch)?;
            let to_snapshots = store.load_epoch_snapshots(to_epoch)?;
            let mut flows = analysis::cohort::compute_cohort_flows(
                from_epoch,
                to_epoch,
                &from_snapshots,
                &to_snapshots,
            );
            if let Some(from_filter) = from {
                let from_filter = from_filter.to_ascii_lowercase();
                flows.retain(|flow| flow.from_cohort.to_string().contains(&from_filter));
            }
            if let Some(to_filter) = to {
                let to_filter = to_filter.to_ascii_lowercase();
                flows.retain(|flow| flow.to_cohort.to_string().contains(&to_filter));
            }
            render_cohort_flows(&cli.output, &flows)?;
        }
        Command::Watch {
            interval_seconds,
            max_cycles,
        } => {
            let your_validator = require_validator(&config)?;
            let mut cycle = 0u32;
            loop {
                cycle += 1;
                info!(cycle, "starting watch cycle");
                if config.snapshot.auto_snapshot {
                    if let Err(err) = capture_snapshot(&rpc, &mut store).await {
                        warn!("snapshot capture failed in watch mode: {err:#}");
                    }
                }

                let (_, histories) = load_histories(&store, lookback)?;
                let threats = analyze_threats(
                    &histories,
                    &your_validator,
                    config.analysis.threat_overtake_horizon_epochs,
                );
                let opportunities = detect_decay_opportunities(
                    &histories,
                    config.analysis.min_opportunity_stake_sol,
                );
                let signals = analysis::adversarial::detect_gaming_signals(
                    &histories,
                    config.analysis.gaming_confidence_threshold,
                );
                let queue_positions = Vec::new();
                let alerts = run_alert_engine(
                    &config.alerts,
                    &threats,
                    &opportunities,
                    &queue_positions,
                    &signals,
                    &http,
                )
                .await?;
                info!(
                    cycle,
                    threats = threats.len(),
                    opportunities = opportunities.len(),
                    signals = signals.len(),
                    alerts = alerts.len(),
                    "watch cycle complete"
                );

                if let Some(max_cycles) = max_cycles {
                    if cycle >= max_cycles {
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_secs(interval_seconds)).await;
            }
        }
        Command::Config { .. } => unreachable!("config command handled earlier"),
    }

    Ok(())
}

fn handle_config_command(
    command: &ConfigCommand,
    config: &AppConfig,
    path: &PathBuf,
) -> Result<()> {
    match command {
        ConfigCommand::Show => {
            println!("{}", toml::to_string_pretty(config)?);
        }
        ConfigCommand::Init { force } => {
            if path.exists() && !*force {
                return Err(anyhow!(
                    "config already exists at {} (use --force to overwrite)",
                    path.display()
                ));
            }
            let defaults = AppConfig::default();
            defaults.save(path)?;
            println!("Wrote default config to {}", path.display());
        }
        ConfigCommand::Path => {
            println!("{}", path.display());
        }
    }
    Ok(())
}

async fn capture_snapshot(rpc: &RpcClient, store: &mut SnapshotStore) -> Result<(u64, usize)> {
    let epoch_info = rpc.get_epoch_info().await?;
    let snapshots = rpc
        .get_vote_snapshots(epoch_info.epoch, epoch_info.absolute_slot)
        .await
        .context("failed to fetch vote account snapshots")?;
    let rows = store.insert_snapshots(&snapshots)?;
    Ok((epoch_info.epoch, rows))
}

fn load_histories(
    store: &SnapshotStore,
    lookback_epochs: u32,
) -> Result<(
    Vec<u64>,
    std::collections::HashMap<String, Vec<stake_radar::models::ValidatorSnapshot>>,
)> {
    let epochs = store.recent_epochs(lookback_epochs)?;
    if epochs.is_empty() {
        return Ok((epochs, std::collections::HashMap::new()));
    }
    let from_epoch = *epochs.last().expect("not empty");
    let to_epoch = *epochs.first().expect("not empty");
    let snapshots = store.load_snapshots_for_epoch_range(from_epoch, to_epoch)?;
    let histories = analysis::group_by_validator(&snapshots);
    Ok((epochs, histories))
}

fn ensure_non_empty_epochs(epochs: &[u64]) -> Result<()> {
    if epochs.is_empty() {
        Err(anyhow!(
            "no snapshots available. run `stake-radar snapshot` first"
        ))
    } else {
        Ok(())
    }
}

fn parse_csv_set(raw: &str) -> HashSet<String> {
    raw.split(',')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .collect()
}

fn parse_pools(raw: &str) -> Result<Vec<StakePoolId>> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.parse::<StakePoolId>().map_err(|err| anyhow!(err)))
        .collect()
}

fn require_validator(config: &AppConfig) -> Result<String> {
    config
        .validator
        .vote_pubkey
        .clone()
        .ok_or_else(|| anyhow!("validator vote account required (--validator or config)"))
}

fn render_threats(
    output_format: &OutputFormat,
    threats: &[stake_radar::models::ThreatProfile],
    your_stake: f64,
    epoch: u64,
) -> Result<()> {
    match output_format {
        OutputFormat::Table => {
            output::table::render_threats(threats, your_stake, epoch);
            Ok(())
        }
        OutputFormat::Json => output::json::print_json(threats),
        OutputFormat::Csv => output::csv::print_threats_csv(threats),
    }
}

fn render_opportunities(
    output_format: &OutputFormat,
    opportunities: &[stake_radar::models::DecayOpportunity],
    epoch_from: u64,
    epoch_to: u64,
) -> Result<()> {
    match output_format {
        OutputFormat::Table => {
            output::table::render_opportunities(opportunities, epoch_from, epoch_to);
            Ok(())
        }
        OutputFormat::Json => output::json::print_json(opportunities),
        OutputFormat::Csv => output::csv::print_opportunities_csv(opportunities),
    }
}

fn render_queue(
    output_format: &OutputFormat,
    queue_positions: &[stake_radar::models::PoolQueuePosition],
) -> Result<()> {
    match output_format {
        OutputFormat::Table => {
            output::table::render_queue_positions(queue_positions);
            Ok(())
        }
        OutputFormat::Json => output::json::print_json(queue_positions),
        OutputFormat::Csv => output::csv::print_queue_csv(queue_positions),
    }
}

fn render_gaming(
    output_format: &OutputFormat,
    signals: &[stake_radar::models::GamingSignal],
) -> Result<()> {
    match output_format {
        OutputFormat::Table => {
            output::table::render_gaming(signals);
            Ok(())
        }
        OutputFormat::Json => output::json::print_json(signals),
        OutputFormat::Csv => output::csv::print_gaming_csv(signals),
    }
}

fn render_cohort_flows(
    output_format: &OutputFormat,
    flows: &[stake_radar::models::CohortFlow],
) -> Result<()> {
    match output_format {
        OutputFormat::Table => {
            output::table::render_cohort_flows(flows);
            Ok(())
        }
        OutputFormat::Json => output::json::print_json(flows),
        OutputFormat::Csv => output::csv::print_cohort_flows_csv(flows),
    }
}
