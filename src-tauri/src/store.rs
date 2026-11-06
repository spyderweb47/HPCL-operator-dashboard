//! Business rules over the local SQLite file. Everything the operator does is written here
//! first; the sync worker uploads it afterwards. Errors are messages meant for the operator.

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
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
/// Other products take free text in the Consumer No box (a name, phone number, note…).
const OTHER_NOTE_MAX: usize = 60;
const INVALID_PRICE: &str = "Enter a valid price greater than 0.";
const QUANTITY_MAX: i64 = 9999;
const QUANTITY_INVALID: &str = "Quantity must be a whole number from 1 to 9999.";
const USER_NAME_MAX: usize = 40;

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

/// Other products: any text, spaces tidied, capped at OTHER_NOTE_MAX characters. Empty -> Ok(None).
fn parse_other_note(v: &Value) -> Result<Option<String>, ()> {
    let s = match v {
        Value::Null => return Ok(None),
        Value::String(s) => s.split_whitespace().collect::<Vec<_>>().join(" "),
        Value::Number(n) => n.to_string(),
        _ => return Err(()),
    };
    Ok((!s.is_empty()).then(|| s.chars().take(OTHER_NOTE_MAX).collect()))
}

/// What goes in the Consumer No column: 6 digits normally, free text for Other products.
fn parse_consumer_field(v: &Value, is_other: bool) -> R<Option<String>> {
    if is_other {
        parse_other_note(v).map_err(|_| "Enter the note as text.".to_string())
    } else {
        parse_consumer_no(v).map_err(|_| CONSUMER_INVALID.to_string())
    }
}

pub const PAYMENT_MODES: [&str; 3] = ["CASH", "ONLINE", "SYSTEM"];

/// Missing/empty -> Cash, the default.
fn parse_payment_mode(v: Option<&str>) -> R<&'static str> {
    let v = v.map(str::trim).filter(|v| !v.is_empty()).unwrap_or("CASH");
    PAYMENT_MODES.into_iter().find(|m| m.eq_ignore_ascii_case(v)).ok_or_else(|| format!("Unknown payment type \"{v}\"."))
}

/// Quantity on delivery boy entries made with v1.6–1.7 (editable on those rows only).
/// A whole number from 1 to QUANTITY_MAX. Empty -> Ok(None).
fn parse_quantity(v: &Value) -> R<Option<i64>> {
    let n = match v {
        Value::Null => return Ok(None),
        Value::String(s) if s.trim().is_empty() => return Ok(None),
        Value::String(s) => s.trim().parse::<i64>().map_err(|_| QUANTITY_INVALID.to_string())?,
        Value::Number(n) => n.as_i64().ok_or(QUANTITY_INVALID)?,
        _ => return Err(QUANTITY_INVALID.into()),
    };
    if (1..=QUANTITY_MAX).contains(&n) {
        Ok(Some(n))
    } else {
        Err(QUANTITY_INVALID.into())
    }
}

// ---------------------------------------------------------------------------------------------
// The operator's name (Settings): recorded on every entry and change, so the cloud shows who did what
// ---------------------------------------------------------------------------------------------

pub fn user_name(conn: &Connection) -> R<Option<String>> {
    crate::db::meta_get(conn, "user_name").map_err(db_err)
}

/// Empty clears it.
pub fn set_user_name(conn: &Connection, raw: &str) -> R<Option<String>> {
    let name: String = clean_name(raw).chars().take(USER_NAME_MAX).collect::<String>().trim_end().to_string();
    if name.is_empty() {
        conn.execute("DELETE FROM meta WHERE key = 'user_name'", []).map_err(db_err)?;
        return Ok(None);
    }
    crate::db::meta_set(conn, "user_name", &name).map_err(db_err)?;
    Ok(Some(name))
}

