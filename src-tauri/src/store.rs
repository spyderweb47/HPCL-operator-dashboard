//! Business rules over the local SQLite file. Everything the operator does is written here
//! first; the sync worker uploads it afterwards. Errors are messages meant for the operator.

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use crate::model::*;
use crate::time;

pub type R<T> = Result<T, String>;

pub fn db_err(e: rusqlite::Error) -> String {
    eprintln!("[db] {e}");
    "Could not read or write the data file on this PC. Please try again.".into()
}

const MAX_PRICE: f64 = 10_000_000.0;
const CONSUMER_NO_DIGITS: usize = 6;
const CONSUMER_REQUIRED: &str = "Enter the 6-digit consumer number, or tick “Other product”.";
const CONSUMER_INVALID: &str = "Consumer number must be exactly 6 digits.";
const INVALID_PRICE: &str = "Enter a valid price greater than 0.";

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// Collapse runs of whitespace, trim, cap at 120 characters.
pub fn clean_name(raw: &str) -> String {
    let joined = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(120).collect::<String>().trim_end().to_string()
}

/// `null`/"" -> Ok(None); a positive amount -> Ok(Some(rounded to paise)); anything else -> Err.
fn parse_price(v: &Value) -> Result<Option<f64>, ()> {
    let n = match v {
        Value::Null => return Ok(None),
        Value::String(s) if s.trim().is_empty() => return Ok(None),
        Value::String(s) => s.trim().parse::<f64>().map_err(|_| ())?,
        Value::Number(n) => n.as_f64().ok_or(())?,
        _ => return Err(()),
    };
    if !n.is_finite() || n <= 0.0 || n > MAX_PRICE {
        return Err(());
    }
    Ok(Some((n * 100.0).round() / 100.0))
}

/// Exactly 6 digits (spaces/dashes from copy-paste are stripped). Empty -> Ok(None).
fn parse_consumer_no(v: &Value) -> Result<Option<String>, ()> {
    let s = match v {
        Value::Null => return Ok(None),
        Value::String(s) => s.chars().filter(|c| !c.is_whitespace() && *c != '-').collect::<String>(),
        _ => return Err(()),
    };
    if s.is_empty() {
        return Ok(None);
    }
    if s.len() == CONSUMER_NO_DIGITS && s.chars().all(|c| c.is_ascii_digit()) {
        Ok(Some(s))
    } else {
        Err(())
    }
}

/// A non-negative whole count, capped.
fn count(v: &Value) -> i64 {
    let n = match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    };
    if n.is_finite() && n >= 0.0 {
        (n.floor() as i64).min(1_000_000)
    } else {
        0
    }
}

// ---------------------------------------------------------------------------------------------
// Transactions
// ---------------------------------------------------------------------------------------------

const TX_COLS: &str = "id, date_string, type, product_name, price, consumer_no, is_other_product, status, \
                       is_day_finalized, created_at, audit_log, version, synced_version";

fn tx_from_row(r: &Row) -> rusqlite::Result<TransactionDTO> {
    let audit: String = r.get(10)?;
    let version: i64 = r.get(11)?;
    let synced_version: i64 = r.get(12)?;
    Ok(TransactionDTO {
        id: r.get(0)?,
        date_string: r.get(1)?,
        kind: r.get(2)?,
        product_name: r.get(3)?,
        price: r.get(4)?,
        consumer_no: r.get(5)?,
        is_other_product: r.get(6)?,
        status: r.get(7)?,
        is_day_finalized: r.get(8)?,
        created_at: r.get(9)?,
        audit_log: serde_json::from_str(&audit).unwrap_or_default(),
        synced: version <= synced_version,
    })
}

pub fn transactions_for_date(conn: &Connection, date: &str) -> R<Vec<TransactionDTO>> {
    let mut stmt = conn
        .prepare_cached(&format!("SELECT {TX_COLS} FROM transactions WHERE date_string = ?1 ORDER BY created_at DESC"))
        .map_err(db_err)?;
    let rows = stmt.query_map([date], tx_from_row).map_err(db_err)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)
}

pub fn is_day_finalized(conn: &Connection, date: &str) -> R<bool> {
    conn.query_row("SELECT is_finalized FROM day_end_logs WHERE date_string = ?1", [date], |r| r.get::<_, bool>(0))
        .optional()
        .map(|v| v.unwrap_or(false))
        .map_err(db_err)
}

