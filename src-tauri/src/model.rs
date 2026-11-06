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
    /// Entries made for a delivery boy with v1.6–1.7 (the Role box, since removed): his ID and
    /// name at the time, and the cylinder count. None on every other entry.
    pub role_id: Option<String>,
    pub role_name: Option<String>,
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SoldProduct {
    pub name: String,
    pub quantity: i64,
    /// Money received for this product, by cash and online (v1.8.1+; earlier sheets had one total).
    #[serde(default)]
    pub cash: f64,
    #[serde(default)]
    pub online: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Expense {
    /// What it was for, as typed ("Fuel", "Service", "Tyre puncture"…).
    pub name: String,
    pub amount: f64,
}

/// Names offered while typing on the delivery boy cards, most used first. New names typed on a
/// card join the list once saved.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesSuggestions {
    pub products: Vec<String>,
    pub expenses: Vec<String>,
    /// Consumers seen on earlier system payments, with the name last typed for them.
    pub consumers: Vec<KnownConsumer>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownConsumer {
    pub consumer_no: String,
    pub name: String,
}

/// Settings shared by the office's PCs.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeSettingsDTO {
    /// The fixed price of a System payment (Day Register Cash In and delivery boys). None = not set yet.
    pub system_price: Option<f64>,
}

/// A system payment collected by a delivery boy: whose (consumer number, optional name) and how much.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemPayment {
    pub consumer_no: String,
    pub name: Option<String>,
    pub amount: f64,
}

/// One delivery boy's day: what he sold, the money he reported by payment type, and what he spent.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeliverySalesDTO {
    pub delivery_boy_id: String,
    pub delivery_boy_name: String,
    pub products: Vec<SoldProduct>,
    pub cash: f64,
    pub online: f64,
    // `cash` and `online` are the totals of the product lines.
    /// Total of `system_payments`.
    pub system: f64,
    pub system_payments: Vec<SystemPayment>,
    pub expenses: Vec<Expense>,
    /// The product he sold most recently before today (the Day End form's ghost product).
    pub usual_product: Option<String>,
    /// Who changed it last (operator name from Settings), and when. None = nothing saved yet.
    pub updated_by_user: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayEndPageDTO {
    pub date: String,
    pub day_end: DayEndDTO,
    pub transactions: Vec<TransactionDTO>,
    /// Every active delivery boy (plus any already saved today), by name.
    pub sales: Vec<DeliverySalesDTO>,
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
    pub sales: Vec<DeliverySalesDTO>,
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
