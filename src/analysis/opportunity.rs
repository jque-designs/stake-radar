use crate::models::{CaptureDifficulty, DecayCause, DecayOpportunity, ValidatorSnapshot};
use crate::snapshot::ValidatorFlowPressure;
use std::collections::HashMap;

pub fn detect_decay_opportunities(
    histories: &HashMap<String, Vec<ValidatorSnapshot>>,
    min_stake_lost_sol: f64,
) -> Vec<DecayOpportunity> {
    detect_decay_opportunities_with_flow(histories, min_stake_lost_sol, None)
}

pub fn detect_decay_opportunities_with_flow(
    histories: &HashMap<String, Vec<ValidatorSnapshot>>,
    min_stake_lost_sol: f64,
    flow_pressure: Option<&HashMap<String, ValidatorFlowPressure>>,
) -> Vec<DecayOpportunity> {
    let mut opportunities = Vec::new();

    for (vote_pubkey, history) in histories {
        if history.len() < 4 {
            continue;
        }

        let mut consecutive_losses = 0u32;
        let mut total_lost = 0.0f64;
        let mut total_lost_pct = 0.0f64;
        let mut commission_increase = false;
        let mut delinquency_flag = false;
        let mut high_skip_rate = false;

        for pair in history.windows(2).rev() {
            let prev = &pair[0];
            let curr = &pair[1];
            let delta = curr.activated_stake_sol - prev.activated_stake_sol;
            if delta < 0.0 {
                consecutive_losses += 1;
                total_lost += -delta;
                if prev.activated_stake_sol > 0.0 {
                    total_lost_pct += (-delta / prev.activated_stake_sol).abs();
                }
                if curr.commission_pct > prev.commission_pct {
                    commission_increase = true;
                }
                if curr.delinquent {
                    delinquency_flag = true;
                }
                if curr.skip_rate > 0.15 {
                    high_skip_rate = true;
                }
            } else {
                break;
            }
        }

        if consecutive_losses < 3 || total_lost < min_stake_lost_sol {
            continue;
        }

        let net_outbound = flow_pressure
            .and_then(|pressure| pressure.get(vote_pubkey))
            .map(ValidatorFlowPressure::net_outbound_sol)
            .unwrap_or(0.0);
        let pressure_ratio = if total_lost > 0.0 {
            (net_outbound / total_lost).clamp(0.0, 2.0)
        } else {
            0.0
        };

        let probable_cause = if delinquency_flag {
            DecayCause::Delinquency
        } else if commission_increase {
            DecayCause::CommissionIncrease
        } else if high_skip_rate {
            DecayCause::HighSkipRate
        } else if net_outbound > min_stake_lost_sol * 0.5 {
            DecayCause::StakePoolDelisting
        } else {
            DecayCause::Unknown
        };

        let latest = history.last().expect("history has at least one item");
        let orphan_factor = match probable_cause {
            DecayCause::CommissionIncrease | DecayCause::Delinquency => 0.85,
            DecayCause::HighSkipRate => 0.75,
            DecayCause::StakePoolDelisting => 0.92,
            _ => 0.65,
        } * (1.0 + 0.2 * pressure_ratio.min(1.0));

        let capture_difficulty = if latest.commission_pct <= 5 {
            CaptureDifficulty::Easy
        } else if latest.commission_pct <= 10 {
            CaptureDifficulty::Medium
        } else {
            CaptureDifficulty::Hard
        };

        opportunities.push(DecayOpportunity {
            vote_pubkey: vote_pubkey.clone(),
            stake_lost_sol: total_lost,
            stake_lost_pct: total_lost_pct,
            loss_duration_epochs: consecutive_losses,
            probable_cause,
            estimated_orphan_stake_sol: total_lost * orphan_factor,
            capture_difficulty,
        });
    }

    opportunities.sort_by(|a, b| b.stake_lost_sol.total_cmp(&a.stake_lost_sol));
    opportunities
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(
        epoch: u64,
        stake: f64,
        commission: u8,
        delinquent: bool,
    ) -> ValidatorSnapshot {
        ValidatorSnapshot {
            vote_pubkey: "vote".to_string(),
            identity: "identity".to_string(),
            epoch,
            slot_captured: 0,
            activated_stake_sol: stake,
            commission_pct: commission,
            vote_credits_epoch: 100,
            vote_credits_prior_epoch: 95,
            skip_rate: if delinquent { 0.2 } else { 0.02 },
            delinquent,
            version: None,
            dc_location: None,
            superminority_member: false,
        }
    }

    #[test]
    fn finds_consecutive_decay() {
        let mut histories = HashMap::new();
        histories.insert(
            "vote".to_string(),
            vec![
                make_snapshot(1, 1000.0, 5, false),
                make_snapshot(2, 900.0, 5, false),
                make_snapshot(3, 800.0, 7, false),
                make_snapshot(4, 700.0, 7, false),
            ],
        );
        let opportunities = detect_decay_opportunities(&histories, 150.0);
        assert_eq!(opportunities.len(), 1);
        assert_eq!(
            opportunities[0].probable_cause,
            DecayCause::CommissionIncrease
        );
    }

    #[test]
    fn upgrades_cause_when_flow_pressure_is_high() {
        let mut histories = HashMap::new();
        histories.insert(
            "vote".to_string(),
            vec![
                make_snapshot(1, 1000.0, 5, false),
                make_snapshot(2, 900.0, 5, false),
                make_snapshot(3, 820.0, 5, false),
                make_snapshot(4, 740.0, 5, false),
            ],
        );
        let mut flow_pressure = HashMap::new();
        flow_pressure.insert(
            "vote".to_string(),
            ValidatorFlowPressure {
                outbound_sol: 280.0,
                inbound_sol: 0.0,
                reallocated_outbound_sol: 260.0,
                entered_sol: 0.0,
                exited_sol: 280.0,
            },
        );
        let opportunities =
            detect_decay_opportunities_with_flow(&histories, 100.0, Some(&flow_pressure));
        assert_eq!(opportunities.len(), 1);
        assert_eq!(
            opportunities[0].probable_cause,
            DecayCause::StakePoolDelisting
        );
        assert!(opportunities[0].estimated_orphan_stake_sol > 200.0);
    }
}