pub fn register(conn: &Connection) -> R<RegisterDTO> {
    let date = time::today();
    Ok(RegisterDTO {
        is_finalized: is_day_finalized(conn, &date)?,
        transactions: transactions_for_date(conn, &date)?,
        date,
    })
}

/// Adaptive catalog: create the product if new and remember the price just used (the next
/// entry's ghost price). Returns the stored spelling of the name.
fn touch_product(tx: &Transaction, name: &str, price: f64, now: &str) -> R<String> {
    tx.execute(
        "INSERT INTO products (id, name, last_used_price, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT(name) DO UPDATE SET last_used_price = excluded.last_used_price,
                                         updated_at = excluded.updated_at,
                                         version = version + 1
         WHERE last_used_price <> excluded.last_used_price",
        params![new_id(), name, price, now],
    )
    .map_err(db_err)?;
    tx.query_row("SELECT name FROM products WHERE name = ?1", [name], |r| r.get(0)).map_err(db_err)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTxInput {
    #[serde(rename = "type")]
    pub kind: String,
    pub product_name: String,
    /// Empty/absent = use the product's last price (the ghost-price fast path).
    #[serde(default)]
    pub price: Value,
    #[serde(default)]
    pub consumer_no: Value,
    #[serde(default)]
    pub is_other_product: bool,
}

pub fn create_transaction(conn: &mut Connection, input: CreateTxInput) -> R<TransactionDTO> {
    let kind = if input.kind == "CASH_OUT" { "CASH_OUT" } else { "CASH_IN" };
    let name = clean_name(&input.product_name);
    if name.is_empty() {
        return Err("Enter a product / item name.".into());
    }
    let mut price = parse_price(&input.price).map_err(|_| INVALID_PRICE.to_string())?;

    let is_cash_in = kind == "CASH_IN";
    let is_other = is_cash_in && input.is_other_product;
    let consumer_no = if is_cash_in {
        parse_consumer_no(&input.consumer_no).map_err(|_| CONSUMER_INVALID.to_string())?
    } else {
        None
    };
    if is_cash_in && !is_other && consumer_no.is_none() {
        return Err(CONSUMER_REQUIRED.into());
    }

    let date = time::today();
    let now = time::now_iso();
    let tx = conn.transaction().map_err(db_err)?;
    if is_day_finalized(&tx, &date)? {
        return Err("Today's register is finalized. No more entries can be added.".into());
    }
    if price.is_none() {
        let last: Option<f64> = tx
            .query_row("SELECT last_used_price FROM products WHERE name = ?1", [&name], |r| r.get(0))
            .optional()
            .map_err(db_err)?;
        price = Some(last.ok_or_else(|| format!("\"{name}\" is a new item — enter its price."))?);
    }
    let price = price.unwrap_or_default();
    let product_name = touch_product(&tx, &name, price, &now)?;

    let audit = vec![AuditEntry {
        action: "CREATED".into(),
        new_product_name: Some(product_name.clone()),
        new_price: Some(price),
        new_consumer_no: consumer_no.clone(),
        timestamp: now.clone(),
        ..Default::default()
    }];
    let id = new_id();
    tx.execute(
        "INSERT INTO transactions (id, date_string, type, product_name, price, consumer_no, is_other_product,
                                   status, audit_log, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'ACTIVE', ?8, ?9, ?9)",
        params![id, date, kind, product_name, price, consumer_no, is_other, serde_json::to_string(&audit).unwrap_or_default(), now],
    )
    .map_err(db_err)?;
    let dto = tx
        .query_row(&format!("SELECT {TX_COLS} FROM transactions WHERE id = ?1"), [&id], tx_from_row)
        .map_err(db_err)?;
    tx.commit().map_err(db_err)?;
    Ok(dto)
}

/// Only today's, non-finalized, non-deleted rows can change.
fn load_editable(tx: &Transaction, id: &str) -> R<TransactionDTO> {
    let row = tx
        .query_row(&format!("SELECT {TX_COLS} FROM transactions WHERE id = ?1"), [id], tx_from_row)
        .optional()
        .map_err(db_err)?
        .ok_or("Transaction not found.")?;
    if row.is_day_finalized || is_day_finalized(tx, &row.date_string)? {
        return Err("This day is finalized and can no longer be changed.".into());
    }
    if row.date_string != time::today() {
        return Err("Only today's entries can be changed.".into());
    }
    if row.status == "DELETED" {
        return Err("This entry was already deleted.".into());
    }
    Ok(row)
}