/// Who is making a change on this PC, for the audit trail.
fn author(conn: &Connection) -> R<(Option<String>, Option<String>)> {
    Ok((user_name(conn)?, crate::db::meta_get(conn, "machine_id").map_err(db_err)?))
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
// Office settings (shared by the office's PCs; the last change wins)
// ---------------------------------------------------------------------------------------------

const SYSTEM_PRICE_KEY: &str = "system_price";
pub const SYSTEM_PRICE_MISSING: &str = "Set the System payment price in Settings first.";

/// The fixed price of a System payment, if set.
pub fn system_price(conn: &Connection) -> R<Option<f64>> {
    let v: Option<String> = conn
        .query_row("SELECT value FROM office_settings WHERE id = ?1", [SYSTEM_PRICE_KEY], |r| r.get(0))
        .optional()
        .map_err(db_err)?;
    Ok(v.and_then(|v| v.parse::<f64>().ok()).filter(|p| *p > 0.0))
}

pub fn office_settings(conn: &Connection) -> R<OfficeSettingsDTO> {
    Ok(OfficeSettingsDTO { system_price: system_price(conn)? })
}

/// New System payments take this price; ones already saved keep theirs.
pub fn set_system_price(conn: &Connection, v: &Value) -> R<OfficeSettingsDTO> {
    let price = parse_price(v).ok().flatten().ok_or(INVALID_PRICE)?;
    if system_price(conn)? != Some(price) {
        conn.execute(
            "INSERT INTO office_settings (id, value, updated_by_user, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET value = excluded.value, updated_by_user = excluded.updated_by_user,
                                           updated_at = excluded.updated_at, version = version + 1",
            params![SYSTEM_PRICE_KEY, price.to_string(), user_name(conn)?, time::now_iso()],
        )
        .map_err(db_err)?;
    }
    office_settings(conn)
}

/// A setting changed on another PC. Returns true when this PC's value changed.
pub fn apply_remote_setting(tx: &Transaction, id: &str, value: &str, user: Option<&str>, updated_at: &str) -> R<bool> {
    let local: Option<String> =
        tx.query_row("SELECT updated_at FROM office_settings WHERE id = ?1", [id], |r| r.get(0)).optional().map_err(db_err)?;
    if local.as_deref().is_some_and(|l| !newer(updated_at, l)) {
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO office_settings (id, value, updated_by_user, updated_at, version, synced_version) VALUES (?1, ?2, ?3, ?4, 1, 1)
         ON CONFLICT(id) DO UPDATE SET value = excluded.value, updated_by_user = excluded.updated_by_user,
                                       updated_at = excluded.updated_at, synced_version = version",
        params![id, value, user, updated_at],
    )
    .map_err(db_err)?;
    Ok(true)
}

// ---------------------------------------------------------------------------------------------
// Transactions
// ---------------------------------------------------------------------------------------------

/// `created_by` NULL = made on this PC before v1.5; the device name comes from the office PC list.
const TX_COLS: &str = "id, date_string, type, product_name, price, consumer_no, is_other_product, status, \
                       is_day_finalized, created_at, audit_log, version, synced_version, payment_mode, \
                       COALESCE(created_by, (SELECT value FROM meta WHERE key = 'machine_id')), \
                       (SELECT device_name FROM machines \
                         WHERE machines.id = COALESCE(transactions.created_by, (SELECT value FROM meta WHERE key = 'machine_id'))), \
                       created_by_user, updated_by_user, role_id, role_name, quantity";

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
        payment_mode: r.get(13)?,
        created_by: r.get(14)?,
        created_by_name: r.get(15)?,
        created_by_user: r.get(16)?,
        updated_by_user: r.get(17)?,
        role_id: r.get(18)?,
        role_name: r.get(19)?,
        quantity: r.get(20)?,
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

/// System payments have a fixed price: add the product to the catalog if it's new, but don't
/// make the fixed price its ghost price.
fn add_product(tx: &Transaction, name: &str, price: f64, now: &str) -> R<String> {
    tx.execute(
        "INSERT INTO products (id, name, last_used_price, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT(name) DO NOTHING",
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
    /// "CASH" (default) | "ONLINE" | "SYSTEM". A System Cash In always takes the fixed System price.
    #[serde(default)]
    pub payment_mode: Option<String>,
}

pub fn create_transaction(conn: &mut Connection, input: CreateTxInput) -> R<TransactionDTO> {
    let kind = if input.kind == "CASH_OUT" { "CASH_OUT" } else { "CASH_IN" };
    let name = clean_name(&input.product_name);
    if name.is_empty() {
        return Err("Enter a product / item name.".into());
    }
    let mut price = parse_price(&input.price).map_err(|_| INVALID_PRICE.to_string())?;
    let payment_mode = parse_payment_mode(input.payment_mode.as_deref())?;

    let is_cash_in = kind == "CASH_IN";
    let is_other = is_cash_in && input.is_other_product;
    let consumer_no = if is_cash_in {
        parse_consumer_field(&input.consumer_no, is_other)?
    } else {
        None
    };
    if is_cash_in && !is_other && consumer_no.is_none() {
        return Err(CONSUMER_REQUIRED.into());
    }
    let (user, machine) = author(conn)?;

    let date = time::today();
    let now = time::now_iso();
    let tx = conn.transaction().map_err(db_err)?;
    if is_day_finalized(&tx, &date)? {
        return Err("Today's register is finalized. No more entries can be added.".into());
    }
    let fixed = is_cash_in && payment_mode == "SYSTEM";
    if fixed {
        price = Some(system_price(&tx)?.ok_or(SYSTEM_PRICE_MISSING)?);
    } else if price.is_none() {
        let last: Option<f64> = tx
            .query_row("SELECT last_used_price FROM products WHERE name = ?1", [&name], |r| r.get(0))
            .optional()
            .map_err(db_err)?;
        price = Some(last.ok_or_else(|| format!("\"{name}\" is a new item — enter its price."))?);
    }
    let price = price.unwrap_or_default();
    let product_name = if fixed { add_product(&tx, &name, price, &now)? } else { touch_product(&tx, &name, price, &now)? };

    let audit = vec![AuditEntry {
        action: "CREATED".into(),
        new_product_name: Some(product_name.clone()),
        new_price: Some(price),
        new_consumer_no: consumer_no.clone(),
        new_payment_mode: Some(payment_mode.into()),
        user: user.clone(),
        machine_id: machine,
        timestamp: now.clone(),
        ..Default::default()
    }];
    let id = new_id();
    tx.execute(
        "INSERT INTO transactions (id, date_string, type, product_name, price, consumer_no, is_other_product,
                                   status, audit_log, created_at, updated_at, payment_mode, created_by_user, updated_by_user)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'ACTIVE', ?8, ?9, ?9, ?10, ?11, ?11)",
        params![
            id, date, kind, product_name, price, consumer_no, is_other, serde_json::to_string(&audit).unwrap_or_default(), now,
            payment_mode, user
        ],
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
    let (user, machine) = author(tx)?;
    let mut audit = row.audit_log.clone();
    audit.push(AuditEntry { user: user.clone(), machine_id: machine, ..entry });
    tx.execute(
        "UPDATE transactions SET product_name = ?2, price = ?3, consumer_no = ?4, status = ?5, audit_log = ?6,
                                 updated_at = ?7, payment_mode = ?8, quantity = ?9, updated_by_user = ?10, version = version + 1
         WHERE id = ?1",
        params![
            row.id, row.product_name, row.price, row.consumer_no, status, serde_json::to_string(&audit).unwrap_or_default(), now,
            row.payment_mode, row.quantity, user
        ],
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
    /// Absent = keep the entry's payment type.
    #[serde(default)]
    pub payment_mode: Option<String>,
    /// Delivery boy Cash In entries only.
    #[serde(default)]
    pub quantity: Value,
}

pub fn update_transaction(conn: &mut Connection, input: UpdateTxInput) -> R<()> {
    let tx = conn.transaction().map_err(db_err)?;
    let old = load_editable(&tx, &input.id)?;

    let name = clean_name(&input.product_name);
    if name.is_empty() {
        return Err("Product / item name cannot be empty.".into());
    }
    let payment_mode = match input.payment_mode.as_deref() {
        None => old.payment_mode.clone(),
        v => parse_payment_mode(v)?.to_string(),
    };
    // A System Cash In has the fixed price: kept as it was, or the current one when it becomes System.
    let fixed = old.kind == "CASH_IN" && payment_mode == "SYSTEM";
    let price = if fixed && old.payment_mode == "SYSTEM" {
        old.price
    } else if fixed {
        system_price(&tx)?.ok_or(SYSTEM_PRICE_MISSING)?
    } else {
        parse_price(&input.price).ok().flatten().ok_or(INVALID_PRICE)?
    };

    // Consumer number only applies to Cash In; the Other-product flag is fixed at entry time.
    // Other products keep free text there, which can be added, changed or cleared.
    // Delivery boy entries from v1.6–1.7 have a quantity there instead.
    let mut consumer_no = old.consumer_no.clone();
    let mut quantity = old.quantity;
    if old.kind == "CASH_IN" && old.role_id.is_some() {
        quantity = parse_quantity(&input.quantity)?;
        if quantity.is_none() && !old.is_other_product {
            return Err("Quantity is required for this entry.".into());
        }
    } else if old.kind == "CASH_IN" {
        let parsed = parse_consumer_field(&input.consumer_no, old.is_other_product)?;
        if parsed.is_none() && !old.is_other_product {
            return Err("Consumer number is required for this entry.".into());
        }
        consumer_no = parsed;
    }
    if name == old.product_name
        && price == old.price
        && consumer_no == old.consumer_no
        && payment_mode == old.payment_mode
        && quantity == old.quantity
    {
        return Ok(());
    }

    let now = time::now_iso();
    // Only touch the catalog when the product or price actually changed.
    let product_name = if name == old.product_name && price == old.price {
        old.product_name.clone()
    } else if fixed {
        add_product(&tx, &name, price, &now)?
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
        previous_payment_mode: Some(old.payment_mode.clone()),
        new_payment_mode: Some(payment_mode.clone()),
        previous_quantity: old.quantity,
        new_quantity: quantity,
        timestamp: now.clone(),
        ..Default::default()
    };
    let new = TransactionDTO { product_name, price, consumer_no, payment_mode, quantity, ..old };
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
        previous_payment_mode: Some(row.payment_mode.clone()),
        previous_quantity: row.quantity,
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
    // A finalized day shows what was saved; an open day has a sheet for every active delivery boy.
    let sales = if day_end.is_finalized { sales_for_date(conn, &date)? } else { sales_sheet(conn, &date)? };
    Ok(DayEndPageDTO { transactions: transactions_for_date(conn, &date)?, day_end, sales, date })
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
    /// The counts the form last loaded or saved. Only cells the operator changed since then are
    /// written, so counts typed meanwhile on another PC are not overwritten. Absent = all cells.
    #[serde(default)]
    pub base: Option<DayEndBase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayEndBase {
    #[serde(default)]
    pub deliveries: Vec<DeliveryInput>,
    #[serde(default)]
    pub cylinder_stock: Value,
}

// Day End counts are merged between PCs one cell at a time ("bills:<boy id>", "cyl:<boy id>",
// "kg14.full", …): each cell keeps the value typed last, wherever it was typed.

/// When each cell was last typed (ISO time).
pub type Stamps = BTreeMap<String, String>;
type Cells = BTreeMap<String, i64>;

const STOCK_CELLS: [&str; 3] = ["kg5", "kg14", "kg19"];

fn stock_pair(s: &CylinderStock, k: &str) -> StockPair {
    match k {
        "kg5" => s.kg5,
        "kg14" => s.kg14,
        _ => s.kg19,
    }
}

fn to_cells(deliveries: &[DeliveryDTO], stock: &CylinderStock) -> Cells {
    let mut c = Cells::new();
    for d in deliveries {
        c.insert(format!("bills:{}", d.delivery_boy_id), d.bills_printed);
        c.insert(format!("cyl:{}", d.delivery_boy_id), d.cylinders_delivered);
    }
    for k in STOCK_CELLS {
        let p = stock_pair(stock, k);
        c.insert(format!("{k}.full"), p.full);
        c.insert(format!("{k}.empty"), p.empty);
    }
    c
}

/// Back to the stored shape. Rows are sorted by name so equal data always looks equal.
fn from_cells(c: &Cells, names: &HashMap<String, String>) -> (Vec<DeliveryDTO>, CylinderStock) {
    let get = |k: &str| c.get(k).copied().unwrap_or(0);
    let mut deliveries: Vec<DeliveryDTO> = c
        .keys()
        .filter_map(|k| k.strip_prefix("bills:").or_else(|| k.strip_prefix("cyl:")))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|id| DeliveryDTO {
            delivery_boy_id: id.to_string(),
            delivery_boy_name: names.get(id).cloned().unwrap_or_default(),
            bills_printed: get(&format!("bills:{id}")),
            cylinders_delivered: get(&format!("cyl:{id}")),
        })
        .collect();
    deliveries.sort_by(|a, b| a.delivery_boy_name.to_lowercase().cmp(&b.delivery_boy_name.to_lowercase()).then(a.delivery_boy_id.cmp(&b.delivery_boy_id)));
    let pair = |k: &str| StockPair { full: get(&format!("{k}.full")), empty: get(&format!("{k}.empty")) };
    (deliveries, CylinderStock { kg5: pair("kg5"), kg14: pair("kg14"), kg19: pair("kg19") })
}

/// A day-end log as stored: on this PC, or one PC's copy in the cloud.
#[derive(Clone, Debug, PartialEq)]
pub struct DayCopy {
    pub day: DayEndDTO,
    /// None = saved before v1.5: every cell counts as typed at `updated_at`.
    pub stamps: Option<Stamps>,
    pub updated_at: String,
}

impl DayCopy {
    fn cells(&self) -> Cells {
        to_cells(&self.day.deliveries, &self.day.cylinder_stock)
    }
    fn names(&self) -> HashMap<String, String> {
        self.day.deliveries.iter().map(|d| (d.delivery_boy_id.clone(), d.delivery_boy_name.clone())).collect()
    }
    /// When a cell was typed; -1 = never (a zero nobody entered).
    fn stamp(&self, cell: &str) -> i64 {
        match &self.stamps {
            None => time::iso_to_millis(&self.updated_at),
            Some(s) => s.get(cell).map_or(-1, |t| time::iso_to_millis(t)),
        }
    }
}

/// Combine two copies of the same day. The same inputs give the same result on every PC.
pub fn merge_day(a: &DayCopy, b: &DayCopy) -> DayCopy {
    let (ca, cb) = (a.cells(), b.cells());
    let mut cells = Cells::new();
    let mut stamps = Stamps::new();
    for k in ca.keys().chain(cb.keys()) {
        if cells.contains_key(k) {
            continue;
        }
        let pick = |c: &DayCopy, v: Option<&i64>| v.map(|v| (c.stamp(k), *v));
        let (stamp, value) = match (pick(a, ca.get(k)), pick(b, cb.get(k))) {
            (Some(x), Some(y)) => x.max(y), // newer wins; a tie goes to the larger count
            (x, y) => x.or(y).unwrap_or((-1, 0)),
        };
        cells.insert(k.clone(), value);
        if stamp >= 0 {
            stamps.insert(k.clone(), time::millis_to_iso(stamp));
        }
    }
    let mut names = b.names();
    names.extend(a.names()); // a = this PC: its (current) names win
    let (deliveries, cylinder_stock) = from_cells(&cells, &names);
    let finalized_at = match (&a.day.finalized_at, &b.day.finalized_at) {
        (Some(x), Some(y)) => Some(x.min(y).clone()),
        (x, y) => x.clone().or(y.clone()),
    };
    DayCopy {
        day: DayEndDTO {
            date_string: a.day.date_string.clone(),
            is_finalized: a.day.is_finalized || b.day.is_finalized,
            finalized_at,
            deliveries,
            cylinder_stock,
        },
        stamps: Some(stamps),
        updated_at: a.updated_at.clone().max(b.updated_at.clone()),
    }
}

fn load_day(conn: &Connection, date: &str) -> R<Option<DayCopy>> {
    let row = conn
        .query_row("SELECT stamps, updated_at FROM day_end_logs WHERE date_string = ?1", [date], |r| {
            Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?))
        })
        .optional()
        .map_err(db_err)?;
    let Some((stamps, updated_at)) = row else { return Ok(None) };
    let day = get_day_end(conn, date)?.unwrap_or_else(|| blank_day_end(date));
    Ok(Some(DayCopy { day, stamps: stamps.and_then(|s| serde_json::from_str(&s).ok()), updated_at }))
}

