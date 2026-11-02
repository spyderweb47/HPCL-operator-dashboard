//! Cloud sync with MongoDB.
//!
//! The local SQLite file is always written first. This worker uploads every changed row
//! (`version > synced_version`) as a full-document upsert, so retries are harmless and nothing
//! is lost while offline. It wakes immediately after each local write, and retries every
//! 5 s → 30 s while the internet is down.
//!
//! Every PC using the same cloud database is one office: each one also downloads what the
//! others changed (every few seconds while online) and merges it into its own file, so all of
//! them show the same register, catalog, delivery boys and day end. `syncedAt` is stamped by the
//! database server, so PCs with different clocks never miss each other's changes.

use futures_util::{stream, StreamExt, TryStreamExt};
use mongodb::bson::{doc, Bson, DateTime as BsonDate, Document};
use mongodb::error::ErrorKind;
use mongodb::options::{ClientOptions, IndexOptions};
use mongodb::{Client, Database, IndexModel};
use rusqlite::{params, Connection};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;

use crate::cloud_config::{self, CloudConfig};
use crate::core::Core;
use crate::model::*;
use crate::store::{self, db_err, DayCopy, R};
use crate::{db, time};

const TRANSACTIONS: &str = "transactions";
const DAY_END_LOGS: &str = "dayendlogs";
const PRODUCTS: &str = "products";
const DELIVERY_BOYS: &str = "deliveryboys";
const MACHINES: &str = "machines";

