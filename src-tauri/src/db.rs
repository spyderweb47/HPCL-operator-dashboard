//! Local SQLite file: the source of truth on this PC. Every synced table carries
//! `version` (bumped on each local change) and `synced_version` (last version the cloud has).

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

pub(crate) const MIGRATIONS: &[&str] = &[r#"
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
"#, r#"
-- v1.4: how the money was paid. Existing entries were all cash.
ALTER TABLE transactions ADD COLUMN payment_mode TEXT NOT NULL DEFAULT 'CASH' CHECK (payment_mode IN ('CASH', 'ONLINE', 'SYSTEM'));
"#, r#"
-- v1.5: several PCs in one office share data through the same cloud database.

-- Two PCs may add the same delivery boy while offline, so names are no longer UNIQUE in the
-- database (the app still refuses duplicates when adding on one PC).
CREATE TABLE delivery_boys_v5 (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL COLLATE NOCASE,
  is_active      INTEGER NOT NULL DEFAULT 1,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  version        INTEGER NOT NULL DEFAULT 1,
  synced_version INTEGER NOT NULL DEFAULT 0
);
INSERT INTO delivery_boys_v5 SELECT id, name, is_active, created_at, updated_at, version, synced_version FROM delivery_boys;
DROP TABLE delivery_boys;
ALTER TABLE delivery_boys_v5 RENAME TO delivery_boys;
CREATE INDEX delivery_boys_pending ON delivery_boys (id) WHERE version > synced_version;
CREATE INDEX delivery_boys_by_name ON delivery_boys (name);

-- Which PC made the entry (machine ID). NULL = this PC, before v1.5.
ALTER TABLE transactions ADD COLUMN created_by TEXT;

-- Day End is merged cell by cell between PCs: when each count was last typed, {"cell": iso}.
-- NULL = saved before v1.5 (every cell counts as changed at updated_at).
ALTER TABLE day_end_logs ADD COLUMN stamps TEXT;

-- The office's PCs, as last reported to the cloud.
CREATE TABLE machines (
  id           TEXT PRIMARY KEY,
  device_name  TEXT NOT NULL,
  app_version  TEXT,
  last_seen_at TEXT
);
"#, r#"
-- v1.6: who made each entry, and entries made for a delivery boy.

-- The delivery boy the entry is for (NULL = Office), with his name as it was then.
ALTER TABLE transactions ADD COLUMN role_id TEXT;
ALTER TABLE transactions ADD COLUMN role_name TEXT;
-- Delivery boy entries count cylinders instead of taking a consumer number.
ALTER TABLE transactions ADD COLUMN quantity INTEGER;
-- The operator's name from Settings (NULL = not set), when the entry was made / last changed.
ALTER TABLE transactions ADD COLUMN created_by_user TEXT;
ALTER TABLE transactions ADD COLUMN updated_by_user TEXT;

ALTER TABLE machines ADD COLUMN user_name TEXT;
"#, r#"
-- v1.8: what each delivery boy sold and handed in, per day (Day End page). One row per delivery
-- boy per day, shared by the office's PCs (id = "<date>:<delivery boy id>"); the last change wins.
CREATE TABLE delivery_sales (
  id                TEXT PRIMARY KEY,
  date_string       TEXT NOT NULL,
  delivery_boy_id   TEXT NOT NULL,
  delivery_boy_name TEXT NOT NULL,
  products          TEXT NOT NULL DEFAULT '[]',  -- [{"name", "quantity"}]
  cash              REAL NOT NULL DEFAULT 0,
  online            REAL NOT NULL DEFAULT 0,
  system            REAL NOT NULL DEFAULT 0,     -- total of system_payments
  system_payments   TEXT NOT NULL DEFAULT '[]',  -- [{"consumerNo", "name", "amount"}]
  expenses          TEXT NOT NULL DEFAULT '[]',  -- [{"kind": "FUEL"|"SERVICE"|"OTHER", "note", "amount"}]
  updated_by_user   TEXT,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL,
  version           INTEGER NOT NULL DEFAULT 1,
  synced_version    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX delivery_sales_by_date ON delivery_sales (date_string);
CREATE INDEX delivery_sales_by_boy ON delivery_sales (delivery_boy_id, date_string);
CREATE INDEX delivery_sales_pending ON delivery_sales (id) WHERE version > synced_version;

-- Settings shared by the office's PCs, e.g. "system_price" (the fixed price of a System payment).
-- The last change wins.
CREATE TABLE office_settings (
  id              TEXT PRIMARY KEY,
  value           TEXT NOT NULL,
  updated_by_user TEXT,
  updated_at      TEXT NOT NULL,
  version         INTEGER NOT NULL DEFAULT 1,
  synced_version  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX office_settings_pending ON office_settings (id) WHERE version > synced_version;
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