fn save_change(tx: &Transaction, row: &TransactionDTO, status: &str, entry: AuditEntry, now: &str) -> R<()> {
    let mut audit = row.audit_log.clone();
    audit.push(entry);
    tx.execute(
        "UPDATE transactions SET product_name = ?2, price = ?3, consumer_no = ?4, status = ?5, audit_log = ?6,
                                 updated_at = ?7, version = version + 1
         WHERE id = ?1",
        params![row.id, row.product_name, row.price, row.consumer_no, status, serde_json::to_string(&audit).unwrap_or_default(), now],
    )
    .map_err(db_err)?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTxInput {
    pub id: String,
    pub product_name: String,
    #[serde(default)]
    pub price: Value,
    #[serde(default)]
    pub consumer_no: Value,
}

pub fn update_transaction(conn: &mut Connection, input: UpdateTxInput) -> R<()> {
    let tx = conn.transaction().map_err(db_err)?;
    let old = load_editable(&tx, &input.id)?;

    let name = clean_name(&input.product_name);
    if name.is_empty() {
        return Err("Product / item name cannot be empty.".into());
    }
    let price = parse_price(&input.price).ok().flatten().ok_or(INVALID_PRICE)?;

    // Consumer number only applies to Cash In; the Other-product flag is fixed at entry time.
    let mut consumer_no = old.consumer_no.clone();
    if old.kind == "CASH_IN" {
        let parsed = parse_consumer_no(&input.consumer_no).map_err(|_| CONSUMER_INVALID.to_string())?;
        if parsed.is_none() && !old.is_other_product {
            return Err("Consumer number is required for this entry.".into());
        }
        consumer_no = parsed;
    }
    if name == old.product_name && price == old.price && consumer_no == old.consumer_no {
        return Ok(());
    }

    let now = time::now_iso();
    // Only touch the catalog when the product or price actually changed.
    let product_name = if name == old.product_name && price == old.price {
        old.product_name.clone()
    } else {
        touch_product(&tx, &name, price, &now)?
    };
    let entry = AuditEntry {
        action: "UPDATED".into(),
        previous_product_name: Some(old.product_name.clone()),
        new_product_name: Some(product_name.clone()),
        previous_price: Some(old.price),
        new_price: Some(price),
        previous_consumer_no: old.consumer_no.clone(),
        new_consumer_no: consumer_no.clone(),
        timestamp: now.clone(),
    };
    let new = TransactionDTO { product_name, price, consumer_no, ..old };
    save_change(&tx, &new, "EDITED", entry, &now)?;
    tx.commit().map_err(db_err)
}

/// Soft delete: the row stays, flagged and audited.
pub fn delete_transaction(conn: &mut Connection, id: &str) -> R<()> {
    let tx = conn.transaction().map_err(db_err)?;
    let row = load_editable(&tx, id)?;
    let now = time::now_iso();
    let entry = AuditEntry {
        action: "DELETED".into(),
        previous_product_name: Some(row.product_name.clone()),
        previous_price: Some(row.price),
        previous_consumer_no: row.consumer_no.clone(),
        timestamp: now.clone(),
        ..Default::default()
    };
    save_change(&tx, &row, "DELETED", entry, &now)?;
    tx.commit().map_err(db_err)
}

/// Prefix matches first, then names containing the text.
pub fn search_products(conn: &Connection, q: &str) -> R<Vec<ProductSuggestion>> {
    let q = clean_name(q);
    if q.is_empty() {
        return Ok(vec![]);
    }
    let escaped = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let mut stmt = conn
        .prepare_cached(
            "SELECT name, last_used_price FROM products
             WHERE name LIKE ?1 ESCAPE '\\'
             ORDER BY (name LIKE ?2 ESCAPE '\\') DESC, name COLLATE NOCASE
             LIMIT 8",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map([format!("%{escaped}%"), format!("{escaped}%")], |r| {
            Ok(ProductSuggestion { name: r.get(0)?, last_used_price: r.get(1)? })
        })
        .map_err(db_err)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)
}

