use crate::models::{
    short_pubkey, CohortFlow, DecayOpportunity, GamingSignal, PoolQueuePosition, ThreatProfile,
    ThreatTier,
};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, ContentArrangement, Table};

pub fn render_threats(threats: &[ThreatProfile], your_stake_sol: f64, epoch: u64) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Tier", "Validator", "Stake (SOL)", "Velocity", "ETA"]);

    for threat in threats {
        table.add_row(vec![
            tier_label(threat.threat_tier).to_string(),
            short_pubkey(&threat.vote_pubkey),
            format!("{:.0}", threat.current_stake_sol),
            format!("{:+.1}/ep", threat.velocity_sol_per_epoch),
            threat
                .epochs_to_overtake
                .map(|eta| format!("~{eta} epochs"))
                .unwrap_or_else(|| "-".to_string()),
        ]);
    }

    println!("STAKE RADAR — THREAT ASSESSMENT");
    println!("Your stake: {:.0} SOL | Epoch: {}", your_stake_sol, epoch);
    println!("{table}");
}

pub fn render_opportunities(opportunities: &[DecayOpportunity], epoch_from: u64, epoch_to: u64) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            "Validator",
            "Lost (SOL)",
            "Lost %",
            "Cause",
            "Difficulty",
        ]);

    for opp in opportunities {
        table.add_row(vec![
            short_pubkey(&opp.vote_pubkey),
            format!("-{:.0}", opp.stake_lost_sol),
            format!("-{:.1}%", opp.stake_lost_pct * 100.0),
            opp.probable_cause.to_string(),
            opp.capture_difficulty.to_string(),
        ]);
    }

    let total_orphan: f64 = opportunities
        .iter()
        .map(|opportunity| opportunity.estimated_orphan_stake_sol)
        .sum();
    println!("STAKE RADAR — DECAY OPPORTUNITIES");
    println!("Scanning epoch range: {epoch_from}–{epoch_to}");
    println!("{table}");
    println!(
        "Estimated orphan stake across listed validators: {:.0} SOL",
        total_orphan
    );
}

pub fn render_queue_positions(positions: &[PoolQueuePosition]) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            "Pool",
            "Rank",
            "Total Eligible",
            "Direction",
            "Score",
            "Gap To Next",
            "Projected Delegation",
        ]);

    for queue in positions {
        table.add_row(vec![
            queue.pool.to_string(),
            queue.rank.to_string(),
            queue.total_eligible.to_string(),
            queue.direction.to_string(),
            format!("{:.3}", queue.your_score),
            format!("{:.3}", queue.score_gap_to_next),
            format!("{:.1} SOL", queue.estimated_delegation_sol),
        ]);
    }
    println!("{table}");
}

pub fn render_gaming(signals: &[GamingSignal]) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Validator", "Signal", "Confidence", "Evidence"]);
    for signal in signals {
        table.add_row(vec![
            short_pubkey(&signal.vote_pubkey),
            signal.signal_type.to_string(),
            format!("{:.2}", signal.confidence),
            signal.evidence.join("; "),
        ]);
    }
    println!("{table}");
}

pub fn render_cohort_flows(flows: &[CohortFlow]) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["From", "To", "Flow (SOL)", "Epoch Range"]);
    for flow in flows {
        table.add_row(vec![
            flow.from_cohort.to_string(),
            flow.to_cohort.to_string(),
            format!("{:.0}", flow.flow_sol),
            format!("{}-{}", flow.epoch_range.0, flow.epoch_range.1),
        ]);
    }
    println!("{table}");
}

fn tier_label(tier: ThreatTier) -> &'static str {
    match tier {
        ThreatTier::Critical => "CRITICAL",
        ThreatTier::Rising => "RISING",
        ThreatTier::Watching => "WATCHING",
        ThreatTier::Neutral => "NEUTRAL",
    }
}
