export type TransactionType = "CASH_IN" | "CASH_OUT";

/** How the money was paid. Cash unless ticked otherwise. */
export type PaymentMode = "CASH" | "ONLINE" | "SYSTEM";

export type AuditEntryDTO = {
  action: "CREATED" | "UPDATED" | "DELETED";
  previousProductName?: string;
  newProductName?: string;
  previousPrice?: number;
  newPrice?: number;
  previousConsumerNo?: string;
  newConsumerNo?: string;
  previousPaymentMode?: PaymentMode;
  newPaymentMode?: PaymentMode;
  timestamp: string;
};

export type TransactionDTO = {
  id: string;
  dateString: string;
  type: TransactionType;
  productName: string;
  price: number;
  consumerNo: string | null;
  isOtherProduct: boolean;
  paymentMode: PaymentMode;
  status: "ACTIVE" | "EDITED" | "DELETED";
  isDayFinalized: boolean;
  createdAt: string;
  auditLog: AuditEntryDTO[];
  /** false while the latest change is still waiting to be uploaded. */
  synced: boolean;
};

export type DeliveryDTO = {
  deliveryBoyId: string;
  deliveryBoyName: string;
  billsPrinted: number;
  cylindersDelivered: number;
};

export type StockPair = { full: number; empty: number };
export type CylinderStockDTO = { kg5: StockPair; kg14: StockPair; kg19: StockPair };

export type DayEndDTO = {
  dateString: string;
  isFinalized: boolean;
  finalizedAt: string | null;
  deliveries: DeliveryDTO[];
  cylinderStock: CylinderStockDTO;
};

export type RegisterDTO = {
  date: string;
  isFinalized: boolean;
  transactions: TransactionDTO[];
};

export type DayEndPageDTO = {
  date: string;
  dayEnd: DayEndDTO;
  transactions: TransactionDTO[];
};

/** Where a history day came from: this PC, the cloud (older than the local window), or nowhere (offline). */
export type DaySource = "local" | "cloud" | "unavailable";

export type HistoryDayDTO = {
  date: string;
  today: string;
  source: DaySource;
  message: string | null;
  transactions: TransactionDTO[];
  dayEnd: DayEndDTO | null;
};

export type DeliveryBoyDTO = { id: string; name: string; isActive: boolean; createdAt: string };

export type ProductSuggestion = { name: string; lastUsedPrice: number };

export type SyncState = "not_configured" | "connecting" | "syncing" | "online" | "offline" | "error";

export type SyncStatusDTO = {
  state: SyncState;
  pending: number;
  lastSyncAt: string | null;
  lastError: string | null;
};

export type AppInfoDTO = {
  machineId: string;
  deviceName: string;
  appVersion: string;
  retentionDays: number;
  cloudConfigured: boolean;
  cloudDatabase: string | null;
  dataDir: string;
};

export type ActionResult<T = undefined> = { ok: true; data: T } | { ok: false; error: string };

export type Totals = {
  /** All Cash In / Cash Out entries, whatever the payment type. */
  cashIn: number;
  cashOut: number;
  /** Money in the drawer: cash-paid entries only. */
  net: number;
  count: number;
  inByMode: Record<PaymentMode, number>;
  outByMode: Record<PaymentMode, number>;
  /** Any Online/System entries? (otherwise the totals look exactly as before) */
  hasNonCash: boolean;
};

/** Saved cloud settings, as safe to show: never contains the password. */
export type CloudSettingsDTO = {
  configured: boolean;
  source: "settings" | "environment" | null;
  connectionString: string;
  username: string;
  hasPassword: boolean;
  host: string;
  database: string;
};
