//! Development only: serves the real backend over HTTP so the UI can run in a normal browser
//! (`npm run bridge` + `npm run dev`). Not part of the installed app.

use axum::{extract::State, routing::post, Json, Router};
use hpcl_operator_lib::{cloud, core};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};

#[derive(Deserialize)]
struct Req {
    cmd: String,
    #[serde(default)]
    args: Value,
}

async fn handle(State(core): State<Arc<core::Core>>, Json(req): Json<Req>) -> Json<Value> {
    Json(match core::dispatch(&core, &req.cmd, req.args).await {
        Ok(data) => json!({ "ok": true, "data": data }),
        Err(error) => json!({ "ok": false, "error": error }),
    })
}

#[tokio::main]
async fn main() {
    let dir = std::env::var("HPCL_DATA_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("../.dev-data"));
    let core = core::Core::open(&dir, &dir).expect("open local database");
    println!("data: {}  machine: {}  cloud: {:?}", dir.display(), core.info.machine_id, core.info.cloud_database);
    tokio::spawn(cloud::run(core.clone()));
    let port = std::env::var("HPCL_BRIDGE_PORT").unwrap_or_else(|_| "1430".into());
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await.expect("bind");
    println!("bridge listening on http://127.0.0.1:{port}");
    axum::serve(listener, Router::new().route("/bridge/call", post(handle)).with_state(core)).await.unwrap();
}