/// Write a day's merged state. `upload`: whether the cloud needs this PC's copy again.
fn store_day(tx: &Transaction, d: &DayCopy, upload: bool) -> R<()> {
    let day = &d.day;
    let stamps = d.stamps.as_ref().map(|s| serde_json::to_string(s).unwrap_or_default());
    tx.execute(
        "INSERT INTO day_end_logs (id, date_string, deliveries, cylinder_stock, is_finalized, finalized_at, stamps,
                                   created_at, updated_at, version, synced_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1, ?9)
         ON CONFLICT(date_string) DO UPDATE SET deliveries = excluded.deliveries,
                                                cylinder_stock = excluded.cylinder_stock,
                                                is_finalized = excluded.is_finalized,
                                                finalized_at = excluded.finalized_at,
                                                stamps = excluded.stamps,
                                                updated_at = excluded.updated_at,
                                                version = version + 1,
                                                synced_version = CASE WHEN ?10 THEN synced_version ELSE version + 1 END",
        params![
            new_id(),
            day.date_string,
            serde_json::to_string(&day.deliveries).unwrap_or_default(),
            serde_json::to_string(&day.cylinder_stock).unwrap_or_default(),
            day.is_finalized,
            day.finalized_at,
            stamps,
            d.updated_at,
            if upload { 0 } else { 1 },
            upload
        ],
    )
    .map_err(db_err)?;
    if day.is_finalized {
        // Lock the day's entries on this PC too (a flag derived from the day, not a change to upload).
        tx.execute("UPDATE transactions SET is_day_finalized = 1 WHERE date_string = ?1 AND is_day_finalized = 0", [&day.date_string])
            .map_err(db_err)?;
    }
    Ok(())
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
    Ok((deliveries, parse_stock(&input.cylinder_stock)))
}

fn parse_stock(s: &Value) -> CylinderStock {
    let pair = |k: &str| StockPair { full: count(&s[k]["full"]), empty: count(&s[k]["empty"]) };
    CylinderStock { kg5: pair("kg5"), kg14: pair("kg14"), kg19: pair("kg19") }
}

fn base_cells(b: &DayEndBase) -> Cells {
    let deliveries: Vec<DeliveryDTO> = b
        .deliveries
        .iter()
        .map(|d| DeliveryDTO {
            delivery_boy_id: d.delivery_boy_id.clone(),
            delivery_boy_name: String::new(),
            bills_printed: count(&d.bills_printed),
            cylinders_delivered: count(&d.cylinders_delivered),
        })
        .collect();
    to_cells(&deliveries, &parse_stock(&b.cylinder_stock))
}

/// Apply the operator's changes to today's log. Returns false when nothing changed.
fn save_today(tx: &Transaction, input: &DayEndInput, now: &str) -> R<(String, bool)> {
    let date = time::today();
    let existing = load_day(tx, &date)?;
    if existing.as_ref().is_some_and(|e| e.day.is_finalized) {
        return Err("Today is already finalized.".into());
    }
    let saved = existing.as_ref().map(|e| e.day.deliveries.clone()).unwrap_or_default();
    let (deliveries, stock) = sanitize(tx, input, &saved)?;
    let typed = to_cells(&deliveries, &stock);
    let base = input.base.as_ref().map(base_cells);

    let mut cells = existing.as_ref().map(|e| e.cells()).unwrap_or_default();
    // Logs from before v1.5: give every existing cell the log's own time.
    let mut stamps = match &existing {
        Some(DayCopy { stamps: Some(s), .. }) => s.clone(),
        Some(e) => cells.keys().map(|k| (k.clone(), e.updated_at.clone())).collect(),
        None => Stamps::new(),
    };
    let mut changed = existing.is_none() || existing.as_ref().is_some_and(|e| e.stamps.is_none());
    for (k, v) in &typed {
        let operator_changed = base.as_ref().is_none_or(|b| b.get(k).copied().unwrap_or(0) != *v);
        if operator_changed && cells.get(k) != Some(v) {
            cells.insert(k.clone(), *v);
            stamps.insert(k.clone(), now.to_string());
            changed = true;
        } else if !cells.contains_key(k) {
            cells.insert(k.clone(), *v); // a row nobody has typed in yet
            changed = true;
        }
    }
    let mut names = existing.as_ref().map(|e| e.names()).unwrap_or_default();
    for d in &deliveries {
        if names.get(&d.delivery_boy_id) != Some(&d.delivery_boy_name) {
            names.insert(d.delivery_boy_id.clone(), d.delivery_boy_name.clone());
            changed = true;
        }
    }
    if !changed {
        return Ok((date, false));
    }
    let (deliveries, cylinder_stock) = from_cells(&cells, &names);
    let day = DayEndDTO { date_string: date.clone(), is_finalized: false, finalized_at: None, deliveries, cylinder_stock };
    store_day(tx, &DayCopy { day, stamps: Some(stamps), updated_at: now.to_string() }, true)?;
    Ok((date, true))
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
    let (date, _) = save_today(&tx, &input, &now)?;
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
// Delivery boy sales (Day End page): per delivery boy per day, what he sold, the money he
// reported (cash / online / system, for all his sales together) and what he spent from it.
// One row per delivery boy per day, shared by the office's PCs; the last change wins.
// ---------------------------------------------------------------------------------------------

/// Offered on every card even before they've been used.
const STARTER_EXPENSES: [&str; 2] = ["Fuel", "Service"];
const EXPENSE_NAME_MAX: usize = 40;
const SALES_LINES_MAX: usize = 10;
const SYSTEM_LINES_MAX: usize = 50;
const MONEY_INVALID: &str = "Enter amounts as numbers, like 1500 or 1500.50.";
const SOLD_INVALID: &str = "Quantity sold must be a whole number up to 9999.";

pub fn sales_id(date: &str, boy_id: &str) -> String {
    format!("{date}:{boy_id}")
}

/// Empty -> 0; otherwise a non-negative amount, rounded to paise.
fn parse_money(v: &Value) -> R<f64> {
    let n = match v {
        Value::Null => return Ok(0.0),
        Value::String(s) if s.trim().is_empty() => return Ok(0.0),
        Value::String(s) => s.trim().parse::<f64>().map_err(|_| MONEY_INVALID.to_string())?,
        Value::Number(n) => n.as_f64().ok_or(MONEY_INVALID)?,
        _ => return Err(MONEY_INVALID.into()),
    };
    if !n.is_finite() || !(0.0..=MAX_PRICE).contains(&n) {
        return Err(MONEY_INVALID.into());
    }
    Ok((n * 100.0).round() / 100.0)
}

/// Empty -> 0; otherwise a whole number from 0 to QUANTITY_MAX.
fn parse_sold(v: &Value) -> R<i64> {
    let n = match v {
        Value::Null => return Ok(0),
        Value::String(s) if s.trim().is_empty() => return Ok(0),
        Value::String(s) => s.trim().parse::<i64>().map_err(|_| SOLD_INVALID.to_string())?,
        Value::Number(n) => n.as_i64().ok_or(SOLD_INVALID)?,
        _ => return Err(SOLD_INVALID.into()),
    };
    if (0..=QUANTITY_MAX).contains(&n) {
        Ok(n)
    } else {
        Err(SOLD_INVALID.into())
    }
}

const SALES_COLS: &str =
    "delivery_boy_id, delivery_boy_name, products, cash, online, system, expenses, updated_by_user, updated_at, system_payments";

fn sales_from_row(r: &Row) -> rusqlite::Result<DeliverySalesDTO> {
    Ok(DeliverySalesDTO {
        delivery_boy_id: r.get(0)?,
        delivery_boy_name: r.get(1)?,
        products: serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default(),
        cash: r.get(3)?,
        online: r.get(4)?,
        system: r.get(5)?,
        system_payments: serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or_default(),
        expenses: serde_json::from_str(&r.get::<_, String>(6)?).unwrap_or_default(),
        usual_product: None,
        updated_by_user: r.get(7)?,
        updated_at: Some(r.get(8)?),
    })
}

/// What was saved for a day, by delivery boy name.
pub fn sales_for_date(conn: &Connection, date: &str) -> R<Vec<DeliverySalesDTO>> {
    let mut stmt = conn
        .prepare_cached(&format!(
            "SELECT {SALES_COLS} FROM delivery_sales WHERE date_string = ?1 ORDER BY delivery_boy_name COLLATE NOCASE"
        ))
        .map_err(db_err)?;
    let rows = stmt.query_map([date], sales_from_row).map_err(db_err)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)
}

