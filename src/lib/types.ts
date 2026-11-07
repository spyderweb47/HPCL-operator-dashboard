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
  previousQuantity?: number;
  newQuantity?: number;
  /** Operator name (Settings) and PC that made this change. */
  user?: string;
  machineId?: string;
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
  /** Machine ID of the PC that made the entry, and that PC's name when known. */
  createdBy: string | null;
  createdByName: string | null;
  /** Operator name (Settings) of whoever made the entry. */
  createdByUser: string | null;
  /** Entries made for a delivery boy with v1.6–1.7 (Role box, since removed): his ID and name, and the cylinder count. */
  roleId: string | null;
  roleName: string | null;
  quantity: number | null;
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

export type SoldProduct = { name: string; quantity: number };

/** name: what it was for, as typed ("Fuel", "Service", "Tyre puncture"…). */
export type Expense = { name: string; amount: number };

/** Names offered while typing on the delivery boy cards, most used first. */
export type SalesSuggestions = {
  products: string[];
  expenses: string[];
  /** Consumers seen on earlier system payments, with the name last typed for them. */
  consumers: { consumerNo: string; name: string }[];
};

/** A system payment a delivery boy collected: whose and how much. */
export type SystemPayment = { consumerNo: string; name: string | null; amount: number };

/** One delivery boy's day: what he sold, the money he reported by payment type, and what he spent. */
export type DeliverySalesDTO = {
  deliveryBoyId: string;
  deliveryBoyName: string;
  products: SoldProduct[];
  cash: number;
  online: number;
  /** Total of systemPayments. */
  system: number;
  systemPayments: SystemPayment[];
  expenses: Expense[];
  /** The product he sold most recently before today. */
  usualProduct: string | null;
  updatedByUser: string | null;
  /** null = nothing saved yet today. */
  updatedAt: string | null;
};

export type DayEndPageDTO = {
  date: string;
  dayEnd: DayEndDTO;
  transactions: TransactionDTO[];
  sales: DeliverySalesDTO[];
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
  sales: DeliverySalesDTO[];
};

export type DeliveryBoyDTO = { id: string; name: string; isActive: boolean; createdAt: string };

export type ProductSuggestion = { name: string; lastUsedPrice: number };

/** Settings → Saved Products. No price: only typed on the delivery boy sheet. */
export type SavedProductDTO = { name: string; lastUsedPrice: number | null };

export type SyncState = "not_configured" | "connecting" | "syncing" | "online" | "offline" | "error";

export type SyncStatusDTO = {
  state: SyncState;
  pending: number;
  lastSyncAt: string | null;
  lastError: string | null;
  /** Goes up whenever changes from the office's other PCs arrive. */
  remoteChanges: number;
};

export type OfficePcDTO = {
  machineId: string;
  deviceName: string;
  appVersion: string | null;
  lastSeenAt: string | null;
  userName: string | null;
  isThisPc: boolean;
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

/** Settings shared by the office's PCs. */
export type OfficeSettingsDTO = {
  /** The fixed price of a System payment. null = not set yet. */
  systemPrice: number | null;
};

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
