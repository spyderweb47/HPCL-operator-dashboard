//! The backend shared by the Tauri window and the dev HTTP bridge: one local database,
//! one sync worker, and a single command dispatcher.

use rusqlite::Connection;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::cloud::{self, SyncShared};
use crate::model::AppInfo;
use crate::store::{self, db_err, R};
use crate::{db, time};

pub struct Core {
    db: Mutex<Connection>,
    pub info: AppInfo,
    pub sync: SyncShared,
}

impl Core {
    /// Opens (or creates) `operator.db` in `data_dir` and reads the cloud settings from `config_dir`.
    pub fn open(data_dir: &Path, config_dir: &Path) -> R<Arc<Core>> {
        std::fs::create_dir_all(data_dir).map_err(|e| format!("Cannot create {}: {e}", data_dir.display()))?;
        let conn = db::open(&data_dir.join("operator.db")).map_err(db_err)?;
        let machine_id = machine_id(&conn).map_err(db_err)?;
        let config = cloud::load_config(config_dir);
        let info = AppInfo {
            machine_id,
            device_name: gethostname::gethostname().to_string_lossy().into_owned(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            retention_days: time::RETENTION_DAYS,
            cloud_configured: config.is_some(),
            cloud_database: config.as_ref().map(|c| c.database.clone()),
            data_dir: data_dir.display().to_string(),
        };
        Ok(Arc::new(Core { db: Mutex::new(conn), info, sync: SyncShared::new(config) }))
    }

    /// Short, synchronous access to the local database. Never held across an `.await`.
    pub fn with_db<T>(&self, f: impl FnOnce(&mut Connection) -> R<T>) -> R<T> {
        let mut conn = self.db.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut conn)
    }
}

/// A stable ID for this installation, derived from the Windows machine GUID (so reinstalling
/// on the same PC keeps the same ID and can restore its data) and stored in the database.
fn machine_id(conn: &Connection) -> rusqlite::Result<String> {
    if let Some(id) = db::meta_get(conn, "machine_id")? {
        return Ok(id);
    }
    let seed = machine_uid::get()
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(store::new_id);
    let hash = Sha256::digest(format!("hpcl-operator:{seed}").as_bytes());
    let hex: String = hash.iter().take(8).map(|b| format!("{b:02X}")).collect();
    let id = format!("{}-{}-{}-{}", &hex[0..4], &hex[4..8], &hex[8..12], &hex[12..16]);
    db::meta_set(conn, "machine_id", &id)?;
    Ok(id)
}

fn arg<T: DeserializeOwned>(args: Value) -> R<T> {
    serde_json::from_value(args).map_err(|e| format!("Bad request: {e}"))
}

fn out<T: Serialize>(v: T) -> R<Value> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct Id {
    id: String,
}
#[derive(Deserialize)]
struct Query {
    q: String,
}
#[derive(Deserialize)]
struct Date {
    date: String,
}
#[derive(Deserialize)]
struct Name {
    name: String,
}
#[derive(Deserialize)]
struct Rename {
    id: String,
    name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Active {
    id: String,
    is_active: bool,
}

pub async fn dispatch(core: &Arc<Core>, cmd: &str, args: Value) -> R<Value> {
    // Writes go to the local file first; the sync worker is woken to upload them right away.
    let write = |res: R<Value>| {
        if res.is_ok() {
            core.sync.notify.notify_one();
        }
        res
    };
    match cmd {
        "app_info" => out(&core.info),
        "sync_status" => out(cloud::status(core)),
        "sync_now" => {
            core.sync.notify.notify_one();
            Ok(Value::Null)
        }

        "get_register" => out(core.with_db(|c| store::register(c))?),
        "search_products" => {
            let a: Query = arg(args)?;
            out(core.with_db(|c| store::search_products(c, &a.q))?)
        }
        "create_transaction" => write(out(core.with_db(|c| store::create_transaction(c, arg(args)?))?)),
        "update_transaction" => write(out(core.with_db(|c| store::update_transaction(c, arg(args)?))?)),
        "delete_transaction" => {
            let a: Id = arg(args)?;
            write(out(core.with_db(|c| store::delete_transaction(c, &a.id))?))
        }

        "get_day_end" => out(core.with_db(|c| store::day_end_page(c))?),
        "save_day_end" => write(core.with_db(|c| store::save_day_end(c, arg(args)?))),
        "finalize_day" => write(out(core.with_db(|c| store::finalize_day(c, arg(args)?))?)),

        "get_history_day" => {
            let a: Date = arg(args)?;
            out(cloud::history_day(core, &a.date).await?)
        }

        "list_delivery_boys" => out(core.with_db(|c| store::list_delivery_boys(c))?),
        "add_delivery_boy" => {
            let a: Name = arg(args)?;
            write(out(core.with_db(|c| store::add_delivery_boy(c, &a.name))?))
        }
        "rename_delivery_boy" => {
            let a: Rename = arg(args)?;
            write(out(core.with_db(|c| store::rename_delivery_boy(c, &a.id, &a.name))?))
        }
        "set_delivery_boy_active" => {
            let a: Active = arg(args)?;
            write(out(core.with_db(|c| store::set_delivery_boy_active(c, &a.id, a.is_active))?))
        }

        _ => Err(format!("Unknown command: {cmd}")),
    }
}