/// The product a delivery boy sold most recently before `date`: offered first on his card.
fn usual_product(conn: &Connection, boy_id: &str, date: &str) -> R<Option<String>> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT products FROM delivery_sales WHERE delivery_boy_id = ?1 AND date_string < ?2
             ORDER BY date_string DESC LIMIT 30",
        )
        .map_err(db_err)?;
    let rows = stmt.query_map(params![boy_id, date], |r| r.get::<_, String>(0)).map_err(db_err)?;
    for products in rows {
        let products: Vec<SoldProduct> = serde_json::from_str(&products.map_err(db_err)?).unwrap_or_default();
        if let Some(p) = products.into_iter().find(|p| p.quantity > 0) {
            return Ok(Some(p.name));
        }
    }
    Ok(None)
}

/// An open day's sheet: every active delivery boy (blank until something is typed), plus anyone
/// already saved today who was deactivated since.
fn sales_sheet(conn: &Connection, date: &str) -> R<Vec<DeliverySalesDTO>> {
    let mut saved: HashMap<String, DeliverySalesDTO> =
        sales_for_date(conn, date)?.into_iter().map(|s| (s.delivery_boy_id.clone(), s)).collect();
    let mut sheet = Vec::new();
    for (id, name) in active_roster(conn)? {
        let row = saved.remove(&id).unwrap_or_else(|| DeliverySalesDTO {
            delivery_boy_id: id.clone(),
            delivery_boy_name: String::new(),
            products: vec![],
            cash: 0.0,
            online: 0.0,
            system: 0.0,
            system_payments: vec![],
            expenses: vec![],
            usual_product: None,
            updated_by_user: None,
            updated_at: None,
        });
        sheet.push(DeliverySalesDTO { delivery_boy_name: name, ..row }); // current name after a rename
    }
    sheet.extend(saved.into_values());
    for row in &mut sheet {
        row.usual_product = usual_product(conn, &row.delivery_boy_id, date)?;
    }
    sheet.sort_by_key(|r| r.delivery_boy_name.to_lowercase());
    Ok(sheet)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoldInput {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub quantity: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseInput {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub amount: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPaymentInput {
    #[serde(default)]
    pub consumer_no: Value,
    #[serde(default)]
    pub name: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesInput {
    pub delivery_boy_id: String,
    #[serde(default)]
    pub products: Vec<SoldInput>,
    #[serde(default)]
    pub cash: Value,
    #[serde(default)]
    pub online: Value,
    /// One line per system payment; the System total is their sum.
    #[serde(default)]
    pub system_payments: Vec<SystemPaymentInput>,
    #[serde(default)]
    pub expenses: Vec<ExpenseInput>,
}

/// Save one delivery boy's sheet for today, as typed. Rows the operator is still filling in are
/// kept (a product without a quantity yet, an expense without an amount yet).
pub fn save_delivery_sales(conn: &mut Connection, input: SalesInput) -> R<DeliverySalesDTO> {
    if input.products.len() > SALES_LINES_MAX || input.expenses.len() > SALES_LINES_MAX {
        return Err(format!("Up to {SALES_LINES_MAX} products and {SALES_LINES_MAX} expenses per delivery boy."));
    }
    if input.system_payments.len() > SYSTEM_LINES_MAX {
        return Err(format!("Up to {SYSTEM_LINES_MAX} system payments per delivery boy."));
    }
    let mut products = Vec::new();
    for p in &input.products {
        let (name, quantity) = (clean_name(&p.name), parse_sold(&p.quantity)?);
        if name.is_empty() && quantity == 0 {
            continue;
        }
        if name.is_empty() {
            return Err("Enter the product name for each quantity sold.".into());
        }
        products.push(SoldProduct { name, quantity });
    }
    let mut expenses = Vec::new();
    for e in &input.expenses {
        let name: String = clean_name(&e.name).chars().take(EXPENSE_NAME_MAX).collect::<String>().trim_end().to_string();
        let amount = parse_money(&e.amount)?;
        if name.is_empty() && amount == 0.0 {
            continue;
        }
        if name.is_empty() {
            return Err("Enter what each expense was for (fuel, service…).".into());
        }
        expenses.push(Expense { name, amount });
    }
    // System payments: consumer number and optional name; the amount is the fixed System price.
    let mut system_lines = Vec::new();
    for p in &input.system_payments {
        let consumer_no = parse_consumer_no(&p.consumer_no)
            .map_err(|_| "System payment: the consumer number must be exactly 6 digits.".to_string())?;
        let name = parse_other_note(&p.name).map_err(|_| "Enter the consumer's name as text.".to_string())?;
        match consumer_no {
            Some(c) => system_lines.push((c, name)),
            None if name.is_none() => {} // a blank line
            None => return Err("Enter the 6-digit consumer number for each system payment.".into()),
        }
    }
    let (cash, online) = (parse_money(&input.cash)?, parse_money(&input.online)?);
    let price_now = system_price(conn)?;

    let date = time::today();
    let tx = conn.transaction().map_err(db_err)?;
    if is_day_finalized(&tx, &date)? {
        return Err("Today is finalized. Delivery boy sales can no longer be changed.".into());
    }
    let id = sales_id(&date, &input.delivery_boy_id);
    let existing = tx
        .query_row(&format!("SELECT {SALES_COLS} FROM delivery_sales WHERE id = ?1"), [&id], sales_from_row)
        .optional()
        .map_err(db_err)?;
    let active: Option<String> = tx
        .query_row("SELECT name FROM delivery_boys WHERE id = ?1 AND is_active = 1", [&input.delivery_boy_id], |r| r.get(0))
        .optional()
        .map_err(db_err)?;
    let name = active
        .or_else(|| existing.as_ref().map(|e| e.delivery_boy_name.clone()))
        .ok_or("This delivery boy is no longer on the list. Reload the page.")?;
    // Lines saved before keep the price they were saved with; new lines take today's price.
    let mut earlier: Vec<SystemPayment> = existing.as_ref().map(|e| e.system_payments.clone()).unwrap_or_default();
    let mut system_payments = Vec::new();
    for (consumer_no, consumer_name) in system_lines {
        let amount = match earlier.iter().position(|p| p.consumer_no == consumer_no) {
            Some(i) => earlier.remove(i).amount,
            None => price_now.ok_or(SYSTEM_PRICE_MISSING)?,
        };
        system_payments.push(SystemPayment { consumer_no, name: consumer_name, amount });
    }
    let system = (system_payments.iter().map(|p| p.amount).sum::<f64>() * 100.0).round() / 100.0;

    if let Some(e) = existing.as_ref().filter(|e| {
        e.delivery_boy_name == name
            && e.products == products
            && e.expenses == expenses
            && e.system_payments == system_payments
            && (e.cash, e.online) == (cash, online)
    }) {
        return Ok(e.clone()); // nothing changed: nothing to upload
    }
    let user = user_name(&tx)?;
    let now = time::now_iso();
    let (products_json, expenses_json) = (
        serde_json::to_string(&products).unwrap_or_default(),
        serde_json::to_string(&expenses).unwrap_or_default(),
    );
    tx.execute(
        "INSERT INTO delivery_sales (id, date_string, delivery_boy_id, delivery_boy_name, products, cash, online, system,
                                     expenses, updated_by_user, created_at, updated_at, system_payments)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, ?12)
         ON CONFLICT(id) DO UPDATE SET delivery_boy_name = excluded.delivery_boy_name, products = excluded.products,
             cash = excluded.cash, online = excluded.online, system = excluded.system, expenses = excluded.expenses,
             system_payments = excluded.system_payments,
             updated_by_user = excluded.updated_by_user, updated_at = excluded.updated_at, version = version + 1",
        params![
            id, date, input.delivery_boy_id, name, products_json, cash, online, system, expenses_json, user, now,
            serde_json::to_string(&system_payments).unwrap_or_default()
        ],
    )
    .map_err(db_err)?;
    tx.commit().map_err(db_err)?;
    Ok(DeliverySalesDTO {
        delivery_boy_id: input.delivery_boy_id,
        delivery_boy_name: name,
        products,
        cash,
        online,
        system,
        system_payments,
        expenses,
        usual_product: None,
        updated_by_user: user,
        updated_at: Some(now),
    })
}

/// Names for the delivery boy cards: everything sold or spent on before (most used first), plus
/// the register's product catalog and the starter expense names.
pub fn sales_suggestions(conn: &Connection) -> R<SalesSuggestions> {
    // lowercase name -> (spelling first seen, times used)
    type Tally = HashMap<String, (String, usize)>;
    let add = |t: &mut Tally, name: &str, n: usize| {
        let name = name.trim();
        if !name.is_empty() {
            t.entry(name.to_lowercase()).or_insert_with(|| (name.to_string(), 0)).1 += n;
        }
    };
    let (mut products, mut expenses) = (Tally::new(), Tally::new());
    // consumer number -> name, newest first
    let mut consumers: Vec<KnownConsumer> = Vec::new();
    let mut stmt = conn
        .prepare_cached("SELECT products, expenses, system_payments FROM delivery_sales ORDER BY date_string DESC, updated_at DESC")
        .map_err(db_err)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
        .map_err(db_err)?;
    for row in rows {
        let (p, e, sp) = row.map_err(db_err)?;
        for sp in serde_json::from_str::<Vec<SystemPayment>>(&sp).unwrap_or_default() {
            if let Some(name) = sp.name.filter(|_| !consumers.iter().any(|c| c.consumer_no == sp.consumer_no)) {
                consumers.push(KnownConsumer { consumer_no: sp.consumer_no, name });
            }
        }
        for p in serde_json::from_str::<Vec<SoldProduct>>(&p).unwrap_or_default() {
            add(&mut products, &p.name, 1);
        }
        for e in serde_json::from_str::<Vec<Expense>>(&e).unwrap_or_default() {
            add(&mut expenses, &e.name, 1);
        }
    }
    let mut stmt = conn.prepare_cached("SELECT name FROM products").map_err(db_err)?;
    for name in stmt.query_map([], |r| r.get::<_, String>(0)).map_err(db_err)? {
        add(&mut products, &name.map_err(db_err)?, 0);
    }
    for name in STARTER_EXPENSES {
        add(&mut expenses, name, 0);
    }
    let ranked = |t: Tally| {
        let mut v: Vec<(String, usize)> = t.into_values().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
        v.into_iter().map(|(name, _)| name).collect()
    };
    Ok(SalesSuggestions { products: ranked(products), expenses: ranked(expenses), consumers })
}

// ---------------------------------------------------------------------------------------------
// Changes from the other PCs of the office (downloaded by the sync worker)
// ---------------------------------------------------------------------------------------------
// Every row keeps its newest version (by its last change time). What comes from the cloud is
// marked as already synced, so it isn't uploaded back.

fn newer(remote: &str, local: &str) -> bool {
    time::iso_to_millis(remote) > time::iso_to_millis(local)
}

/// An entry from the cloud. Returns true when this PC's data changed.
pub fn apply_remote_tx(tx: &Transaction, t: &TransactionDTO, updated_at: &str) -> R<bool> {
    let local = tx
        .query_row("SELECT updated_at, is_day_finalized FROM transactions WHERE id = ?1", [&t.id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
        })
        .optional()
        .map_err(db_err)?;
    let audit = serde_json::to_string(&t.audit_log).unwrap_or_default();
    match local {
        None => {
            let finalized = t.is_day_finalized || is_day_finalized(tx, &t.date_string)?;
            tx.execute(
                "INSERT INTO transactions (id, date_string, type, product_name, price, consumer_no, is_other_product, status,
                    is_day_finalized, audit_log, created_at, updated_at, payment_mode, created_by,
                    role_id, role_name, quantity, created_by_user, updated_by_user, version, synced_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, 1, 1)",
                params![
                    t.id, t.date_string, t.kind, t.product_name, t.price, t.consumer_no, t.is_other_product, t.status,
                    finalized, audit, t.created_at, updated_at, t.payment_mode, t.created_by,
                    t.role_id, t.role_name, t.quantity, t.created_by_user, t.updated_by_user
                ],
            )
            .map_err(db_err)?;
            Ok(true)
        }
        Some((local_updated, local_final)) if newer(updated_at, &local_updated) => {
            tx.execute(
                "UPDATE transactions SET product_name = ?2, price = ?3, consumer_no = ?4, status = ?5, is_day_finalized = ?6,
                    audit_log = ?7, updated_at = ?8, payment_mode = ?9, created_by = COALESCE(?10, created_by),
                    quantity = ?11, updated_by_user = ?12, synced_version = version
                 WHERE id = ?1",
                params![
                    t.id, t.product_name, t.price, t.consumer_no, t.status, t.is_day_finalized || local_final, audit,
                    updated_at, t.payment_mode, t.created_by, t.quantity, t.updated_by_user
                ],
            )
            .map_err(db_err)?;
            Ok(true)
        }
        Some((_, false)) if t.is_day_finalized => {
            tx.execute("UPDATE transactions SET is_day_finalized = 1 WHERE id = ?1", [&t.id]).map_err(db_err)?;
            Ok(true)
        }
        Some(_) => Ok(false),
    }
}

pub fn apply_remote_delivery_boy(tx: &Transaction, b: &DeliveryBoyDTO, updated_at: &str) -> R<bool> {
    let local: Option<String> = tx
        .query_row("SELECT updated_at FROM delivery_boys WHERE id = ?1", [&b.id], |r| r.get(0))
        .optional()
        .map_err(db_err)?;
    if local.as_deref().is_some_and(|l| !newer(updated_at, l)) {
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO delivery_boys (id, name, is_active, created_at, updated_at, version, synced_version)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, 1)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, is_active = excluded.is_active,
                                       updated_at = excluded.updated_at, synced_version = version",
        params![b.id, b.name, b.is_active, b.created_at, updated_at],
    )
    .map_err(db_err)?;
    Ok(true)
}

