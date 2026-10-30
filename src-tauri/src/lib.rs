pub mod cloud;
pub mod core;
pub mod db;
pub mod model;
pub mod store;
pub mod time;

use serde_json::Value;
use std::sync::Arc;
use tauri::Manager;

/// The UI's single entry point into the backend (see src/lib/api.ts).
#[tauri::command]
async fn call(cmd: String, args: Value, core: tauri::State<'_, Arc<core::Core>>) -> Result<Value, String> {
    core::dispatch(&core, &cmd, args).await
}

pub fn run() {
    tauri::Builder::default()
        // Opening the app twice just brings the running window forward (one writer per database).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
            // Windows: %APPDATA%\com.hpcl.operator\operator.db
            let data_dir = app.path().app_data_dir()?;
            let config_dir = app.path().app_config_dir()?;
            let core = core::Core::open(&data_dir, &config_dir)?;
            tauri::async_runtime::spawn(cloud::run(core.clone()));
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![call])
        .run(tauri::generate_context!())
        .expect("error while running HPCL Operator");
}
