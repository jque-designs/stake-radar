use anyhow::Result;
use rusqlite::{params, Connection};

pub const CURRENT_SCHEMA_VERSION: i64 = 2;

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_meta (
            key TEXT PRIMARY KEY,
            value INTEGER NOT NULL
        )",
        [],
    )?;

    let version: Option<i64> = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .ok();

    match version {
        Some(v) if v >= CURRENT_SCHEMA_VERSION => return Ok(()),
        Some(_) | None => {}
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS validator_snapshots (
            vote_pubkey TEXT NOT NULL,
            identity TEXT NOT NULL,
            epoch INTEGER NOT NULL,
            slot_captured INTEGER NOT NULL,
            activated_stake_sol REAL NOT NULL,
            commission_pct INTEGER NOT NULL,
            vote_credits_epoch INTEGER NOT NULL,
            vote_credits_prior_epoch INTEGER NOT NULL,
            skip_rate REAL NOT NULL,
            delinquent INTEGER NOT NULL,
            version TEXT NULL,
            dc_location TEXT NULL,
            superminority_member INTEGER NOT NULL,
            captured_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (vote_pubkey, epoch)
        );

        CREATE INDEX IF NOT EXISTS idx_validator_snapshots_epoch
            ON validator_snapshots (epoch);

        CREATE INDEX IF NOT EXISTS idx_validator_snapshots_stake
            ON validator_snapshots (activated_stake_sol);

        CREATE TABLE IF NOT EXISTS stake_account_snapshots (
            epoch INTEGER NOT NULL,
            stake_pubkey TEXT NOT NULL,
            delegated_vote_pubkey TEXT NULL,
            staker TEXT NULL,
            withdrawer TEXT NULL,
            delegated_stake_sol REAL NOT NULL,
            activation_epoch INTEGER NULL,
            deactivation_epoch INTEGER NULL,
            state TEXT NULL,
            captured_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (epoch, stake_pubkey)
        );

        CREATE INDEX IF NOT EXISTS idx_stake_account_snapshots_epoch
            ON stake_account_snapshots (epoch);
        CREATE INDEX IF NOT EXISTS idx_stake_account_snapshots_vote
            ON stake_account_snapshots (delegated_vote_pubkey);

        CREATE TABLE IF NOT EXISTS pool_queue_snapshots (
            epoch INTEGER NOT NULL,
            pool TEXT NOT NULL,
            vote_pubkey TEXT NOT NULL,
            rank INTEGER NOT NULL,
            score REAL NOT NULL,
            delegated_stake_sol REAL NOT NULL,
            projected_delegation_sol REAL NOT NULL,
            captured_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (epoch, pool, vote_pubkey)
        );

        CREATE INDEX IF NOT EXISTS idx_pool_queue_snapshots_lookup
            ON pool_queue_snapshots (pool, vote_pubkey, epoch DESC);
        ",
    )?;

    conn.execute(
        "INSERT INTO schema_meta(key, value)
         VALUES('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![CURRENT_SCHEMA_VERSION],
    )?;

    Ok(())
}