/// Catalog entries are matched by name; the most recently used price wins.
pub fn apply_remote_product(tx: &Transaction, name: &str, price: f64, created_at: &str, updated_at: &str) -> R<bool> {
    let name = clean_name(name);
    if name.is_empty() {
        return Ok(false);
    }
    let local: Option<String> = tx
        .query_row("SELECT updated_at FROM products WHERE name = ?1", [&name], |r| r.get(0))
        .optional()
        .map_err(db_err)?;
    match local {
        Some(l) if !newer(updated_at, &l) => Ok(false),
        Some(_) => {
            tx.execute(
                "UPDATE products SET last_used_price = ?2, updated_at = ?3, synced_version = version WHERE name = ?1",
                params![name, price, updated_at],
            )
            .map_err(db_err)?;
            Ok(true)
        }
        None => {
            tx.execute(
                "INSERT INTO products (id, name, last_used_price, created_at, updated_at, version, synced_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, 1)",
                params![new_id(), name, price, created_at, updated_at],
            )
            .map_err(db_err)?;
            Ok(true)
        }
    }
}

/// Another PC's copy of a day's counts, merged cell by cell into this PC's.
pub fn apply_remote_day(tx: &Transaction, remote: &DayCopy) -> R<bool> {
    let date = &remote.day.date_string;
    if !time::is_valid_date(date) {
        return Ok(false);
    }
    match load_day(tx, date)? {
        None => {
            store_day(tx, remote, false)?;
            Ok(true)
        }
        Some(local) => {
            let merged = merge_day(&local, remote);
            if merged.day == local.day && merged.stamps == local.stamps {
                return Ok(false);
            }
            // This PC's copy in the cloud gets the merged counts too, so every copy ends up equal.
            store_day(tx, &merged, true)?;
            Ok(true)
        }
    }
}

/// A delivery boy's sheet from the cloud. Returns true when this PC's data changed.
pub fn apply_remote_sales(tx: &Transaction, date: &str, s: &DeliverySalesDTO, created_at: &str, updated_at: &str) -> R<bool> {
    let id = sales_id(date, &s.delivery_boy_id);
    let local: Option<String> = tx
        .query_row("SELECT updated_at FROM delivery_sales WHERE id = ?1", [&id], |r| r.get(0))
        .optional()
        .map_err(db_err)?;
    if local.as_deref().is_some_and(|l| !newer(updated_at, l)) {
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO delivery_sales (id, date_string, delivery_boy_id, delivery_boy_name, products, cash, online, system,
                                     expenses, updated_by_user, created_at, updated_at, system_payments, version, synced_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 1, 1)
         ON CONFLICT(id) DO UPDATE SET delivery_boy_name = excluded.delivery_boy_name, products = excluded.products,
             cash = excluded.cash, online = excluded.online, system = excluded.system, expenses = excluded.expenses,
             system_payments = excluded.system_payments, updated_by_user = excluded.updated_by_user, updated_at = excluded.updated_at, synced_version = version",
        params![
            id, date, s.delivery_boy_id, s.delivery_boy_name,
            serde_json::to_string(&s.products).unwrap_or_default(), s.cash, s.online, s.system,
            serde_json::to_string(&s.expenses).unwrap_or_default(), s.updated_by_user, created_at, updated_at,
            serde_json::to_string(&s.system_payments).unwrap_or_default()
        ],
    )
    .map_err(db_err)?;
    Ok(true)
}

pub struct MachineInfo<'a> {
    pub id: &'a str,
    pub device_name: &'a str,
    pub app_version: Option<&'a str>,
    pub last_seen_at: Option<&'a str>,
    pub user_name: Option<&'a str>,
}