// ---------------------------------------------------------------------------------------------
// Delivery boys (managed on this PC from Settings)
// ---------------------------------------------------------------------------------------------

pub fn list_delivery_boys(conn: &Connection) -> R<Vec<DeliveryBoyDTO>> {
    let mut stmt = conn
        .prepare_cached("SELECT id, name, is_active, created_at FROM delivery_boys ORDER BY is_active DESC, name COLLATE NOCASE")
        .map_err(db_err)?;
    let rows = stmt
        .query_map([], |r| Ok(DeliveryBoyDTO { id: r.get(0)?, name: r.get(1)?, is_active: r.get(2)?, created_at: r.get(3)? }))
        .map_err(db_err)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)
}

fn find_boy_by_name(conn: &Connection, name: &str) -> R<Option<(String, String, bool)>> {
    conn.query_row("SELECT id, name, is_active FROM delivery_boys WHERE name = ?1", [name], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })
    .optional()
    .map_err(db_err)
}

pub fn add_delivery_boy(conn: &Connection, raw: &str) -> R<DeliveryBoyDTO> {
    let name = clean_name(raw);
    if name.is_empty() {
        return Err("Enter the delivery boy's name.".into());
    }
    if let Some((_, existing, active)) = find_boy_by_name(conn, &name)? {
        return Err(if active {
            format!("“{existing}” is already in the list.")
        } else {
            format!("“{existing}” is already in the list but inactive. Reactivate them below.")
        });
    }
    let now = time::now_iso();
    let id = new_id();
    conn.execute(
        "INSERT INTO delivery_boys (id, name, is_active, created_at, updated_at) VALUES (?1, ?2, 1, ?3, ?3)",
        params![id, name, now],
    )
    .map_err(db_err)?;
    Ok(DeliveryBoyDTO { id, name, is_active: true, created_at: now })
}

pub fn rename_delivery_boy(conn: &Connection, id: &str, raw: &str) -> R<()> {
    let name = clean_name(raw);
    if name.is_empty() {
        return Err("Name cannot be empty.".into());
    }
    if let Some((other, existing, _)) = find_boy_by_name(conn, &name)? {
        if other != id {
            return Err(format!("“{existing}” is already in the list."));
        }
    }
    let changed = conn
        .execute(
            "UPDATE delivery_boys SET name = ?2, updated_at = ?3, version = version + 1 WHERE id = ?1 AND name IS NOT ?2",
            params![id, name, time::now_iso()],
        )
        .map_err(db_err)?;
    if changed == 0 && conn.query_row("SELECT 1 FROM delivery_boys WHERE id = ?1", [id], |_| Ok(())).optional().map_err(db_err)?.is_none() {
        return Err("Delivery boy not found.".into());
    }
    Ok(())
}

/// Deactivate instead of delete: past day-end logs keep pointing at them.
pub fn set_delivery_boy_active(conn: &Connection, id: &str, active: bool) -> R<()> {
    conn.execute(
        "UPDATE delivery_boys SET is_active = ?2, updated_at = ?3, version = version + 1 WHERE id = ?1 AND is_active <> ?2",
        params![id, active, time::now_iso()],
    )
    .map(|_| ())
    .map_err(db_err)
}

fn active_roster(conn: &Connection) -> R<Vec<(String, String)>> {
    let mut stmt = conn
        .prepare_cached("SELECT id, name FROM delivery_boys WHERE is_active = 1 ORDER BY name COLLATE NOCASE")
        .map_err(db_err)?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(db_err)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)
}

// ---------------------------------------------------------------------------------------------
// Day end
// ---------------------------------------------------------------------------------------------

pub fn get_day_end(conn: &Connection, date: &str) -> R<Option<DayEndDTO>> {
    conn.query_row(
        "SELECT date_string, is_finalized, finalized_at, deliveries, cylinder_stock FROM day_end_logs WHERE date_string = ?1",
        [date],
        |r| {
            let deliveries: String = r.get(3)?;
            let stock: String = r.get(4)?;
            Ok(DayEndDTO {
                date_string: r.get(0)?,
                is_finalized: r.get(1)?,
                finalized_at: r.get(2)?,
                deliveries: serde_json::from_str(&deliveries).unwrap_or_default(),
                cylinder_stock: serde_json::from_str(&stock).unwrap_or_default(),
            })
        },
    )
    .optional()
    .map_err(db_err)
}

