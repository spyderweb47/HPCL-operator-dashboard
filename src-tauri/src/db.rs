//! Local SQLite file: the source of truth on this PC. Every synced table carries
//! `version` (bumped on each local change) and `synced_version` (last version the cloud has).

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE products (
  id              TEXT PRIMARY KEY,
  name            TEXT NOT NULL COLLATE NOCASE UNIQUE,
  last_used_price REAL NOT NULL,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  version         INTEGER NOT NULL DEFAULT 1,
  synced_version  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE delivery_boys (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL COLLATE NOCASE UNIQUE,
  is_active      INTEGER NOT NULL DEFAULT 1,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  version        INTEGER NOT NULL DEFAULT 1,
  synced_version INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE transactions (
  id               TEXT PRIMARY KEY,
  date_string      TEXT NOT NULL,
  type             TEXT NOT NULL CHECK (type IN ('CASH_IN', 'CASH_OUT')),
  product_name     TEXT NOT NULL,
  price            REAL NOT NULL,
  consumer_no      TEXT,
  is_other_product INTEGER NOT NULL DEFAULT 0,
  status           TEXT NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'EDITED', 'DELETED')),
  is_day_finalized INTEGER NOT NULL DEFAULT 0,
  audit_log        TEXT NOT NULL DEFAULT '[]',
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  version          INTEGER NOT NULL DEFAULT 1,
  synced_version   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX transactions_by_date ON transactions (date_string, created_at);

CREATE TABLE day_end_logs (
  id             TEXT PRIMARY KEY,
  date_string    TEXT NOT NULL UNIQUE,
  deliveries     TEXT NOT NULL DEFAULT '[]',
  cylinder_stock TEXT NOT NULL,
  is_finalized   INTEGER NOT NULL DEFAULT 0,
  finalized_at   TEXT,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  version        INTEGER NOT NULL DEFAULT 1,
  synced_version INTEGER NOT NULL DEFAULT 0
);

-- Partial indexes over unsynced rows: "what still needs uploading?" stays instant however much history is kept.
CREATE INDEX products_pending      ON products      (id) WHERE version > synced_version;
CREATE INDEX delivery_boys_pending ON delivery_boys (id) WHERE version > synced_version;
CREATE INDEX transactions_pending  ON transactions  (id) WHERE version > synced_version;
CREATE INDEX day_end_logs_pending  ON day_end_logs  (id) WHERE version > synced_version;
"#];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let mut conn = Connection::open(path)?;
    // auto_vacuum only takes effect before the first table exists, i.e. on a brand-new file.
    conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

pub fn meta_get(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional()
}

pub fn meta_set(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

/// Return freed pages to the OS after pruning (needs auto_vacuum = INCREMENTAL).
pub fn incremental_vacuum(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare("PRAGMA incremental_vacuum")?;
    let mut rows = stmt.query([])?;
    while rows.next()?.is_some() {}
    Ok(())
}