pub fn apply_remote_machine(tx: &Transaction, m: &MachineInfo) -> R<()> {
    tx.execute(
        "INSERT INTO machines (id, device_name, app_version, last_seen_at, user_name) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET device_name = excluded.device_name, app_version = excluded.app_version,
                                       last_seen_at = excluded.last_seen_at, user_name = excluded.user_name",
        params![m.id, m.device_name, m.app_version, m.last_seen_at, m.user_name],
    )
    .map_err(db_err)?;
    Ok(())
}

/// This PC's own row in the office list, with the current operator name.
pub fn touch_this_pc(tx: &Transaction, info: &AppInfo) -> R<()> {
    let user = user_name(tx)?;
    let now = time::now_iso();
    apply_remote_machine(
        tx,
        &MachineInfo {
            id: &info.machine_id,
            device_name: &info.device_name,
            app_version: Some(&info.app_version),
            last_seen_at: Some(&now),
            user_name: user.as_deref(),
        },
    )
}

pub fn office_pcs(conn: &Connection) -> R<Vec<OfficePcDTO>> {
    let me: Option<String> = crate::db::meta_get(conn, "machine_id").map_err(db_err)?;
    let mut stmt = conn
        .prepare_cached("SELECT id, device_name, app_version, last_seen_at, user_name FROM machines ORDER BY device_name COLLATE NOCASE")
        .map_err(db_err)?;
    let rows = stmt
        .query_map([], |r| {
            let id: String = r.get(0)?;
            Ok(OfficePcDTO {
                is_this_pc: me.as_deref() == Some(id.as_str()),
                machine_id: id,
                device_name: r.get(1)?,
                app_version: r.get(2)?,
                last_seen_at: r.get(3)?,
                user_name: r.get(4)?,
            })
        })
        .map_err(db_err)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)
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
              + (SELECT COUNT(*) FROM delivery_boys WHERE version > synced_version)
              + (SELECT COUNT(*) FROM delivery_sales WHERE version > synced_version)
              + (SELECT COUNT(*) FROM office_settings WHERE version > synced_version)",
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
            .map_err(db_err)?
        + conn
            .execute("DELETE FROM delivery_sales WHERE date_string < ?1 AND version <= synced_version", [cutoff])
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
        let noted = create_transaction(
            &mut c,
            serde_json::from_value(json!({"type":"CASH_IN","productName":"Stove","price":900,"isOtherProduct":true,"consumerNo":"  Ramesh   9876 "}))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(noted.consumer_no.as_deref(), Some("Ramesh 9876"));
        // An empty Other entry can get a note when edited, and lose it again.
        let edit = |c: &mut Connection, note: &str| {
            update_transaction(c, serde_json::from_value(json!({"id":other.id,"productName":"Stove","price":900,"consumerNo":note})).unwrap())
        };
        edit(&mut c, "Paid by Suresh").unwrap();
        assert_eq!(transactions_for_date(&c, &time::today()).unwrap().iter().find(|t| t.id == other.id).unwrap().consumer_no.as_deref(), Some("Paid by Suresh"));
        edit(&mut c, "").unwrap();
        assert!(transactions_for_date(&c, &time::today()).unwrap().iter().find(|t| t.id == other.id).unwrap().consumer_no.is_none());
        let out = create_transaction(
            &mut c,
            serde_json::from_value(json!({"type":"CASH_OUT","productName":"Tea","price":"40","consumerNo":"123456"})).unwrap(),
        )
        .unwrap();
        assert_eq!(out.consumer_no, None);
        assert_eq!(search_products(&c, "reg").unwrap()[0].name, "regulator");
        assert_eq!(pending_count(&c).unwrap(), 5 + 3);
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
    fn payment_modes() {
        let mut c = mem();
        let make = |c: &mut Connection, mode: Value| {
            create_transaction(
                c,
                serde_json::from_value(json!({"type":"CASH_IN","productName":"Refill","price":900,"consumerNo":"123456","paymentMode":mode}))
                    .unwrap(),
            )
        };
        assert_eq!(make(&mut c, json!(null)).unwrap().payment_mode, "CASH");
        assert_eq!(make(&mut c, json!("online")).unwrap().payment_mode, "ONLINE");
        assert!(make(&mut c, json!("CHEQUE")).is_err());
        // A System Cash In takes the fixed System price from Settings, whatever was typed.
        assert_eq!(make(&mut c, json!("SYSTEM")).unwrap_err(), SYSTEM_PRICE_MISSING);
        assert!(set_system_price(&c, &json!("0")).is_err());
        set_system_price(&c, &json!("250")).unwrap();
        let t = make(&mut c, json!("SYSTEM")).unwrap();
        assert_eq!((t.price, t.audit_log[0].new_payment_mode.as_deref()), (250.0, Some("SYSTEM")));
        // ...without making it the product's ghost price.
        assert_eq!(search_products(&c, "Refill").unwrap()[0].last_used_price, 900.0);
        // A System Cash Out keeps its typed price.
        let sys_out = create_transaction(
            &mut c,
            serde_json::from_value(json!({"type":"CASH_OUT","productName":"Diesel","price":500,"paymentMode":"SYSTEM"})).unwrap(),
        )
        .unwrap();
        assert_eq!(sys_out.price, 500.0);
        let out = create_transaction(
            &mut c,
            serde_json::from_value(json!({"type":"CASH_OUT","productName":"Diesel","price":500,"paymentMode":"ONLINE"})).unwrap(),
        )
        .unwrap();
        assert_eq!(out.payment_mode, "ONLINE");

        let update = |c: &mut Connection, body: Value| update_transaction(c, serde_json::from_value(body).unwrap());
        // Leaving paymentMode out keeps it (a System entry keeps its fixed price even if the price
        // changes in Settings); changing only the payment type is a real, audited edit.
        set_system_price(&c, &json!(300)).unwrap();
        update(&mut c, json!({"id":t.id,"productName":"Refill 14kg","price":950,"consumerNo":"123456"})).unwrap();
        let row = transactions_for_date(&c, &time::today()).unwrap().into_iter().find(|r| r.id == t.id).unwrap();
        assert_eq!((row.payment_mode.as_str(), row.price), ("SYSTEM", 250.0));
        update(&mut c, json!({"id":t.id,"productName":"Refill 14kg","price":950,"consumerNo":"123456","paymentMode":"CASH"})).unwrap();
        let row = transactions_for_date(&c, &time::today()).unwrap().into_iter().find(|r| r.id == t.id).unwrap();
        assert_eq!((row.payment_mode.as_str(), row.price, row.audit_log.len()), ("CASH", 950.0, 3));
        assert_eq!(row.audit_log[1].new_payment_mode.as_deref(), Some("SYSTEM"));
        assert_eq!((row.audit_log[2].previous_payment_mode.as_deref(), row.audit_log[2].new_payment_mode.as_deref()), (Some("SYSTEM"), Some("CASH")));
        // Back to System: today's fixed price.
        update(&mut c, json!({"id":t.id,"productName":"Refill 14kg","price":950,"consumerNo":"123456","paymentMode":"SYSTEM"})).unwrap();
        let row = transactions_for_date(&c, &time::today()).unwrap().into_iter().find(|r| r.id == t.id).unwrap();
        assert_eq!(row.price, 300.0);
        assert!(update(&mut c, json!({"id":t.id,"productName":"Refill","price":950,"consumerNo":"123456","paymentMode":"bad"})).is_err());
    }

    #[test]
    fn operator_name_and_old_delivery_boy_entries() {
        let mut c = mem();
        crate::db::meta_set(&c, "machine_id", "PC-A").unwrap();
        assert_eq!(set_user_name(&c, "  Tanuj   B ").unwrap().as_deref(), Some("Tanuj B"));
        let make = |c: &mut Connection, body: Value| create_transaction(c, serde_json::from_value(body).unwrap());

        // The Role box is gone: a role sent by an old screen is ignored, the consumer number is required again.
        let t = make(&mut c, json!({"type":"CASH_IN","productName":"Refill","price":900,"consumerNo":"123456","roleId":"x","quantity":3})).unwrap();
        assert_eq!((t.role_id.as_deref(), t.quantity, t.consumer_no.as_deref()), (None, None, Some("123456")));
        assert_eq!((t.created_by_user.as_deref(), t.audit_log[0].user.as_deref()), (Some("Tanuj B"), Some("Tanuj B")));
        assert_eq!(t.audit_log[0].machine_id.as_deref(), Some("PC-A"));
        assert_eq!(make(&mut c, json!({"type":"CASH_IN","productName":"Refill","price":900,"quantity":3})).unwrap_err(), CONSUMER_REQUIRED);

        // An entry made for a delivery boy with v1.6–1.7 keeps its quantity, which stays editable.
        let boy = add_delivery_boy(&c, "Suresh").unwrap();
        c.execute(
            "INSERT INTO transactions (id, date_string, type, product_name, price, status, audit_log, created_at, updated_at,
                                       role_id, role_name, quantity, created_by_user)
             VALUES ('old', ?1, 'CASH_IN', 'Refill', 1800, 'ACTIVE', '[]', ?2, ?2, ?3, 'Suresh', 2, 'Tanuj B')",
            params![time::today(), time::now_iso(), boy.id],
        )
        .unwrap();
        set_user_name(&c, "Ravi").unwrap();
        let update = |c: &mut Connection, body: Value| update_transaction(c, serde_json::from_value(body).unwrap());
        assert!(update(&mut c, json!({"id":"old","productName":"Refill","price":1800})).is_err());
        update(&mut c, json!({"id":"old","productName":"Refill","price":1800,"quantity":3})).unwrap();
        delete_transaction(&mut c, &t.id).unwrap();
        let rows = transactions_for_date(&c, &time::today()).unwrap();
        let old = rows.iter().find(|r| r.id == "old").unwrap();
        assert_eq!((old.quantity, old.audit_log[0].previous_quantity, old.audit_log[0].user.as_deref()), (Some(3), Some(2), Some("Ravi")));
        assert_eq!((old.created_by_user.as_deref(), old.updated_by_user.as_deref()), (Some("Tanuj B"), Some("Ravi")));
        let gone = rows.iter().find(|r| r.id == t.id).unwrap();
        assert_eq!((gone.status.as_str(), gone.audit_log[1].user.as_deref()), ("DELETED", Some("Ravi")));

        // Without a name, changes still say which PC made them.
        set_user_name(&c, "   ").unwrap();
        let anon = make(&mut c, json!({"type":"CASH_OUT","productName":"Tea","price":20})).unwrap();
        assert_eq!((anon.created_by_user, anon.audit_log[0].machine_id.as_deref()), (None, Some("PC-A")));
    }

    #[test]
    fn delivery_boy_sales_sheet() {
        let mut c = mem();
        set_user_name(&c, "Tanuj").unwrap();
        let suresh = add_delivery_boy(&c, "Suresh").unwrap();
        let ramesh = add_delivery_boy(&c, "Ramesh").unwrap();
        let save = |c: &mut Connection, body: Value| save_delivery_sales(c, serde_json::from_value(body).unwrap());

        // Every active delivery boy has a blank sheet; nothing is stored until something is typed.
        let page = day_end_page(&c).unwrap();
        assert_eq!(page.sales.iter().map(|s| s.delivery_boy_name.as_str()).collect::<Vec<_>>(), ["Ramesh", "Suresh"]);
        assert!(page.sales.iter().all(|s| s.updated_at.is_none() && s.usual_product.is_none()));

        let line = |c: &mut Connection| save(c, json!({"deliveryBoyId":suresh.id,"systemPayments":[{"consumerNo":"123456"}]}));
        assert_eq!(line(&mut c).unwrap_err(), SYSTEM_PRICE_MISSING);
        set_system_price(&c, &json!(900)).unwrap();
        let saved = save(
            &mut c,
            json!({"deliveryBoyId":suresh.id,
                   "products":[{"name":" Refill  14.2kg ","quantity":"12"},{"name":"","quantity":""},{"name":"Refill 19kg","quantity":""}],
                   "cash":"10500.5","online":"2400",
                   "systemPayments":[{"consumerNo":"123-456","name":"  Anil  Kumar ","amount":"900"},{"consumerNo":"654321","amount":"450.5"},
                                     {"consumerNo":"","name":"","amount":""}],
                   "expenses":[{"name":"  Fuel ","amount":"200"},{"name":"Tyre puncture","amount":""},{"name":"","amount":""}]}),
        )
        .unwrap();
        // Blank lines dropped; a product still waiting for its quantity is kept.
        assert_eq!(
            saved.products,
            vec![SoldProduct { name: "Refill 14.2kg".into(), quantity: 12 }, SoldProduct { name: "Refill 19kg".into(), quantity: 0 }]
        );
        // System lines take the fixed price (an amount sent along is ignored).
        assert_eq!((saved.cash, saved.online, saved.system), (10500.5, 2400.0, 1800.0));
        assert_eq!(
            saved.system_payments,
            vec![
                SystemPayment { consumer_no: "123456".into(), name: Some("Anil Kumar".into()), amount: 900.0 },
                SystemPayment { consumer_no: "654321".into(), name: None, amount: 900.0 },
            ]
        );
        // A blank expense row is dropped; one still waiting for its amount is kept.
        assert_eq!(
            saved.expenses,
            vec![Expense { name: "Fuel".into(), amount: 200.0 }, Expense { name: "Tyre puncture".into(), amount: 0.0 }]
        );
        assert_eq!(saved.updated_by_user.as_deref(), Some("Tanuj"));
        let pending = pending_count(&c).unwrap();
        // Saving the same sheet again changes nothing (nothing to upload).
        save(
            &mut c,
            json!({"deliveryBoyId":suresh.id,"products":[{"name":"Refill 14.2kg","quantity":12},{"name":"Refill 19kg","quantity":0}],
                   "cash":10500.5,"online":2400,"expenses":[{"name":"Fuel","amount":200},{"name":"Tyre puncture","amount":0}],
                   "systemPayments":[{"consumerNo":"123456","name":"Anil Kumar","amount":900},{"consumerNo":"654321","amount":450.5}]}),
        )
        .unwrap();
        let version: i64 = c.query_row("SELECT version FROM delivery_sales", [], |r| r.get(0)).unwrap();
        assert_eq!((pending_count(&c).unwrap(), version), (pending, 1));

        // A new price in Settings: lines already saved keep theirs, a new line takes the new one.
        set_system_price(&c, &json!(1000)).unwrap();
        let more = save(
            &mut c,
            json!({"deliveryBoyId":suresh.id,"products":[{"name":"Refill 14.2kg","quantity":12},{"name":"Refill 19kg","quantity":0}],
                   "cash":10500.5,"online":2400,"expenses":[{"name":"Fuel","amount":200},{"name":"Tyre puncture","amount":0}],
                   "systemPayments":[{"consumerNo":"123456","name":"Anil Kumar"},{"consumerNo":"654321"},{"consumerNo":"111111"}]}),
        )
        .unwrap();
        assert_eq!(more.system_payments.iter().map(|p| p.amount).collect::<Vec<_>>(), [900.0, 900.0, 1000.0]);
        assert_eq!(more.system, 2800.0);

        // Bad input is refused with a message.
        assert!(save(&mut c, json!({"deliveryBoyId":suresh.id,"products":[{"name":"","quantity":3}]})).is_err());
        assert!(save(&mut c, json!({"deliveryBoyId":suresh.id,"cash":"-5"})).is_err());
        assert!(save(&mut c, json!({"deliveryBoyId":suresh.id,"products":[{"name":"X","quantity":"1.5"}]})).is_err());
        assert!(save(&mut c, json!({"deliveryBoyId":suresh.id,"expenses":[{"name":" ","amount":5}]})).is_err());
        assert!(save(&mut c, json!({"deliveryBoyId":suresh.id,"systemPayments":[{"consumerNo":"","name":"Anil"}]})).is_err());
        assert!(save(&mut c, json!({"deliveryBoyId":suresh.id,"systemPayments":[{"consumerNo":"12345"}]})).is_err());

        // What was typed is offered next time: used names first, then the catalog / starter names.
        cash_in(&mut c, "Regulator", json!(350), "123456").unwrap();
        let s = sales_suggestions(&c).unwrap();
        assert_eq!(s.products, ["Refill 14.2kg", "Refill 19kg", "Regulator"]);
        assert_eq!(s.expenses, ["Fuel", "Tyre puncture", "Service"]);
        assert_eq!(s.consumers.iter().map(|c| (c.consumer_no.as_str(), c.name.as_str())).collect::<Vec<_>>(), [("123456", "Anil Kumar")]);
        assert!(save(&mut c, json!({"deliveryBoyId":"nobody","cash":5})).is_err());

        // Tomorrow, his card offers the product he sold last.
        c.execute("UPDATE delivery_sales SET date_string = '2000-01-01', id = '2000-01-01:' || delivery_boy_id", []).unwrap();
        let page = day_end_page(&c).unwrap();
        let s = page.sales.iter().find(|s| s.delivery_boy_id == suresh.id).unwrap();
        assert_eq!((s.usual_product.as_deref(), s.updated_at.as_deref()), (Some("Refill 14.2kg"), None));

        // A sheet from another PC: newer wins, older is ignored.
        let remote = |name: &str, cash: f64| DeliverySalesDTO {
            delivery_boy_id: ramesh.id.clone(),
            delivery_boy_name: name.into(),
            products: vec![SoldProduct { name: "Refill 14.2kg".into(), quantity: 4 }],
            cash,
            online: 0.0,
            system: 0.0,
            system_payments: vec![],
            expenses: vec![],
            usual_product: None,
            updated_by_user: Some("Ravi".into()),
            updated_at: None,
        };
        let today = time::today();
        let tx = c.transaction().unwrap();
        assert!(apply_remote_sales(&tx, &today, &remote("Ramesh", 3600.0), "2026-01-01T10:00:00.000Z", "2026-01-01T10:00:00.000Z").unwrap());
        assert!(!apply_remote_sales(&tx, &today, &remote("Ramesh", 1.0), "2026-01-01T09:00:00.000Z", "2026-01-01T09:00:00.000Z").unwrap());
        tx.commit().unwrap();
        let r = sales_for_date(&c, &today).unwrap();
        assert_eq!((r.len(), r[0].cash, r[0].updated_by_user.as_deref()), (1, 3600.0, Some("Ravi")));
        // Rows from the cloud are not uploaded back.
        let unsynced: i64 = c.query_row("SELECT COUNT(*) FROM delivery_sales WHERE version > synced_version AND date_string = ?1", [&today], |r| r.get(0)).unwrap();
        assert_eq!(unsynced, 0);

        // Once the day is finalized, sheets are locked.
        finalize_day(&mut c, serde_json::from_value(json!({})).unwrap()).unwrap();
        assert!(save(&mut c, json!({"deliveryBoyId":ramesh.id,"cash":1})).is_err());
        assert_eq!(day_end_page(&c).unwrap().sales.len(), 1); // finalized: only what was saved
    }

    #[test]
    fn upgrade_from_v1_3_database_marks_old_entries_cash() {
        let path = std::env::temp_dir().join(format!("hpcl-upgrade-{}.db", new_id()));
        {
            let mut old = Connection::open(&path).unwrap();
            let tx = old.transaction().unwrap();
            tx.execute_batch(crate::db::MIGRATIONS[0]).unwrap();
            tx.pragma_update(None, "user_version", 1).unwrap();
            tx.execute(
                "INSERT INTO transactions (id, date_string, type, product_name, price, created_at, updated_at) VALUES ('a', ?1, 'CASH_IN', 'X', 5, ?1, ?1)",
                [time::today()],
            )
            .unwrap();
            tx.commit().unwrap();
        }
        let c = crate::db::open(&path).unwrap();
        assert_eq!(transactions_for_date(&c, &time::today()).unwrap()[0].payment_mode, "CASH");
    }

    /// Copy everything PC `from` has into PC `to`, the way the sync worker would.
    fn sync(from: &Connection, to: &mut Connection) {
        let today = time::today();
        let txs = transactions_for_date(from, &today).unwrap();
        let ups: Vec<String> = txs
            .iter()
            .map(|t| from.query_row("SELECT updated_at FROM transactions WHERE id = ?1", [&t.id], |r| r.get(0)).unwrap())
            .collect();
        let day = load_day(from, &today).unwrap();
        let boys: Vec<(DeliveryBoyDTO, String)> = list_delivery_boys(from)
            .unwrap()
            .into_iter()
            .map(|b| {
                let u = from.query_row("SELECT updated_at FROM delivery_boys WHERE id = ?1", [&b.id], |r| r.get(0)).unwrap();
                (b, u)
            })
            .collect();
        let tx = to.transaction().unwrap();
        for (b, u) in &boys {
            apply_remote_delivery_boy(&tx, b, u).unwrap();
        }
        for (t, u) in txs.iter().zip(&ups) {
            apply_remote_tx(&tx, t, u).unwrap();
        }
        if let Some(d) = day {
            apply_remote_day(&tx, &d).unwrap();
        }
        tx.commit().unwrap();
    }

    fn day_input(boy: &str, bills: i64, kg14_full: i64, base: Option<(i64, i64)>) -> DayEndInput {
        let mut v = json!({"deliveries":[{"deliveryBoyId":boy,"billsPrinted":bills}],"cylinderStock":{"kg14":{"full":kg14_full}}});
        if let Some((b, f)) = base {
            v["base"] = json!({"deliveries":[{"deliveryBoyId":boy,"billsPrinted":b}],"cylinderStock":{"kg14":{"full":f}}});
        }
        serde_json::from_value(v).unwrap()
    }

    fn tick() {
        std::thread::sleep(std::time::Duration::from_millis(5)); // distinct timestamps
    }

    #[test]
    fn two_pcs_share_one_office() {
        let (mut a, mut b) = (mem(), mem());
        let boy = add_delivery_boy(&a, "Suresh").unwrap();
        let t = cash_in(&mut a, "Refill", json!(900), "123456").unwrap();
        sync(&a, &mut b);
        // B sees A's entry and delivery boy, and doesn't upload them back.
        assert_eq!(transactions_for_date(&b, &time::today()).unwrap().len(), 1);
        assert_eq!(list_delivery_boys(&b).unwrap()[0].name, "Suresh");
        assert_eq!(pending_count(&b).unwrap(), 0);
        // Applying the same change again is a no-op.
        let again = b.transaction().unwrap();
        let up: String = a.query_row("SELECT updated_at FROM transactions WHERE id = ?1", [&t.id], |r| r.get(0)).unwrap();
        assert!(!apply_remote_tx(&again, &transactions_for_date(&a, &time::today()).unwrap()[0], &up).unwrap());
        again.commit().unwrap();

        // Both edit the same entry: the later edit wins on both PCs.
        tick();
        update_transaction(&mut a, serde_json::from_value(json!({"id":t.id,"productName":"Refill","price":950,"consumerNo":"123456"})).unwrap()).unwrap();
        tick();
        update_transaction(&mut b, serde_json::from_value(json!({"id":t.id,"productName":"Refill","price":1000,"consumerNo":"123456"})).unwrap()).unwrap();
        sync(&a, &mut b);
        sync(&b, &mut a);
        let price = |c: &Connection| transactions_for_date(c, &time::today()).unwrap()[0].price;
        assert_eq!((price(&a), price(&b)), (1000.0, 1000.0));

        // Day End: A types bills while B (with a form loaded earlier) types stock. Neither overwrites the other.
        save_day_end(&mut a, day_input(&boy.id, 12, 0, Some((0, 0)))).unwrap();
        tick();
        save_day_end(&mut b, day_input(&boy.id, 0, 40, Some((0, 0)))).unwrap();
        sync(&a, &mut b);
        sync(&b, &mut a);
        sync(&a, &mut b);
        let counts = |c: &Connection| {
            let d = get_day_end(c, &time::today()).unwrap().unwrap();
            (d.deliveries[0].bills_printed, d.cylinder_stock.kg14.full)
        };
        assert_eq!((counts(&a), counts(&b)), ((12, 40), (12, 40)));
        // A's form still shows kg14.full = 0 from before; saving it again must not wipe B's 40.
        tick();
        save_day_end(&mut a, day_input(&boy.id, 15, 0, Some((12, 0)))).unwrap();
        assert_eq!(counts(&a), (15, 40));

        // Finalizing on B locks the day on A.
        finalize_day(&mut b, day_input(&boy.id, 12, 40, Some((12, 40)))).unwrap();
        sync(&b, &mut a);
        assert!(is_day_finalized(&a, &time::today()).unwrap());
        assert!(transactions_for_date(&a, &time::today()).unwrap()[0].is_day_finalized);
        assert!(cash_in(&mut a, "Refill", json!(900), "123456").unwrap_err().contains("finalized"));
    }

    #[test]
    fn day_merge_is_the_same_both_ways_and_handles_old_logs() {
        let day = |bills: i64, full: i64| DayEndDTO {
            date_string: "2026-11-02".into(),
            is_finalized: false,
            finalized_at: None,
            deliveries: vec![DeliveryDTO { delivery_boy_id: "x".into(), delivery_boy_name: "X".into(), bills_printed: bills, cylinders_delivered: 0 }],
            cylinder_stock: CylinderStock { kg14: StockPair { full, empty: 0 }, ..Default::default() },
        };
        let stamps = |cells: &[(&str, &str)]| Some(cells.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Stamps>());
        let a = DayCopy { day: day(5, 0), stamps: stamps(&[("bills:x", "2026-11-02T10:00:00.000Z")]), updated_at: "2026-11-02T10:00:00.000Z".into() };
        let b = DayCopy { day: day(0, 30), stamps: stamps(&[("kg14.full", "2026-11-02T09:00:00.000Z")]), updated_at: "2026-11-02T11:00:00.000Z".into() };
        let (ab, ba) = (merge_day(&a, &b), merge_day(&b, &a));
        assert_eq!(ab, ba);
        assert_eq!((ab.day.deliveries[0].bills_printed, ab.day.cylinder_stock.kg14.full), (5, 30));
        // A log saved before v1.5 (no stamps) counts as typed at its own time.
        let old = DayCopy { day: day(9, 9), stamps: None, updated_at: "2026-11-02T09:30:00.000Z".into() };
        let m = merge_day(&a, &old);
        assert_eq!((m.day.deliveries[0].bills_printed, m.day.cylinder_stock.kg14.full), (5, 9));
    }

    #[test]
    fn upgrade_from_v1_4_database_keeps_delivery_boys() {
        let path = std::env::temp_dir().join(format!("hpcl-upgrade14-{}.db", new_id()));
        {
            let mut old = Connection::open(&path).unwrap();
            let tx = old.transaction().unwrap();
            tx.execute_batch(crate::db::MIGRATIONS[0]).unwrap();
            tx.execute_batch(crate::db::MIGRATIONS[1]).unwrap();
            tx.pragma_update(None, "user_version", 2).unwrap();
            tx.execute("INSERT INTO delivery_boys (id, name, created_at, updated_at) VALUES ('b1', 'Ramesh', 'x', 'x')", []).unwrap();
            tx.commit().unwrap();
        }
        let c = crate::db::open(&path).unwrap();
        assert_eq!(list_delivery_boys(&c).unwrap()[0].name, "Ramesh");
        // Same name from another PC (added there while offline) no longer breaks the download.
        let tx = Connection::open(&path).unwrap();
        tx.execute("INSERT INTO delivery_boys (id, name, created_at, updated_at) VALUES ('b2', 'ramesh', 'x', 'x')", []).unwrap();
        assert!(add_delivery_boy(&c, "RAMESH").is_err());
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
