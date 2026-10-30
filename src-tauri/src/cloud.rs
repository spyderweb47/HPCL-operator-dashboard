//! Cloud sync with MongoDB.
//!
//! The local SQLite file is always written first. This worker uploads every changed row
//! (`version > synced_version`) as a full-document upsert, so retries are harmless and nothing
//! is lost while offline. It wakes immediately after each local write (real-time when online),
//! otherwise every 30 s, and retries every 5 s → 30 s while the internet is down.

use futures_util::{stream, StreamExt, TryStreamExt};
use mongodb::bson::{doc, Bson, DateTime as BsonDate, Document};
use mongodb::error::ErrorKind;
use mongodb::options::{ClientOptions, IndexOptions};
use mongodb::{Client, Database, IndexModel};
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;

use crate::core::Core;
use crate::model::*;
use crate::store::{self, db_err, R};
use crate::{db, time};

pub const DEFAULT_DATABASE: &str = "hpcl_dealership";
const TRANSACTIONS: &str = "transactions";
const DAY_END_LOGS: &str = "dayendlogs";
const PRODUCTS: &str = "products";
const DELIVERY_BOYS: &str = "deliveryboys";
const MACHINES: &str = "machines";

const BATCH: i64 = 200;
const PARALLEL_UPLOADS: usize = 8;
const IDLE_CHECK: Duration = Duration::from_secs(30);
const HEARTBEAT_EVERY: Duration = Duration::from_secs(5 * 60);
const PRUNE_EVERY_HOURS: i64 = 6;

// ---------------------------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct CloudConfig {
    pub uri: String,
    pub database: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigFile {
    mongodb_uri: String,
    #[serde(default)]
    database: Option<String>,
}

