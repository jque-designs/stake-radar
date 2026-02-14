use crate::models::{CaptureDifficulty, DecayCause, DecayOpportunity, ValidatorSnapshot};
use std::collections::HashMap;

pub fn detect_decay_opportunities(
    histories: &HashMap<String, Vec<ValidatorSnapshot>>,
    min_stake_lost_sol: f64,
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

        let probable_cause = if delinquency_flag {
            DecayCause::Delinquency
        } else if commission_increase {
            DecayCause::CommissionIncrease
        } else if high_skip_rate {
            DecayCause::HighSkipRate
        } else {
            DecayCause::Unknown
        };

        let latest = history.last().expect("history has at least one item");
        let orphan_factor = match probable_cause {
            DecayCause::CommissionIncrease | DecayCause::Delinquency => 0.85,
            DecayCause::HighSkipRate => 0.75,
            _ => 0.65,
        };

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
}