fn blank_day_end(date: &str) -> DayEndDTO {
    DayEndDTO {
        date_string: date.to_string(),
        is_finalized: false,
        finalized_at: None,
        deliveries: vec![],
        cylinder_stock: CylinderStock::default(),
    }
}

/// Every active delivery boy (with today's saved counts, or zeros), plus anyone already saved
/// today who has since been deactivated, so no counts disappear.
fn merge_roster(roster: &[(String, String)], saved: &[DeliveryDTO]) -> Vec<DeliveryDTO> {
    let mut by_id: HashMap<&str, &DeliveryDTO> = saved.iter().map(|d| (d.delivery_boy_id.as_str(), d)).collect();
    let mut rows: Vec<DeliveryDTO> = roster
        .iter()
        .map(|(id, name)| {
            let s = by_id.remove(id.as_str());
            DeliveryDTO {
                delivery_boy_id: id.clone(),
                delivery_boy_name: name.clone(),
                bills_printed: s.map_or(0, |s| s.bills_printed),
                cylinders_delivered: s.map_or(0, |s| s.cylinders_delivered),
            }
        })
        .collect();
    rows.extend(saved.iter().filter(|d| by_id.contains_key(d.delivery_boy_id.as_str())).cloned());
    rows
}

pub fn day_end_page(conn: &Connection) -> R<DayEndPageDTO> {
    let date = time::today();
    let base = get_day_end(conn, &date)?.unwrap_or_else(|| blank_day_end(&date));
    // A finalized day shows exactly what was locked; an open day shows the live roster.
    let day_end = if base.is_finalized {
        base
    } else {
        DayEndDTO { deliveries: merge_roster(&active_roster(conn)?, &base.deliveries), ..base }
    };
    Ok(DayEndPageDTO { transactions: transactions_for_date(conn, &date)?, day_end, date })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryInput {
    pub delivery_boy_id: String,
    #[serde(default)]
    pub bills_printed: Value,
    #[serde(default)]
    pub cylinders_delivered: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayEndInput {
    #[serde(default)]
    pub deliveries: Vec<DeliveryInput>,
    #[serde(default)]
    pub cylinder_stock: Value,
}

/// Rows must be an active delivery boy (or someone already saved today who was deactivated
/// since). Names always come from the database, never from the form.
fn sanitize(conn: &Connection, input: &DayEndInput, saved: &[DeliveryDTO]) -> R<(Vec<DeliveryDTO>, CylinderStock)> {
    let mut allowed: HashMap<String, String> =
        saved.iter().map(|d| (d.delivery_boy_id.clone(), d.delivery_boy_name.clone())).collect();
    allowed.extend(active_roster(conn)?); // current name wins after a rename

    let mut seen = std::collections::HashSet::new();
    let mut deliveries = Vec::with_capacity(input.deliveries.len());
    for d in &input.deliveries {
        let name = allowed
            .get(&d.delivery_boy_id)
            .ok_or("A delivery boy in this list is no longer available. Reload the page.")?;
        if !seen.insert(d.delivery_boy_id.clone()) {
            return Err(format!("\"{name}\" is listed more than once."));
        }
        deliveries.push(DeliveryDTO {
            delivery_boy_id: d.delivery_boy_id.clone(),
            delivery_boy_name: name.clone(),
            bills_printed: count(&d.bills_printed),
            cylinders_delivered: count(&d.cylinders_delivered),
        });
    }
    let s = &input.cylinder_stock;
    let pair = |k: &str| StockPair { full: count(&s[k]["full"]), empty: count(&s[k]["empty"]) };
    Ok((deliveries, CylinderStock { kg5: pair("kg5"), kg14: pair("kg14"), kg19: pair("kg19") }))
}

fn save_today(tx: &Transaction, input: &DayEndInput, now: &str) -> R<String> {
    let date = time::today();
    let existing = get_day_end(tx, &date)?;
    if existing.as_ref().is_some_and(|e| e.is_finalized) {
        return Err("Today is already finalized.".into());
    }
    let saved = existing.map(|e| e.deliveries).unwrap_or_default();
    let (deliveries, stock) = sanitize(tx, input, &saved)?;
    tx.execute(
        "INSERT INTO day_end_logs (id, date_string, deliveries, cylinder_stock, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(date_string) DO UPDATE SET deliveries = excluded.deliveries,
                                                cylinder_stock = excluded.cylinder_stock,
                                                updated_at = excluded.updated_at,
                                                version = version + 1",
        params![
            new_id(),
            date,
            serde_json::to_string(&deliveries).unwrap_or_default(),
            serde_json::to_string(&stock).unwrap_or_default(),
            now
        ],
    )
    .map_err(db_err)?;
    Ok(date)
}

pub fn save_day_end(conn: &mut Connection, input: DayEndInput) -> R<Value> {
    let now = time::now_iso();
    let tx = conn.transaction().map_err(db_err)?;
    save_today(&tx, &input, &now)?;
    tx.commit().map_err(db_err)?;
    Ok(serde_json::json!({ "savedAt": now }))
}

/// Saves the final numbers, locks the day log, then locks every transaction of the day.
pub fn finalize_day(conn: &mut Connection, input: DayEndInput) -> R<()> {
    let now = time::now_iso();
    let tx = conn.transaction().map_err(db_err)?;
    let date = save_today(&tx, &input, &now)?;
    tx.execute(
        "UPDATE day_end_logs SET is_finalized = 1, finalized_at = ?2, updated_at = ?2, version = version + 1 WHERE date_string = ?1",
        params![date, now],
    )
    .map_err(db_err)?;
    tx.execute(
        "UPDATE transactions SET is_day_finalized = 1, updated_at = ?2, version = version + 1
         WHERE date_string = ?1 AND is_day_finalized = 0",
        params![date, now],
    )
    .map_err(db_err)?;
    tx.commit().map_err(db_err)
}

// ---------------------------------------------------------------------------------------------
// Housekeeping
// ---------------------------------------------------------------------------------------------

/// Rows changed on this PC that the cloud doesn't have yet.
pub fn pending_count(conn: &Connection) -> R<i64> {
    conn.query_row(
        "SELECT (SELECT COUNT(*) FROM transactions  WHERE version > synced_version)
              + (SELECT COUNT(*) FROM day_end_logs  WHERE version > synced_version)
              + (SELECT COUNT(*) FROM products      WHERE version > synced_version)
              + (SELECT COUNT(*) FROM delivery_boys WHERE version > synced_version)",
        [],
        |r| r.get(0),
    )
    .map_err(db_err)
}

/// Drop days older than the local window, but only rows the cloud already has.
pub fn prune(conn: &Connection, cutoff: &str) -> R<usize> {
    let n = conn
        .execute("DELETE FROM transactions WHERE date_string < ?1 AND version <= synced_version", [cutoff])
        .map_err(db_err)?
        + conn
            .execute("DELETE FROM day_end_logs WHERE date_string < ?1 AND version <= synced_version", [cutoff])
            .map_err(db_err)?;
    if n > 0 {
        crate::db::incremental_vacuum(conn).map_err(db_err)?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mem() -> Connection {
        let dir = std::env::temp_dir().join(format!("hpcl-test-{}.db", new_id()));
        crate::db::open(&dir).unwrap()
    }

    fn cash_in(conn: &mut Connection, product: &str, price: Value, consumer: &str) -> R<TransactionDTO> {
        create_transaction(
            conn,
            serde_json::from_value(json!({"type":"CASH_IN","productName":product,"price":price,"consumerNo":consumer})).unwrap(),
        )
    }

    #[test]
    fn entry_rules_and_ghost_price() {
        let mut c = mem();
        assert_eq!(cash_in(&mut c, "Regulator", json!("250"), "").unwrap_err(), CONSUMER_REQUIRED);
        assert_eq!(cash_in(&mut c, "Regulator", json!("250"), "12345").unwrap_err(), CONSUMER_INVALID);
        assert!(cash_in(&mut c, "Regulator", json!(null), "123456").unwrap_err().contains("new item"));
        let t = cash_in(&mut c, "  regulator  ", json!("250.555"), "123456").unwrap();
        assert_eq!((t.product_name.as_str(), t.price, t.synced), ("regulator", 250.56, false));
        // Ghost price + case-insensitive catalog keeps the first spelling.
        let t2 = cash_in(&mut c, "REGULATOR", json!(""), "654321").unwrap();
        assert_eq!((t2.product_name.as_str(), t2.price), ("regulator", 250.56));
        let other = create_transaction(
            &mut c,
            serde_json::from_value(json!({"type":"CASH_IN","productName":"Stove","price":900,"isOtherProduct":true})).unwrap(),
        )
        .unwrap();
        assert!(other.consumer_no.is_none() && other.is_other_product);
        let out = create_transaction(
            &mut c,
            serde_json::from_value(json!({"type":"CASH_OUT","productName":"Tea","price":"40","consumerNo":"123456"})).unwrap(),
        )
        .unwrap();
        assert_eq!(out.consumer_no, None);
        assert_eq!(search_products(&c, "reg").unwrap()[0].name, "regulator");
        assert_eq!(pending_count(&c).unwrap(), 4 + 3);
    }

    #[test]
    fn edit_delete_finalize() {
        let mut c = mem();
        let t = cash_in(&mut c, "Regulator", json!(250), "123456").unwrap();
        update_transaction(
            &mut c,
            serde_json::from_value(json!({"id":t.id,"productName":"Regulator","price":"300","consumerNo":"111111"})).unwrap(),
        )
        .unwrap();
        let rows = transactions_for_date(&c, &time::today()).unwrap();
        assert_eq!((rows[0].status.as_str(), rows[0].price, rows[0].audit_log.len()), ("EDITED", 300.0, 2));
        assert_eq!(rows[0].audit_log[1].previous_consumer_no.as_deref(), Some("123456"));

        let boy = add_delivery_boy(&c, "  Suresh   Patil ").unwrap();
        assert!(add_delivery_boy(&c, "suresh patil").is_err());
        let page = day_end_page(&c).unwrap();
        assert_eq!(page.day_end.deliveries.len(), 1);
        let input = json!({"deliveries":[{"deliveryBoyId":boy.id,"billsPrinted":"12","cylindersDelivered":11}],
                           "cylinderStock":{"kg14":{"full":40,"empty":"-3"}}});
        assert!(serde_json::from_value::<DayEndInput>(json!({"deliveries":[{"deliveryBoyId":"nope"}]}))
            .map(|i| save_day_end(&mut c, i).is_err())
            .unwrap());
        save_day_end(&mut c, serde_json::from_value(input.clone()).unwrap()).unwrap();
        delete_transaction(&mut c, &t.id).unwrap();
        assert!(delete_transaction(&mut c, &t.id).is_err());
        finalize_day(&mut c, serde_json::from_value(input.clone()).unwrap()).unwrap();
        let de = get_day_end(&c, &time::today()).unwrap().unwrap();
        assert!(de.is_finalized);
        assert_eq!((de.deliveries[0].bills_printed, de.cylinder_stock.kg14.full, de.cylinder_stock.kg14.empty), (12, 40, 0));
        assert!(cash_in(&mut c, "Regulator", json!(1), "123456").unwrap_err().contains("finalized"));
        assert!(finalize_day(&mut c, serde_json::from_value(input).unwrap()).is_err());
        assert!(transactions_for_date(&c, &time::today()).unwrap()[0].is_day_finalized);
    }

    #[test]
    fn prune_keeps_unsynced_and_recent_rows() {
        let c = mem();
        let old = time::shift_date(&time::today(), -(time::RETENTION_DAYS + 5));
        let insert = |id: &str, date: &str, synced: i64| {
            c.execute(
                "INSERT INTO transactions (id, date_string, type, product_name, price, created_at, updated_at, version, synced_version)
                 VALUES (?1, ?2, 'CASH_IN', 'X', 1, ?2, ?2, 1, ?3)",
                params![id, date, synced],
            )
            .unwrap();
        };
        insert("old-synced", &old, 1);
        insert("old-unsynced", &old, 0);
        insert("recent-synced", &time::today(), 1);
        assert_eq!(prune(&c, &time::retention_cutoff(&time::today())).unwrap(), 1);
        let left: Vec<String> = c
            .prepare("SELECT id FROM transactions ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(left, ["old-unsynced", "recent-synced"]);
    }
}
