use crate::models::{CohortFlow, DecayOpportunity, GamingSignal, PoolQueuePosition, ThreatProfile};
use anyhow::Result;

pub fn print_threats_csv(threats: &[ThreatProfile]) -> Result<()> {
    let mut writer = csv::Writer::from_writer(Vec::<u8>::new());
    writer.write_record([
        "vote_pubkey",
        "current_stake_sol",
        "velocity_sol_per_epoch",
        "acceleration",
        "epochs_to_overtake",
        "threat_tier",
        "primary_stake_source",
    ])?;
    for threat in threats {
        writer.write_record([
            threat.vote_pubkey.as_str(),
            &threat.current_stake_sol.to_string(),
            &threat.velocity_sol_per_epoch.to_string(),
            &threat.acceleration.to_string(),
            &threat
                .epochs_to_overtake
                .map(|value| value.to_string())
                .unwrap_or_default(),
            &threat.threat_tier.to_string(),
            &format!("{:?}", threat.primary_stake_source),
        ])?;
    }
    print_csv(writer)
}

pub fn print_opportunities_csv(opportunities: &[DecayOpportunity]) -> Result<()> {
    let mut writer = csv::Writer::from_writer(Vec::<u8>::new());
    writer.write_record([
        "vote_pubkey",
        "stake_lost_sol",
        "stake_lost_pct",
        "loss_duration_epochs",
        "probable_cause",
        "estimated_orphan_stake_sol",
        "capture_difficulty",
    ])?;
    for opportunity in opportunities {
        writer.write_record([
            opportunity.vote_pubkey.as_str(),
            &opportunity.stake_lost_sol.to_string(),
            &opportunity.stake_lost_pct.to_string(),
            &opportunity.loss_duration_epochs.to_string(),
            &opportunity.probable_cause.to_string(),
            &opportunity.estimated_orphan_stake_sol.to_string(),
            &opportunity.capture_difficulty.to_string(),
        ])?;
    }
    print_csv(writer)
}

pub fn print_queue_csv(positions: &[PoolQueuePosition]) -> Result<()> {
    let mut writer = csv::Writer::from_writer(Vec::<u8>::new());
    writer.write_record([
        "pool",
        "your_score",
        "rank",
        "total_eligible",
        "direction",
        "score_gap_to_next",
        "estimated_delegation_sol",
    ])?;
    for queue in positions {
        writer.write_record([
            &queue.pool.to_string(),
            &queue.your_score.to_string(),
            &queue.rank.to_string(),
            &queue.total_eligible.to_string(),
            &queue.direction.to_string(),
            &queue.score_gap_to_next.to_string(),
            &queue.estimated_delegation_sol.to_string(),
        ])?;
    }
    print_csv(writer)
}

pub fn print_gaming_csv(signals: &[GamingSignal]) -> Result<()> {
    let mut writer = csv::Writer::from_writer(Vec::<u8>::new());
    writer.write_record([
        "vote_pubkey",
        "signal_type",
        "confidence",
        "first_detected_epoch",
        "evidence",
    ])?;
    for signal in signals {
        writer.write_record([
            signal.vote_pubkey.as_str(),
            &signal.signal_type.to_string(),
            &signal.confidence.to_string(),
            &signal.first_detected_epoch.to_string(),
            &signal.evidence.join(" | "),
        ])?;
    }
    print_csv(writer)
}

pub fn print_cohort_flows_csv(flows: &[CohortFlow]) -> Result<()> {
    let mut writer = csv::Writer::from_writer(Vec::<u8>::new());
    writer.write_record([
        "from_cohort",
        "to_cohort",
        "flow_sol",
        "epoch_from",
        "epoch_to",
        "delegator_count",
    ])?;
    for flow in flows {
        writer.write_record([
            &flow.from_cohort.to_string(),
            &flow.to_cohort.to_string(),
            &flow.flow_sol.to_string(),
            &flow.epoch_range.0.to_string(),
            &flow.epoch_range.1.to_string(),
            &flow.delegator_count.to_string(),
        ])?;
    }
    print_csv(writer)
}

fn print_csv(mut writer: csv::Writer<Vec<u8>>) -> Result<()> {
    writer.flush()?;
    let bytes = writer.into_inner()?;
    let text = String::from_utf8(bytes)?;
    print!("{text}");
    Ok(())
}
