use anyhow::Result;
use rusqlite::{params, Connection};

pub const CURRENT_SCHEMA_VERSION: i64 = 1;

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