/// First match wins: `cloud.json` in the app config folder, the `HPCL_MONGODB_URI` environment
/// variable, then the URI baked in at build time (`HPCL_MONGODB_URI=... npm run app:build`).
pub fn load_config(config_dir: &Path) -> Option<CloudConfig> {
    let from_file = std::fs::read_to_string(config_dir.join("cloud.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<ConfigFile>(&s).ok())
        .map(|f| (f.mongodb_uri, f.database));
    let (uri, database) = from_file
        .or_else(|| std::env::var("HPCL_MONGODB_URI").ok().map(|u| (u, std::env::var("HPCL_MONGODB_DB").ok())))
        .or_else(|| option_env!("HPCL_MONGODB_URI").map(|u| (u.to_string(), option_env!("HPCL_MONGODB_DB").map(String::from))))?;
    let uri = uri.trim().to_string();
    if uri.is_empty() {
        return None;
    }
    let database = database
        .filter(|d| !d.trim().is_empty())
        .or_else(|| database_from_uri(&uri))
        .unwrap_or_else(|| DEFAULT_DATABASE.into());
    Some(CloudConfig { uri, database })
}

/// `mongodb+srv://user:pw@host/<db>?opts` -> `<db>`
fn database_from_uri(uri: &str) -> Option<String> {
    let rest = uri.split_once("://")?.1;
    let path = rest.split_once('/')?.1;
    let name = path.split('?').next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

// ---------------------------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------------------------

struct Inner {
    state: &'static str,
    last_sync_at: Option<String>,
    last_error: Option<String>,
}

pub struct SyncShared {
    pub notify: Notify,
    pub config: Option<CloudConfig>,
    inner: Mutex<Inner>,
    remote: Mutex<Option<Database>>,
}

impl SyncShared {
    pub fn new(config: Option<CloudConfig>) -> Self {
        let state = if config.is_some() { "connecting" } else { "not_configured" };
        SyncShared {
            notify: Notify::new(),
            config,
            inner: Mutex::new(Inner { state, last_sync_at: None, last_error: None }),
            remote: Mutex::new(None),
        }
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_state(&self, state: &'static str) {
        self.inner().state = state;
    }

    fn remote(&self) -> Option<Database> {
        self.remote.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

pub fn status(core: &Core) -> SyncStatusDTO {
    let pending = core.with_db(|c| store::pending_count(c)).unwrap_or(0);
    let inner = core.sync.inner();
    SyncStatusDTO {
        state: inner.state,
        pending,
        last_sync_at: inner.last_sync_at.clone(),
        last_error: inner.last_error.clone(),
    }
}

enum SyncError {
    Local(String),
    Remote(mongodb::error::Error),
}

impl From<String> for SyncError {
    fn from(e: String) -> Self {
        SyncError::Local(e)
    }
}

impl From<mongodb::error::Error> for SyncError {
    fn from(e: mongodb::error::Error) -> Self {
        SyncError::Remote(e)
    }
}

/// (is it just "no internet"?, message for the operator / support)
fn describe(e: &SyncError) -> (bool, String) {
    match e {
        SyncError::Local(m) => (false, m.clone()),
        SyncError::Remote(e) => match e.kind.as_ref() {
            ErrorKind::ServerSelection { .. }
            | ErrorKind::Io(_)
            | ErrorKind::DnsResolve { .. }
            | ErrorKind::ConnectionPoolCleared { .. } => (true, format!("Cannot reach the cloud database: {e}")),
            ErrorKind::Authentication { message, .. } => (false, format!("Cloud login failed: {message}")),
            ErrorKind::InvalidArgument { message, .. } => (false, format!("Cloud address is invalid: {message}")),
            _ => (false, format!("Cloud sync failed: {e}")),
        },
    }
}

// ---------------------------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------------------------

pub async fn run(core: Arc<Core>) {
    let Some(cfg) = core.sync.config.clone() else { return };
    let mut failures = 0usize;
    let mut last_heartbeat: Option<Instant> = None;
    loop {
        let wait = match cycle(&core, &cfg, &mut last_heartbeat).await {
            Ok(()) => {
                failures = 0;
                IDLE_CHECK
            }
            Err(e) => {
                let (offline, message) = describe(&e);
                eprintln!("[sync] {message}");
                {
                    let mut inner = core.sync.inner();
                    inner.state = if offline { "offline" } else { "error" };
                    inner.last_error = Some(message);
                }
                failures += 1;
                Duration::from_secs([5, 10, 20, 30][(failures - 1).min(3)])
            }
        };
        tokio::select! {
            _ = core.sync.notify.notified() => {
                // Let a burst of rapid entries collect into one upload round.
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            _ = tokio::time::sleep(wait) => {}
        }
    }
}

async fn cycle(core: &Arc<Core>, cfg: &CloudConfig, last_heartbeat: &mut Option<Instant>) -> Result<(), SyncError> {
    let db = match core.sync.remote() {
        Some(db) => db,
        None => {
            let db = connect(cfg).await?;
            *core.sync.remote.lock().unwrap_or_else(|e| e.into_inner()) = Some(db.clone());
            db
        }
    };
    db.run_command(doc! { "ping": 1 }).await?;

    if core.with_db(|c| db::meta_get(c, "restore_done").map_err(db_err))?.is_none() {
        restore(core, &db).await?;
    }
    let pushed = push_all(core, &db).await?;
    if pushed > 0 || last_heartbeat.is_none_or(|t| t.elapsed() > HEARTBEAT_EVERY) {
        heartbeat(core, &db).await?;
        *last_heartbeat = Some(Instant::now());
    }
    core.with_db(|c| maybe_prune(c))?;

    let mut inner = core.sync.inner();
    inner.state = "online";
    inner.last_error = None;
    inner.last_sync_at = Some(time::now_iso());
    Ok(())
}

async fn connect(cfg: &CloudConfig) -> mongodb::error::Result<Database> {
    let mut opts = ClientOptions::parse(&cfg.uri).await?;
    opts.app_name = Some(format!("hpcl-operator/{}", env!("CARGO_PKG_VERSION")));
    opts.server_selection_timeout = Some(Duration::from_secs(8));
    opts.connect_timeout = Some(Duration::from_secs(8));
    opts.max_pool_size = Some(PARALLEL_UPLOADS as u32);
    let db = Client::with_options(opts)?.database(&cfg.database);
    ensure_indexes(&db).await?;
    Ok(db)
}

async fn ensure_indexes(db: &Database) -> mongodb::error::Result<()> {
    let index = |keys: Document, unique: bool| {
        IndexModel::builder().keys(keys).options(IndexOptions::builder().unique(unique).build()).build()
    };
    db.collection::<Document>(TRANSACTIONS)
        .create_indexes([
            index(doc! { "machineId": 1, "dateString": 1, "createdAt": 1 }, false),
            index(doc! { "dateString": 1 }, false),
            index(doc! { "consumerNo": 1 }, false),
        ])
        .await?;
    db.collection::<Document>(DAY_END_LOGS)
        .create_index(index(doc! { "machineId": 1, "dateString": 1 }, true))
        .await?;
    db.collection::<Document>(PRODUCTS)
        .create_index(index(doc! { "machineId": 1, "nameKey": 1 }, true))
        .await?;
    db.collection::<Document>(DELIVERY_BOYS)
        .create_index(index(doc! { "machineId": 1, "name": 1 }, false))
        .await?;
    Ok(())
}

async fn heartbeat(core: &Core, db: &Database) -> Result<(), SyncError> {
    let pending = core.with_db(|c| store::pending_count(c))?;
    let now = BsonDate::now();
    let info = &core.info;
    db.collection::<Document>(MACHINES)
        .update_one(
            doc! { "_id": &info.machine_id },
            doc! {
                "$set": {
                    "deviceName": &info.device_name,
                    "os": std::env::consts::OS,
                    "appVersion": &info.app_version,
                    "retentionDays": info.retention_days,
                    "pendingCount": pending,
                    "lastSeenAt": now,
                },
                "$setOnInsert": { "firstSeenAt": now },
            },
        )
        .upsert(true)
        .await?;
    Ok(())
}

fn maybe_prune(conn: &Connection) -> R<()> {
    let last = db::meta_get(conn, "last_prune_at").map_err(db_err)?;
    let due = last.is_none_or(|t| time::iso_to_millis(&time::now_iso()) - time::iso_to_millis(&t) > PRUNE_EVERY_HOURS * 3_600_000);
    if due {
        let removed = store::prune(conn, &time::retention_cutoff(&time::today()))?;
        if removed > 0 {
            eprintln!("[sync] pruned {removed} synced rows older than {} days", time::RETENTION_DAYS);
        }
        db::meta_set(conn, "last_prune_at", &time::now_iso()).map_err(db_err)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Upload
// ---------------------------------------------------------------------------------------------

struct Upload {
    table: &'static str,
    collection: &'static str,
    id: String,
    version: i64,
    filter: Document,
    doc: Document,
}

fn date(iso: &str) -> Bson {
    Bson::DateTime(BsonDate::from_millis(time::iso_to_millis(iso)))
}

fn opt_str(v: &Option<String>) -> Bson {
    v.clone().map(Bson::String).unwrap_or(Bson::Null)
}

fn audit_doc(a: &AuditEntry) -> Document {
    let mut d = doc! { "action": &a.action, "timestamp": date(&a.timestamp) };
    let mut put_s = |k: &str, v: &Option<String>| {
        if let Some(v) = v {
            d.insert(k, v);
        }
    };
    put_s("previousProductName", &a.previous_product_name);
    put_s("newProductName", &a.new_product_name);
    put_s("previousConsumerNo", &a.previous_consumer_no);
    put_s("newConsumerNo", &a.new_consumer_no);
    if let Some(p) = a.previous_price {
        d.insert("previousPrice", p);
    }
    if let Some(p) = a.new_price {
        d.insert("newPrice", p);
    }
    d
}

/// Up to BATCH changed rows across all tables, as ready-to-send MongoDB documents.
fn pending_uploads(conn: &Connection, machine_id: &str) -> R<Vec<Upload>> {
    let mut out = Vec::new();
    let synced_at = BsonDate::now();

    // Delivery boys: keyed by their local id.
    let mut stmt = conn
        .prepare_cached("SELECT id, name, is_active, created_at, updated_at, version FROM delivery_boys WHERE version > synced_version LIMIT ?1")
        .map_err(db_err)?;
    let rows = stmt
        .query_map([BATCH], |r| {
            let id: String = r.get(0)?;
            Ok(Upload {
                table: "delivery_boys",
                collection: DELIVERY_BOYS,
                filter: doc! { "_id": &id },
                doc: doc! {
                    "_id": &id,
                    "machineId": machine_id,
                    "name": r.get::<_, String>(1)?,
                    "isActive": r.get::<_, bool>(2)?,
                    "createdAt": date(&r.get::<_, String>(3)?),
                    "updatedAt": date(&r.get::<_, String>(4)?),
                    "syncedAt": synced_at,
                },
                version: r.get(5)?,
                id,
            })
        })
        .map_err(db_err)?;
    out.extend(rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)?);

    // Products: one per (machine, lower-case name).
    let mut stmt = conn
        .prepare_cached("SELECT id, name, last_used_price, created_at, updated_at, version FROM products WHERE version > synced_version LIMIT ?1")
        .map_err(db_err)?;
    let rows = stmt
        .query_map([BATCH], |r| {
            let id: String = r.get(0)?;
            let name: String = r.get(1)?;
            let key = name.to_lowercase();
            Ok(Upload {
                table: "products",
                collection: PRODUCTS,
                filter: doc! { "machineId": machine_id, "nameKey": &key },
                doc: doc! {
                    "machineId": machine_id,
                    "name": &name,
                    "nameKey": &key,
                    "lastUsedPrice": r.get::<_, f64>(2)?,
                    "localId": &id,
                    "createdAt": date(&r.get::<_, String>(3)?),
                    "updatedAt": date(&r.get::<_, String>(4)?),
                    "syncedAt": synced_at,
                },
                version: r.get(5)?,
                id,
            })
        })
        .map_err(db_err)?;
    out.extend(rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)?);

    // Day-end logs: one per (machine, date).
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, date_string, deliveries, cylinder_stock, is_finalized, finalized_at, created_at, updated_at, version
             FROM day_end_logs WHERE version > synced_version LIMIT ?1",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map([BATCH], |r| {
            let id: String = r.get(0)?;
            let date_string: String = r.get(1)?;
            let deliveries: Vec<DeliveryDTO> = serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default();
            let stock: CylinderStock = serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default();
            let pair = |p: StockPair| doc! { "full": p.full, "empty": p.empty };
            Ok(Upload {
                table: "day_end_logs",
                collection: DAY_END_LOGS,
                filter: doc! { "machineId": machine_id, "dateString": &date_string },
                doc: doc! {
                    "machineId": machine_id,
                    "dateString": &date_string,
                    "localId": &id,
                    "deliveries": deliveries.iter().map(|d| doc! {
                        "deliveryBoyId": &d.delivery_boy_id,
                        "deliveryBoyName": &d.delivery_boy_name,
                        "billsPrinted": d.bills_printed,
                        "cylindersDelivered": d.cylinders_delivered,
                    }).collect::<Vec<_>>(),
                    "cylinderStock": { "kg5": pair(stock.kg5), "kg14": pair(stock.kg14), "kg19": pair(stock.kg19) },
                    "isFinalized": r.get::<_, bool>(4)?,
                    "finalizedAt": r.get::<_, Option<String>>(5)?.map(|s| date(&s)).unwrap_or(Bson::Null),
                    "createdAt": date(&r.get::<_, String>(6)?),
                    "updatedAt": date(&r.get::<_, String>(7)?),
                    "syncedAt": synced_at,
                },
                version: r.get(8)?,
                id,
            })
        })
        .map_err(db_err)?;
    out.extend(rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)?);

    // Transactions: keyed by their local UUID.
    let mut stmt = conn
        .prepare_cached(
            "SELECT id, date_string, type, product_name, price, consumer_no, is_other_product, status, is_day_finalized,
                    audit_log, created_at, updated_at, version
             FROM transactions WHERE version > synced_version ORDER BY created_at LIMIT ?1",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map([BATCH], |r| {
            let id: String = r.get(0)?;
            let audit: Vec<AuditEntry> = serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or_default();
            Ok(Upload {
                table: "transactions",
                collection: TRANSACTIONS,
                filter: doc! { "_id": &id },
                doc: doc! {
                    "_id": &id,
                    "machineId": machine_id,
                    "dateString": r.get::<_, String>(1)?,
                    "type": r.get::<_, String>(2)?,
                    "productName": r.get::<_, String>(3)?,
                    "price": r.get::<_, f64>(4)?,
                    "consumerNo": opt_str(&r.get(5)?),
                    "isOtherProduct": r.get::<_, bool>(6)?,
                    "status": r.get::<_, String>(7)?,
                    "isDayFinalized": r.get::<_, bool>(8)?,
                    "auditLog": audit.iter().map(audit_doc).collect::<Vec<_>>(),
                    "createdAt": date(&r.get::<_, String>(10)?),
                    "updatedAt": date(&r.get::<_, String>(11)?),
                    "syncedAt": synced_at,
                },
                version: r.get(12)?,
                id,
            })
        })
        .map_err(db_err)?;
    out.extend(rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)?);

    out.truncate(BATCH as usize);
    Ok(out)
}

/// Record what the cloud now has. A row edited again while uploading keeps its newer version
/// and goes up in the next round.
fn mark_synced(conn: &mut Connection, done: &[(&'static str, String, i64)]) -> R<()> {
    let tx = conn.transaction().map_err(db_err)?;
    for (table, id, version) in done {
        tx.execute(
            &format!("UPDATE {table} SET synced_version = ?2 WHERE id = ?1 AND synced_version < ?2"),
            params![id, version],
        )
        .map_err(db_err)?;
    }
    tx.commit().map_err(db_err)
}

async fn push_all(core: &Core, db: &Database) -> Result<usize, SyncError> {
    let mut total = 0;
    loop {
        let batch = core.with_db(|c| pending_uploads(c, &core.info.machine_id))?;
        if batch.is_empty() {
            return Ok(total);
        }
        core.sync.set_state("syncing");
        let results: Vec<_> = stream::iter(batch)
            .map(|u| {
                let coll = db.collection::<Document>(u.collection);
                async move { coll.replace_one(u.filter, u.doc).upsert(true).await.map(|_| (u.table, u.id, u.version)) }
            })
            .buffer_unordered(PARALLEL_UPLOADS)
            .collect()
            .await;
        let mut done = Vec::with_capacity(results.len());
        let mut first_err = None;
        for r in results {
            match r {
                Ok(d) => done.push(d),
                Err(e) => first_err = first_err.or(Some(e)),
            }
        }
        core.with_db(|c| mark_synced(c, &done))?;
        total += done.len();
        if let Some(e) = first_err {
            return Err(e.into());
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Download: first-run restore and old history
// ---------------------------------------------------------------------------------------------

fn s(d: &Document, k: &str) -> String {
    d.get_str(k).unwrap_or_default().to_string()
}

fn opt_s(d: &Document, k: &str) -> Option<String> {
    match d.get(k) {
        Some(Bson::String(v)) if !v.is_empty() => Some(v.clone()),
        _ => None,
    }
}

fn num(d: &Document, k: &str) -> f64 {
    match d.get(k) {
        Some(Bson::Double(v)) => *v,
        Some(Bson::Int32(v)) => *v as f64,
        Some(Bson::Int64(v)) => *v as f64,
        _ => 0.0,
    }
}

fn flag(d: &Document, k: &str) -> bool {
    d.get_bool(k).unwrap_or(false)
}

fn iso(d: &Document, k: &str) -> Option<String> {
    match d.get(k) {
        Some(Bson::DateTime(t)) => Some(time::millis_to_iso(t.timestamp_millis())),
        Some(Bson::String(v)) => Some(v.clone()),
        _ => None,
    }
}

fn doc_id(d: &Document, local_key: &str) -> String {
    opt_s(d, local_key).unwrap_or_else(|| match d.get("_id") {
        Some(Bson::String(v)) => v.clone(),
        Some(Bson::ObjectId(o)) => o.to_hex(),
        _ => store::new_id(),
    })
}

fn tx_from_doc(d: &Document) -> TransactionDTO {
    let audit = d
        .get_array("auditLog")
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_document())
                .map(|e| AuditEntry {
                    action: s(e, "action"),
                    previous_product_name: opt_s(e, "previousProductName"),
                    new_product_name: opt_s(e, "newProductName"),
                    previous_price: e.contains_key("previousPrice").then(|| num(e, "previousPrice")),
                    new_price: e.contains_key("newPrice").then(|| num(e, "newPrice")),
                    previous_consumer_no: opt_s(e, "previousConsumerNo"),
                    new_consumer_no: opt_s(e, "newConsumerNo"),
                    timestamp: iso(e, "timestamp").unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    TransactionDTO {
        id: doc_id(d, "_id"),
        date_string: s(d, "dateString"),
        kind: s(d, "type"),
        product_name: s(d, "productName"),
        price: num(d, "price"),
        consumer_no: opt_s(d, "consumerNo"),
        is_other_product: flag(d, "isOtherProduct"),
        status: opt_s(d, "status").unwrap_or_else(|| "ACTIVE".into()),
        is_day_finalized: flag(d, "isDayFinalized"),
        created_at: iso(d, "createdAt").unwrap_or_default(),
        audit_log: audit,
        synced: true,
    }
}

fn day_end_from_doc(d: &Document) -> DayEndDTO {
    let deliveries = d
        .get_array("deliveries")
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_document())
                .map(|e| DeliveryDTO {
                    delivery_boy_id: s(e, "deliveryBoyId"),
                    delivery_boy_name: s(e, "deliveryBoyName"),
                    bills_printed: num(e, "billsPrinted") as i64,
                    cylinders_delivered: num(e, "cylindersDelivered") as i64,
                })
                .collect()
        })
        .unwrap_or_default();
    let stock = d.get_document("cylinderStock").ok();
    let pair = |k: &str| {
        stock
            .and_then(|s| s.get_document(k).ok())
            .map(|p| StockPair { full: num(p, "full") as i64, empty: num(p, "empty") as i64 })
            .unwrap_or_default()
    };
    DayEndDTO {
        date_string: s(d, "dateString"),
        is_finalized: flag(d, "isFinalized"),
        finalized_at: iso(d, "finalizedAt"),
        deliveries,
        cylinder_stock: CylinderStock { kg5: pair("kg5"), kg14: pair("kg14"), kg19: pair("kg19") },
    }
}

async fn find_all(db: &Database, coll: &str, filter: Document) -> mongodb::error::Result<Vec<Document>> {
    db.collection::<Document>(coll).find(filter).await?.try_collect().await
}

/// First connection of a fresh install (e.g. Windows reinstalled on the same PC): bring back
/// this machine's delivery boys, catalog and the last RETENTION_DAYS of records. Rows already
/// on this PC always win.
async fn restore(core: &Core, db: &Database) -> Result<(), SyncError> {
    let mid = core.info.machine_id.as_str();
    let cutoff = time::retention_cutoff(&time::today());
    let boys = find_all(db, DELIVERY_BOYS, doc! { "machineId": mid }).await?;
    let products = find_all(db, PRODUCTS, doc! { "machineId": mid }).await?;
    let txs = find_all(db, TRANSACTIONS, doc! { "machineId": mid, "dateString": { "$gte": &cutoff } }).await?;
    let days = find_all(db, DAY_END_LOGS, doc! { "machineId": mid, "dateString": { "$gte": &cutoff } }).await?;

    let restored = core.with_db(|conn| {
        let tx = conn.transaction().map_err(db_err)?;
        let now = time::now_iso();
        let mut n = 0;
        for d in &boys {
            let created = iso(d, "createdAt").unwrap_or_else(|| now.clone());
            n += tx
                .execute(
                    "INSERT OR IGNORE INTO delivery_boys (id, name, is_active, created_at, updated_at, version, synced_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, 1, 1)",
                    params![doc_id(d, "_id"), s(d, "name"), d.get_bool("isActive").unwrap_or(true), created, iso(d, "updatedAt").unwrap_or_else(|| created.clone())],
                )
                .map_err(db_err)?;
        }
        for d in &products {
            let created = iso(d, "createdAt").unwrap_or_else(|| now.clone());
            n += tx
                .execute(
                    "INSERT OR IGNORE INTO products (id, name, last_used_price, created_at, updated_at, version, synced_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, 1, 1)",
                    params![doc_id(d, "localId"), s(d, "name"), num(d, "lastUsedPrice"), created, iso(d, "updatedAt").unwrap_or_else(|| created.clone())],
                )
                .map_err(db_err)?;
        }
        for d in &txs {
            let t = tx_from_doc(d);
            n += tx
                .execute(
                    "INSERT OR IGNORE INTO transactions (id, date_string, type, product_name, price, consumer_no, is_other_product,
                        status, is_day_finalized, audit_log, created_at, updated_at, version, synced_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1, 1)",
                    params![
                        t.id, t.date_string, t.kind, t.product_name, t.price, t.consumer_no, t.is_other_product, t.status,
                        t.is_day_finalized, serde_json::to_string(&t.audit_log).unwrap_or_default(), t.created_at,
                        iso(d, "updatedAt").unwrap_or_else(|| t.created_at.clone())
                    ],
                )
                .map_err(db_err)?;
        }
        for d in &days {
            let de = day_end_from_doc(d);
            let created = iso(d, "createdAt").unwrap_or_else(|| now.clone());
            n += tx
                .execute(
                    "INSERT OR IGNORE INTO day_end_logs (id, date_string, deliveries, cylinder_stock, is_finalized, finalized_at,
                        created_at, updated_at, version, synced_version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 1)",
                    params![
                        doc_id(d, "localId"), de.date_string, serde_json::to_string(&de.deliveries).unwrap_or_default(),
                        serde_json::to_string(&de.cylinder_stock).unwrap_or_default(), de.is_finalized, de.finalized_at,
                        created, iso(d, "updatedAt").unwrap_or_else(|| created.clone())
                    ],
                )
                .map_err(db_err)?;
        }
        db::meta_set(&tx, "restore_done", &now).map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(n)
    })?;
    if restored > 0 {
        eprintln!("[sync] restored {restored} records from the cloud for machine {mid}");
    }
    Ok(())
}

async fn cloud_day(core: &Core, date: &str) -> Result<(Vec<TransactionDTO>, Option<DayEndDTO>), SyncError> {
    let db = core.sync.remote().ok_or_else(|| SyncError::Local("Not connected to the cloud.".into()))?;
    let mid = core.info.machine_id.as_str();
    let fetch = async {
        let txs: Vec<Document> = db
            .collection::<Document>(TRANSACTIONS)
            .find(doc! { "machineId": mid, "dateString": date })
            .sort(doc! { "createdAt": -1 })
            .await?
            .try_collect()
            .await?;
        let day = db
            .collection::<Document>(DAY_END_LOGS)
            .find_one(doc! { "machineId": mid, "dateString": date })
            .await?;
        Ok::<_, mongodb::error::Error>((txs.iter().map(tx_from_doc).collect(), day.as_ref().map(day_end_from_doc)))
    };
    match tokio::time::timeout(Duration::from_secs(10), fetch).await {
        Ok(res) => Ok(res?),
        Err(_) => Err(SyncError::Local("The cloud took too long to answer.".into())),
    }
}

/// A past day for the History page. Days inside the local window come from this PC (works
/// offline). Older days are fetched from the cloud when online.
pub async fn history_day(core: &Core, date: &str) -> R<HistoryDayDTO> {
    let today = time::today();
    if !time::is_valid_date(date) || date > today.as_str() {
        return Err("Pick a valid date that isn't in the future.".into());
    }
    let (txs, day_end) = core.with_db(|c| Ok((store::transactions_for_date(c, date)?, store::get_day_end(c, date)?)))?;
    let cutoff = time::retention_cutoff(&today);
    let mut result = HistoryDayDTO {
        date: date.to_string(),
        today: today.clone(),
        source: "local",
        message: None,
        transactions: txs,
        day_end,
    };
    if date >= cutoff.as_str() || core.sync.config.is_none() {
        return Ok(result);
    }
    match cloud_day(core, date).await {
        Ok((txs, day_end)) => {
            result.source = "cloud";
            result.transactions = txs;
            result.day_end = day_end;
        }
        Err(_) if !result.transactions.is_empty() || result.day_end.is_some() => {
            result.message = Some("Offline: showing the copy still saved on this PC.".into());
        }
        Err(_) => {
            result.source = "unavailable";
            result.message = Some(format!(
                "This PC keeps the last {} days. Connect to the internet to load older days from the cloud.",
                time::RETENTION_DAYS
            ));
        }
    }
    Ok(result)
}
