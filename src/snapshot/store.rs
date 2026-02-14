use crate::models::ValidatorSnapshot;
use crate::snapshot::migrations::run_migrations;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::fs;
use std::path::Path;

pub struct SnapshotStore {
    conn: Connection,
}

impl SnapshotStore {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create snapshot db directory {}",
                    parent.display()
                )
            })?;
        }

        let conn = Connection::open(db_path)
            .with_context(|| format!("failed to open sqlite db {}", db_path.display()))?;
        run_migrations(&conn)?;
        Ok(Self { conn })
    }

    pub fn latest_epoch(&self) -> Result<Option<u64>> {
        let row = self
            .conn
            .query_row(
                "SELECT epoch FROM validator_snapshots ORDER BY epoch DESC LIMIT 1",
                [],
                |row| row.get::<_, u64>(0),
            )
            .ok();
        Ok(row)
    }

    pub fn recent_epochs(&self, limit: u32) -> Result<Vec<u64>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT epoch FROM validator_snapshots ORDER BY epoch DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| row.get::<_, u64>(0))?;
        let mut epochs = Vec::new();
        for item in rows {
            epochs.push(item?);
        }
        Ok(epochs)
    }

    pub fn insert_snapshots(&mut self, snapshots: &[ValidatorSnapshot]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut inserted = 0usize;
        {
            let mut stmt = tx.prepare(
                "
                INSERT INTO validator_snapshots (
                    vote_pubkey,
                    identity,
                    epoch,
                    slot_captured,
                    activated_stake_sol,
                    commission_pct,
                    vote_credits_epoch,
                    vote_credits_prior_epoch,
                    skip_rate,
                    delinquent,
                    version,
                    dc_location,
                    superminority_member
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                ON CONFLICT(vote_pubkey, epoch) DO UPDATE SET
                    identity = excluded.identity,
                    slot_captured = excluded.slot_captured,
                    activated_stake_sol = excluded.activated_stake_sol,
                    commission_pct = excluded.commission_pct,
                    vote_credits_epoch = excluded.vote_credits_epoch,
                    vote_credits_prior_epoch = excluded.vote_credits_prior_epoch,
                    skip_rate = excluded.skip_rate,
                    delinquent = excluded.delinquent,
                    version = excluded.version,
                    dc_location = excluded.dc_location,
                    superminority_member = excluded.superminority_member
                ",
            )?;

            for snapshot in snapshots {
                stmt.execute(params![
                    snapshot.vote_pubkey,
                    snapshot.identity,
                    snapshot.epoch,
                    snapshot.slot_captured,
                    snapshot.activated_stake_sol,
                    snapshot.commission_pct,
                    snapshot.vote_credits_epoch,
                    snapshot.vote_credits_prior_epoch,
                    snapshot.skip_rate,
                    i64::from(snapshot.delinquent),
                    snapshot.version,
                    snapshot.dc_location,
                    i64::from(snapshot.superminority_member),
                ])?;
                inserted += 1;
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    pub fn load_epoch_snapshots(&self, epoch: u64) -> Result<Vec<ValidatorSnapshot>> {
        self.load_snapshots_for_epoch_range(epoch, epoch)
    }

    pub fn load_snapshots_for_epoch_range(
        &self,
        epoch_from: u64,
        epoch_to: u64,
    ) -> Result<Vec<ValidatorSnapshot>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT
                vote_pubkey,
                identity,
                epoch,
                slot_captured,
                activated_stake_sol,
                commission_pct,
                vote_credits_epoch,
                vote_credits_prior_epoch,
                skip_rate,
                delinquent,
                version,
                dc_location,
                superminority_member
            FROM validator_snapshots
            WHERE epoch BETWEEN ?1 AND ?2
            ORDER BY epoch ASC, activated_stake_sol DESC
            ",
        )?;

        let rows = stmt.query_map(params![epoch_from, epoch_to], row_to_snapshot)?;
        let mut snapshots = Vec::new();
        for row in rows {
            snapshots.push(row?);
        }
        Ok(snapshots)
    }

    pub fn load_validator_history(
        &self,
        vote_pubkey: &str,
        lookback_epochs: u32,
    ) -> Result<Vec<ValidatorSnapshot>> {
        let latest = match self.latest_epoch()? {
            Some(epoch) => epoch,
            None => return Ok(Vec::new()),
        };
        let start = latest.saturating_sub(lookback_epochs.saturating_sub(1) as u64);
        let mut stmt = self.conn.prepare(
            "
            SELECT
                vote_pubkey,
                identity,
                epoch,
                slot_captured,
                activated_stake_sol,
                commission_pct,
                vote_credits_epoch,
                vote_credits_prior_epoch,
                skip_rate,
                delinquent,
                version,
                dc_location,
                superminority_member
            FROM validator_snapshots
            WHERE vote_pubkey = ?1 AND epoch BETWEEN ?2 AND ?3
            ORDER BY epoch ASC
            ",
        )?;
        let rows = stmt.query_map(params![vote_pubkey, start, latest], row_to_snapshot)?;
        let mut snapshots = Vec::new();
        for row in rows {
            snapshots.push(row?);
        }
        Ok(snapshots)
    }
}

fn row_to_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<ValidatorSnapshot> {
    Ok(ValidatorSnapshot {
        vote_pubkey: row.get(0)?,
        identity: row.get(1)?,
        epoch: row.get(2)?,
        slot_captured: row.get(3)?,
        activated_stake_sol: row.get(4)?,
        commission_pct: row.get(5)?,
        vote_credits_epoch: row.get(6)?,
        vote_credits_prior_epoch: row.get(7)?,
        skip_rate: row.get(8)?,
        delinquent: row.get::<_, i64>(9)? != 0,
        version: row.get(10)?,
        dc_location: row.get(11)?,
        superminority_member: row.get::<_, i64>(12)? != 0,
    })
}
