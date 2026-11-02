//! JSON shapes shared with the React UI (camelCase, see src/lib/types.ts).

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_product_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_product_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_consumer_no: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_consumer_no: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_payment_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_payment_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_quantity: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_quantity: Option<i64>,
    /// The operator's name from Settings on the PC that made this change (if set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// The PC that made this change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,
    pub timestamp: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDTO {
    pub id: String,
    pub date_string: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub product_name: String,
    pub price: f64,
    pub consumer_no: Option<String>,
    pub is_other_product: bool,
    /// "CASH" | "ONLINE" | "SYSTEM"
    pub payment_mode: String,
    /// Machine ID of the PC that made the entry.
    pub created_by: Option<String>,
    /// That PC's name, when known.
    pub created_by_name: Option<String>,
    /// The operator's name (Settings) who made the entry / changed it last.
    pub created_by_user: Option<String>,
    #[serde(skip_serializing)]
    pub updated_by_user: Option<String>,
    /// The delivery boy the entry is for (None = Office), and his name at the time.
    pub role_id: Option<String>,
    pub role_name: Option<String>,
    /// Cylinders, for delivery boy entries.
    pub quantity: Option<i64>,
    pub status: String,
    pub is_day_finalized: bool,
    pub created_at: String,
    pub audit_log: Vec<AuditEntry>,
    pub synced: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryDTO {
    pub delivery_boy_id: String,
    pub delivery_boy_name: String,
    pub bills_printed: i64,
    pub cylinders_delivered: i64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct StockPair {
    pub full: i64,
    pub empty: i64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct CylinderStock {
    pub kg5: StockPair,
    pub kg14: StockPair,
    pub kg19: StockPair,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayEndDTO {
    pub date_string: String,
    pub is_finalized: bool,
    pub finalized_at: Option<String>,
    pub deliveries: Vec<DeliveryDTO>,
    pub cylinder_stock: CylinderStock,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDTO {
    pub date: String,
    pub is_finalized: bool,
    pub transactions: Vec<TransactionDTO>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayEndPageDTO {
    pub date: String,
    pub day_end: DayEndDTO,
    pub transactions: Vec<TransactionDTO>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDayDTO {
    pub date: String,
    pub today: String,
    /// "local" | "cloud" | "unavailable"
    pub source: &'static str,
    pub message: Option<String>,
    pub transactions: Vec<TransactionDTO>,
    pub day_end: Option<DayEndDTO>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryBoyDTO {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    pub created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductSuggestion {
    pub name: String,
    pub last_used_price: f64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub machine_id: String,
    pub device_name: String,
    pub app_version: String,
    pub retention_days: i64,
    pub cloud_configured: bool,
    pub cloud_database: Option<String>,
    pub data_dir: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusDTO {
    /// "not_configured" | "connecting" | "syncing" | "online" | "offline" | "error"
    pub state: &'static str,
    pub pending: i64,
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
    /// Goes up each time changes from the office's other PCs arrive, so open screens reload.
    pub remote_changes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficePcDTO {
    pub machine_id: String,
    pub device_name: String,
    pub app_version: Option<String>,
    pub last_seen_at: Option<String>,
    pub user_name: Option<String>,
    pub is_this_pc: bool,
}
