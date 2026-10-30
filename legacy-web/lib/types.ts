import type { TransactionType } from "./models/Transaction";

export type AuditEntryDTO = {
  action: "CREATED" | "UPDATED" | "DELETED";
  previousProductName?: string;
  newProductName?: string;
  previousPrice?: number;
  newPrice?: number;
  previousConsumerNo?: string;
  newConsumerNo?: string;
  changedByName?: string;
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
  createdByName: string;
  status: "ACTIVE" | "EDITED" | "DELETED";
  isDayFinalized: boolean;
  createdAt: string;
  auditLog: AuditEntryDTO[];
};

export type DeliveryDTO = {
  deliveryBoyId: string | null;
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
  finalizedByName: string | null;
  deliveries: DeliveryDTO[];
  cylinderStock: CylinderStockDTO;
};

export type ActionResult<T = undefined> = { ok: true; data: T } | { ok: false; error: string };

export type Totals = { cashIn: number; cashOut: number; net: number; count: number };
