pub mod adversarial;
pub mod cohort;
pub mod opportunity;
pub mod queue;
pub mod threat;

use crate::models::ValidatorSnapshot;
use std::collections::HashMap;

pub fn group_by_validator(
    snapshots: &[ValidatorSnapshot],
) -> HashMap<String, Vec<ValidatorSnapshot>> {
    let mut grouped: HashMap<String, Vec<ValidatorSnapshot>> = HashMap::new();
    for snapshot in snapshots {
        grouped
            .entry(snapshot.vote_pubkey.clone())
            .or_default()
            .push(snapshot.clone());
    }
    for history in grouped.values_mut() {
        history.sort_by_key(|snapshot| snapshot.epoch);
    }
    grouped
}