const BATCH: i64 = 200;
const PARALLEL_UPLOADS: usize = 8;
/// How often an online PC looks for the other PCs' changes.
const LIVE_CHECK: Duration = Duration::from_secs(4);
const MACHINES_EVERY: Duration = Duration::from_secs(60);
const PULL_PAGE: i64 = 500;
/// Each download looks this far back again, for writes that became visible slightly late.
const PULL_OVERLAP_MS: i64 = 30_000;
const HEARTBEAT_EVERY: Duration = Duration::from_secs(5 * 60);
const PRUNE_EVERY_HOURS: i64 = 6;

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
    config: Mutex<Option<CloudConfig>>,
    /// Bumped whenever the cloud settings change, so work for the old address is discarded.
    generation: AtomicU64,
    inner: Mutex<Inner>,
    remote: Mutex<Option<(u64, Database)>>,
    remote_changes: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl SyncShared {
    pub fn new(config: Option<CloudConfig>) -> Self {
        let state = if config.is_some() { "connecting" } else { "not_configured" };
        SyncShared {
            notify: Notify::new(),
            config: Mutex::new(config),
            generation: AtomicU64::new(0),
            inner: Mutex::new(Inner { state, last_sync_at: None, last_error: None }),
            remote: Mutex::new(None),
            remote_changes: AtomicU64::new(0),
        }
    }

    pub fn config(&self) -> Option<CloudConfig> {
        lock(&self.config).clone()
    }

    /// Switch to new cloud settings (or none) right away; the worker reconnects immediately.
    pub fn set_config(&self, config: Option<CloudConfig>) {
        let state = if config.is_some() { "connecting" } else { "not_configured" };
        *lock(&self.config) = config;
        self.generation.fetch_add(1, Ordering::SeqCst);
        *lock(&self.remote) = None;
        *self.inner() = Inner { state, last_sync_at: None, last_error: None };
        self.notify.notify_one();
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        lock(&self.inner)
    }

    /// Status updates from a sync round only count if the settings didn't change meanwhile.
    fn update(&self, gen: u64, f: impl FnOnce(&mut Inner)) {
        let mut inner = self.inner();
        if self.generation() == gen {
            f(&mut inner);
        }
    }

    fn remote(&self) -> Option<Database> {
        match &*lock(&self.remote) {
            Some((gen, db)) if *gen == self.generation() => Some(db.clone()),
            _ => None,
        }
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
        remote_changes: core.sync.remote_changes.load(Ordering::Relaxed),
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
            | ErrorKind::ConnectionPoolCleared { .. } => (
                true,
                format!("Cannot reach the cloud database. Check the internet, and that Atlas Network Access allows this PC's IP. ({e})"),
            ),
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
    let mut failures = 0usize;
    let mut timers = Timers::default();
    let mut seen_gen = u64::MAX;
    loop {
        let gen = core.sync.generation();
        if gen != seen_gen {
            seen_gen = gen;
            failures = 0;
            timers = Timers::default();
        }
        let wait = match core.sync.config() {
            // Nothing to do until the operator saves cloud settings (which wakes us).
            None => Duration::from_secs(3600),
            Some(cfg) => match cycle(&core, &cfg, gen, &mut timers).await {
                Ok(()) => {
                    failures = 0;
                    LIVE_CHECK
                }
                Err(e) => {
                    let (offline, message) = describe(&e);
                    eprintln!("[sync] {message}");
                    core.sync.update(gen, |inner| {
                        inner.state = if offline { "offline" } else { "error" };
                        inner.last_error = Some(message);
                    });
                    failures += 1;
                    Duration::from_secs([5, 10, 20, 30][(failures - 1).min(3)])
                }
            },
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

#[derive(Default)]
struct Timers {
    heartbeat: Option<Instant>,
    machines: Option<Instant>,
}

async fn cycle(core: &Arc<Core>, cfg: &CloudConfig, gen: u64, timers: &mut Timers) -> Result<(), SyncError> {
    let db = match core.sync.remote() {
        Some(db) => db,
        None => {
            let db = connect(cfg).await?;
            core.with_db(|c| retarget(c, &cloud_config::target_key(cfg)))?;
            if core.sync.generation() != gen {
                return Ok(()); // settings changed while connecting; the next round uses the new ones
            }
            *lock(&core.sync.remote) = Some((gen, db.clone()));
            db
        }
    };
    // Other PCs' changes first, then ours: a row changed on both keeps its newest version.
    let pulled = pull_all(core, &db).await?;
    if pulled > 0 && core.sync.generation() == gen {
        core.sync.remote_changes.fetch_add(1, Ordering::Relaxed);
    }
    let pushed = push_all(core, &db).await?;
    if pushed > 0 || timers.heartbeat.is_none_or(|t| t.elapsed() > HEARTBEAT_EVERY) {
        heartbeat(core, &db).await?;
        timers.heartbeat = Some(Instant::now());
    }
    // New entries may come from a PC this one hasn't heard of yet: refresh the list then too.
    if pulled > 0 || timers.machines.is_none_or(|t| t.elapsed() > MACHINES_EVERY) {
        pull_machines(core, &db).await?;
        timers.machines = Some(Instant::now());
    }
    core.with_db(|c| maybe_prune(c))?;

    core.sync.update(gen, |inner| {
        inner.state = "online";
        inner.last_error = None;
        inner.last_sync_at = Some(time::now_iso());
    });
    Ok(())
}

/// First connection to a different cluster/database: upload everything on this PC there, and
/// download everything the office's PCs already have there.
fn retarget(conn: &mut Connection, key: &str) -> R<()> {
    if db::meta_get(conn, "cloud_target").map_err(db_err)?.as_deref() == Some(key) {
        return Ok(());
    }
    let tx = conn.transaction().map_err(db_err)?;
    for table in ["transactions", "day_end_logs", "products", "delivery_boys"] {
        tx.execute(&format!("UPDATE {table} SET synced_version = 0"), []).map_err(db_err)?;
    }
    tx.execute("DELETE FROM meta WHERE key = 'restore_done' OR key LIKE 'pull_cursor:%'", []).map_err(db_err)?;
    db::meta_set(&tx, "cloud_target", key).map_err(db_err)?;
    tx.commit().map_err(db_err)?;
    eprintln!("[sync] new cloud target {key}: re-uploading local data");
    Ok(())
}

/// Settings → "Test connection": connect, log in and read the database, without saving anything.
pub async fn test_connection(cfg: &CloudConfig) -> R<String> {
    let attempt = async {
        let mut opts = ClientOptions::parse(&cfg.uri).await?;
        opts.server_selection_timeout = Some(Duration::from_secs(10));
        opts.connect_timeout = Some(Duration::from_secs(10));
        let db = Client::with_options(opts)?.database(&cfg.database);
        db.run_command(doc! { "ping": 1 }).await?;
        db.list_collection_names().await?;
        Ok::<_, mongodb::error::Error>(())
    };
    match tokio::time::timeout(Duration::from_secs(25), attempt).await {
        Ok(Ok(())) => Ok(format!(
            "Connected to {} · database “{}”.",
            cloud_config::describe(Some(cfg)).host,
            cfg.database
        )),
        Ok(Err(e)) => Err(describe(&SyncError::Remote(e)).1),
        Err(_) => Err("No answer from the cloud database within 25 seconds.".into()),
    }
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
    // "What changed since my last look?"
    for coll in [TRANSACTIONS, DAY_END_LOGS, PRODUCTS, DELIVERY_BOYS] {
        db.collection::<Document>(coll).create_index(index(doc! { "syncedAt": 1, "_id": 1 }, false)).await?;
    }
    Ok(())
}

async fn heartbeat(core: &Core, db: &Database) -> Result<(), SyncError> {
    let (pending, user) = core.with_db(|c| Ok((store::pending_count(c)?, store::user_name(c)?)))?;
    let now = BsonDate::now();
    let info = &core.info;
    db.collection::<Document>(MACHINES)
        .update_one(
            doc! { "_id": &info.machine_id },
            doc! {
                "$set": {
                    "deviceName": &info.device_name,
                    // The operator's name from Settings (null = not set).
                    "userName": opt_str(&user),
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
    core.with_db(|c| {
        let tx = c.transaction().map_err(db_err)?;
        store::touch_this_pc(&tx, info)?;
        tx.commit().map_err(db_err)
    })?;
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
    put_s("previousPaymentMode", &a.previous_payment_mode);
    put_s("newPaymentMode", &a.new_payment_mode);
    put_s("user", &a.user);
    put_s("machineId", &a.machine_id);
    if let Some(q) = a.previous_quantity {
        d.insert("previousQuantity", q);
    }
    if let Some(q) = a.new_quantity {
        d.insert("newQuantity", q);
    }
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
                    "updatedBy": machine_id,
                    "createdAt": date(&r.get::<_, String>(3)?),
                    "updatedAt": date(&r.get::<_, String>(4)?),
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
            "SELECT id, date_string, deliveries, cylinder_stock, is_finalized, finalized_at, created_at, updated_at, version, stamps
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
            // When each count was typed, for merging copies from several PCs (cell names contain
            // dots, so a list rather than field names).
            let stamps = r
                .get::<_, Option<String>>(9)?
                .and_then(|s| serde_json::from_str::<store::Stamps>(&s).ok())
                .map(|s| Bson::Array(s.iter().map(|(cell, at)| Bson::Document(doc! { "cell": cell, "at": date(at) })).collect()))
                .unwrap_or(Bson::Null);
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
                    "stamps": stamps,
                    "createdAt": date(&r.get::<_, String>(6)?),
                    "updatedAt": date(&r.get::<_, String>(7)?),
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
                    audit_log, created_at, updated_at, version, payment_mode, created_by,
                    role_id, role_name, quantity, created_by_user, updated_by_user
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
                    // The PC that made the entry; `updatedBy` is the PC that changed it last.
                    "machineId": r.get::<_, Option<String>>(14)?.unwrap_or_else(|| machine_id.to_string()),
                    "updatedBy": machine_id,
                    // Operator names from Settings: who made the entry, and who changed it last.
                    "createdByUser": opt_str(&r.get(18)?),
                    "updatedByUser": opt_str(&r.get(19)?),
                    "dateString": r.get::<_, String>(1)?,
                    "type": r.get::<_, String>(2)?,
                    "productName": r.get::<_, String>(3)?,
                    "price": r.get::<_, f64>(4)?,
                    "consumerNo": opt_str(&r.get(5)?),
                    "isOtherProduct": r.get::<_, bool>(6)?,
                    "paymentMode": r.get::<_, String>(13)?,
                    // The delivery boy the entry is for (null = Office), and his cylinder count.
                    "roleId": opt_str(&r.get(15)?),
                    "roleName": opt_str(&r.get(16)?),
                    "quantity": r.get::<_, Option<i64>>(17)?.map(Bson::Int64).unwrap_or(Bson::Null),
                    "status": r.get::<_, String>(7)?,
                    "isDayFinalized": r.get::<_, bool>(8)?,
                    "auditLog": audit.iter().map(audit_doc).collect::<Vec<_>>(),
                    "createdAt": date(&r.get::<_, String>(10)?),
                    "updatedAt": date(&r.get::<_, String>(11)?),
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
        core.sync.inner().state = "syncing";
        let results: Vec<_> = stream::iter(batch)
            .map(|u| {
                let coll = db.collection::<Document>(u.collection);
                // The server stamps syncedAt ($$NOW), so the other PCs' "changed since" never depends
                // on this PC's clock. $literal keeps text like "$5" from being read as a field.
                let update = vec![doc! { "$replaceWith": { "$literal": u.doc } }, doc! { "$set": { "syncedAt": "$$NOW" } }];
                async move { coll.update_one(u.filter, update).upsert(true).await.map(|_| (u.table, u.id, u.version)) }
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
// Download: the office's other PCs, and old history
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
                    previous_payment_mode: opt_s(e, "previousPaymentMode"),
                    new_payment_mode: opt_s(e, "newPaymentMode"),
                    previous_quantity: e.contains_key("previousQuantity").then(|| num(e, "previousQuantity") as i64),
                    new_quantity: e.contains_key("newQuantity").then(|| num(e, "newQuantity") as i64),
                    user: opt_s(e, "user"),
                    machine_id: opt_s(e, "machineId"),
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
        created_by: opt_s(d, "machineId"),
        created_by_name: None,
        created_by_user: opt_s(d, "createdByUser"),
        updated_by_user: opt_s(d, "updatedByUser"),
        role_id: opt_s(d, "roleId"),
        role_name: opt_s(d, "roleName"),
        quantity: matches!(d.get("quantity"), Some(Bson::Int32(_) | Bson::Int64(_) | Bson::Double(_))).then(|| num(d, "quantity") as i64),
        // Entries uploaded before payment types existed were all cash.
        payment_mode: opt_s(d, "paymentMode")
            .filter(|m| store::PAYMENT_MODES.contains(&m.as_str()))
            .unwrap_or_else(|| "CASH".into()),
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

fn day_copy_from_doc(d: &Document) -> DayCopy {
    let stamps = match d.get("stamps") {
        Some(Bson::Array(a)) => Some(
            a.iter()
                .filter_map(|e| e.as_document())
                .filter_map(|e| Some((opt_s(e, "cell")?, iso(e, "at")?)))
                .collect::<store::Stamps>(),
        ),
        _ => None, // uploaded before v1.5
    };
    DayCopy { day: day_end_from_doc(d), stamps, updated_at: iso(d, "updatedAt").unwrap_or_default() }
}

/// Apply one page of downloaded documents in a single local transaction.
fn apply_docs(conn: &mut Connection, coll: &str, docs: &[Document]) -> R<usize> {
    let tx = conn.transaction().map_err(db_err)?;
    let mut changed = 0;
    for d in docs {
        let Some(updated_at) = iso(d, "updatedAt").or_else(|| iso(d, "createdAt")) else { continue };
        let created_at = iso(d, "createdAt").unwrap_or_else(|| updated_at.clone());
        let did = match coll {
            TRANSACTIONS => store::apply_remote_tx(&tx, &tx_from_doc(d), &updated_at)?,
            DELIVERY_BOYS => {
                let boy = DeliveryBoyDTO { id: doc_id(d, "_id"), name: s(d, "name"), is_active: d.get_bool("isActive").unwrap_or(true), created_at };
                store::apply_remote_delivery_boy(&tx, &boy, &updated_at)?
            }
            PRODUCTS => store::apply_remote_product(&tx, &s(d, "name"), num(d, "lastUsedPrice"), &created_at, &updated_at)?,
            DAY_END_LOGS => store::apply_remote_day(&tx, &day_copy_from_doc(d))?,
            _ => false,
        };
        changed += did as usize;
    }
    tx.commit().map_err(db_err)?;
    Ok(changed)
}

/// Download what changed in the cloud since the last look, from every PC of the office.
async fn pull_all(core: &Core, db: &Database) -> Result<usize, SyncError> {
    // The server's clock: the newest stamp a download can have seen.
    let hello = db.run_command(doc! { "hello": 1 }).await?;
    let server_now = hello.get_datetime("localTime").map(|t| t.timestamp_millis()).unwrap_or(i64::MAX);
    let mut changed = 0;
    // Delivery boys first: day-end counts refer to them.
    for coll in [DELIVERY_BOYS, PRODUCTS, TRANSACTIONS, DAY_END_LOGS] {
        changed += pull(core, db, coll, server_now).await?;
    }
    Ok(changed)
}

async fn pull(core: &Core, db: &Database, coll: &'static str, server_now: i64) -> Result<usize, SyncError> {
    let me = core.info.machine_id.as_str();
    let key = format!("pull_cursor:{coll}");
    let cursor: Option<i64> = core.with_db(|c| db::meta_get(c, &key).map_err(db_err))?.and_then(|v| v.parse().ok());
    let mut base = Document::new();
    match cursor {
        Some(t) => {
            base.insert("syncedAt", doc! { "$gte": BsonDate::from_millis(t - PULL_OVERLAP_MS) });
            // Skip what this PC uploaded itself.
            let by = if matches!(coll, PRODUCTS | DAY_END_LOGS) { "machineId" } else { "updatedBy" };
            base.insert(by, doc! { "$ne": me });
        }
        // First look (new PC, reinstall, other database): everything inside the local window.
        None if matches!(coll, TRANSACTIONS | DAY_END_LOGS) => {
            base.insert("dateString", doc! { "$gte": time::retention_cutoff(&time::today()) });
        }
        None => {}
    }
    let mut after: Option<(Bson, Bson)> = None;
    let mut newest = cursor;
    let mut changed = 0;
    loop {
        let filter = match &after {
            None => base.clone(),
            Some((t, id)) => doc! { "$and": [
                base.clone(),
                { "$or": [ { "syncedAt": { "$gt": t.clone() } }, { "syncedAt": t.clone(), "_id": { "$gt": id.clone() } } ] },
            ] },
        };
        let docs: Vec<Document> = db
            .collection::<Document>(coll)
            .find(filter)
            .sort(doc! { "syncedAt": 1, "_id": 1 })
            .limit(PULL_PAGE)
            .await?
            .try_collect()
            .await?;
        changed += core.with_db(|c| apply_docs(c, coll, &docs))?;
        for d in &docs {
            if let Ok(t) = d.get_datetime("syncedAt") {
                // Older app versions stamped syncedAt with their own clock: never trust it past the server's.
                let t = t.timestamp_millis().min(server_now);
                newest = Some(newest.map_or(t, |n| n.max(t)));
            }
        }
        match docs.last() {
            Some(last) if docs.len() as i64 == PULL_PAGE => {
                after = Some((last.get("syncedAt").cloned().unwrap_or(Bson::Null), last.get("_id").cloned().unwrap_or(Bson::Null)));
            }
            _ => break,
        }
    }
    if newest != cursor {
        if let Some(t) = newest {
            core.with_db(|c| db::meta_set(c, &key, &t.to_string()).map_err(db_err))?;
        }
    }
    if changed > 0 {
        eprintln!("[sync] {changed} change(s) from other PCs in {coll}");
    }
    Ok(changed)
}

/// The office's PCs, for Settings and "entered on" labels.
async fn pull_machines(core: &Core, db: &Database) -> Result<(), SyncError> {
    let docs = find_all(db, MACHINES, doc! {}).await?;
    core.with_db(|c| {
        let tx = c.transaction().map_err(db_err)?;
        for d in &docs {
            let id = doc_id(d, "_id");
            let (name, version, last_seen, user) = (s(d, "deviceName"), opt_s(d, "appVersion"), iso(d, "lastSeenAt"), opt_s(d, "userName"));
            store::apply_remote_machine(
                &tx,
                &store::MachineInfo {
                    id: &id,
                    device_name: &name,
                    app_version: version.as_deref(),
                    last_seen_at: last_seen.as_deref(),
                    user_name: user.as_deref(),
                },
            )?;
        }
        tx.commit().map_err(db_err)
    })?;
    Ok(())
}

async fn cloud_day(core: &Core, date: &str) -> Result<(Vec<TransactionDTO>, Option<DayEndDTO>), SyncError> {
    let db = core.sync.remote().ok_or_else(|| SyncError::Local("Not connected to the cloud.".into()))?;
    // The whole office's entries; each PC's copy of the day-end counts merged into one.
    let fetch = async {
        let txs: Vec<Document> = db
            .collection::<Document>(TRANSACTIONS)
            .find(doc! { "dateString": date })
            .sort(doc! { "createdAt": -1 })
            .await?
            .try_collect()
            .await?;
        let days = find_all(&db, DAY_END_LOGS, doc! { "dateString": date }).await?;
        let day = days.iter().map(day_copy_from_doc).reduce(|a, b| store::merge_day(&a, &b)).map(|d| d.day);
        Ok::<_, mongodb::error::Error>((txs.iter().map(tx_from_doc).collect::<Vec<_>>(), day))
    };
    match tokio::time::timeout(Duration::from_secs(10), fetch).await {
        Ok(Ok((mut txs, day))) => {
            let names: std::collections::HashMap<String, String> =
                core.with_db(|c| store::office_pcs(c))?.into_iter().map(|p| (p.machine_id, p.device_name)).collect();
            for t in &mut txs {
                t.created_by_name = t.created_by.as_ref().and_then(|id| names.get(id).cloned());
            }
            Ok((txs, day))
        }
        Ok(Err(e)) => Err(e.into()),
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
    if date >= cutoff.as_str() || core.sync.config().is_none() {
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
