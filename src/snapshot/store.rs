use crate::analysis::queue::{rank_scores, PoolScore};
use crate::models::StakePoolId;
use crate::models::ValidatorSnapshot;
use crate::rpc::StakeAccountRecord;
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
                |row| row.get::<_, i64>(0),
            )
            .ok();
        Ok(row.map(i64_to_u64))
    }

    pub fn recent_epochs(&self, limit: u32) -> Result<Vec<u64>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT epoch FROM validator_snapshots ORDER BY epoch DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![i64::from(limit)], |row| row.get::<_, i64>(0))?;
        let mut epochs = Vec::new();
        for item in rows {
            epochs.push(i64_to_u64(item?));
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
                let epoch = u64_to_i64(snapshot.epoch);
                let slot = u64_to_i64(snapshot.slot_captured);
                let credits = u64_to_i64(snapshot.vote_credits_epoch);
                let prior_credits = u64_to_i64(snapshot.vote_credits_prior_epoch);
                stmt.execute(params![
                    &snapshot.vote_pubkey,
                    &snapshot.identity,
                    epoch,
                    slot,
                    snapshot.activated_stake_sol,
                    snapshot.commission_pct,
                    credits,
                    prior_credits,
                    snapshot.skip_rate,
                    i64::from(snapshot.delinquent),
                    &snapshot.version,
                    &snapshot.dc_location,
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

        let rows = stmt.query_map(
            params![u64_to_i64(epoch_from), u64_to_i64(epoch_to)],
            row_to_snapshot,
        )?;
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
        let rows = stmt.query_map(
            params![vote_pubkey, u64_to_i64(start), u64_to_i64(latest)],
            row_to_snapshot,
        )?;
        let mut snapshots = Vec::new();
        for row in rows {
            snapshots.push(row?);
        }
        Ok(snapshots)
    }

    pub fn insert_stake_accounts(
        &mut self,
        epoch: u64,
        records: &[StakeAccountRecord],
    ) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut inserted = 0usize;
        {
            let mut stmt = tx.prepare(
                "
                INSERT INTO stake_account_snapshots (
                    epoch,
                    stake_pubkey,
                    delegated_vote_pubkey,
                    staker,
                    withdrawer,
                    delegated_stake_sol,
                    activation_epoch,
                    deactivation_epoch,
                    state
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(epoch, stake_pubkey) DO UPDATE SET
                    delegated_vote_pubkey = excluded.delegated_vote_pubkey,
                    staker = excluded.staker,
                    withdrawer = excluded.withdrawer,
                    delegated_stake_sol = excluded.delegated_stake_sol,
                    activation_epoch = excluded.activation_epoch,
                    deactivation_epoch = excluded.deactivation_epoch,
                    state = excluded.state
                ",
            )?;

            for record in records {
                stmt.execute(params![
                    u64_to_i64(epoch),
                    &record.stake_pubkey,
                    &record.delegated_vote_pubkey,
                    &record.staker,
                    &record.withdrawer,
                    record.delegated_stake_sol,
                    record.activation_epoch.map(u64_to_i64),
                    record.deactivation_epoch.map(u64_to_i64),
                    &record.state,
                ])?;
                inserted += 1;
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    pub fn load_latest_stake_accounts(&self) -> Result<Vec<StakeAccountRecord>> {
        let latest_epoch = self
            .conn
            .query_row(
                "SELECT epoch FROM stake_account_snapshots ORDER BY epoch DESC LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .ok();
        let Some(latest_epoch) = latest_epoch else {
            return Ok(Vec::new());
        };
        self.load_stake_accounts_for_epoch(i64_to_u64(latest_epoch))
    }

    pub fn load_stake_accounts_for_epoch(&self, epoch: u64) -> Result<Vec<StakeAccountRecord>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT
                stake_pubkey,
                delegated_vote_pubkey,
                staker,
                withdrawer,
                delegated_stake_sol,
                activation_epoch,
                deactivation_epoch,
                state
            FROM stake_account_snapshots
            WHERE epoch = ?1
            ",
        )?;
        let rows = stmt.query_map(params![u64_to_i64(epoch)], |row| {
            Ok(StakeAccountRecord {
                stake_pubkey: row.get(0)?,
                delegated_vote_pubkey: row.get(1)?,
                staker: row.get(2)?,
                withdrawer: row.get(3)?,
                delegated_stake_sol: row.get(4)?,
                activation_epoch: row.get::<_, Option<i64>>(5)?.map(i64_to_u64),
                deactivation_epoch: row.get::<_, Option<i64>>(6)?.map(i64_to_u64),
                state: row.get(7)?,
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub fn insert_pool_rankings(
        &mut self,
        epoch: u64,
        pool: StakePoolId,
        scores: &[PoolScore],
    ) -> Result<()> {
        let ranked = rank_scores(scores);
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "
                INSERT INTO pool_queue_snapshots (
                    epoch,
                    pool,
                    vote_pubkey,
                    rank,
                    score,
                    delegated_stake_sol,
                    projected_delegation_sol
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(epoch, pool, vote_pubkey) DO UPDATE SET
                    rank = excluded.rank,
                    score = excluded.score,
                    delegated_stake_sol = excluded.delegated_stake_sol,
                    projected_delegation_sol = excluded.projected_delegation_sol
                ",
            )?;

            for (idx, score) in ranked.iter().enumerate() {
                stmt.execute(params![
                    u64_to_i64(epoch),
                    pool.to_string(),
                    &score.vote_pubkey,
                    i64::try_from(idx + 1).unwrap_or(i64::MAX),
                    score.score,
                    score.delegated_stake_sol,
                    score.projected_delegation_sol,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_prior_queue_rank(
        &self,
        pool: StakePoolId,
        vote_pubkey: &str,
        before_epoch: u64,
    ) -> Result<Option<u32>> {
        let rank = self
            .conn
            .query_row(
                "
                SELECT rank
                FROM pool_queue_snapshots
                WHERE pool = ?1
                  AND vote_pubkey = ?2
                  AND epoch < ?3
                ORDER BY epoch DESC
                LIMIT 1
                ",
                params![pool.to_string(), vote_pubkey, u64_to_i64(before_epoch)],
                |row| row.get::<_, i64>(0),
            )
            .ok();
        Ok(rank.map(|value| {
            if value < 1 {
                1
            } else {
                value.min(u32::MAX as i64) as u32
            }
        }))
    }
}

fn row_to_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<ValidatorSnapshot> {
    Ok(ValidatorSnapshot {
        vote_pubkey: row.get(0)?,
        identity: row.get(1)?,
        epoch: i64_to_u64(row.get(2)?),
        slot_captured: i64_to_u64(row.get(3)?),
        activated_stake_sol: row.get(4)?,
        commission_pct: row.get(5)?,
        vote_credits_epoch: i64_to_u64(row.get(6)?),
        vote_credits_prior_epoch: i64_to_u64(row.get(7)?),
        skip_rate: row.get(8)?,
        delinquent: row.get::<_, i64>(9)? != 0,
        version: row.get(10)?,
        dc_location: row.get(11)?,
        superminority_member: row.get::<_, i64>(12)? != 0,
    })
}

fn i64_to_u64(value: i64) -> u64 {
    if value < 0 {
        0
    } else {
        value as u64
    }
}

fn u64_to_i64(value: u64) -> i64 {
    if value > i64::MAX as u64 {
        i64::MAX
    } else {
        value as i64
    }
}
