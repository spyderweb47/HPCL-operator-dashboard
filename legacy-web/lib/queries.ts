import "server-only";
import { connectDB } from "./db";
import { DayEndLog, DeliveryBoy, Transaction } from "./models";
import type { CylinderStockDTO, DayEndDTO, DeliveryDTO, Totals, TransactionDTO } from "./types";

/* eslint-disable @typescript-eslint/no-explicit-any */

export function toTransactionDTO(t: any): TransactionDTO {
  return {
    id: String(t._id),
    dateString: t.dateString,
    type: t.type,
    productName: t.productName,
    price: t.price,
    consumerNo: t.consumerNo ?? null,
    isOtherProduct: !!t.isOtherProduct,
    createdByName: t.createdByName ?? "",
    status: t.status,
    isDayFinalized: !!t.isDayFinalized,
    createdAt: new Date(t.createdAt).toISOString(),
    auditLog: (t.auditLog ?? []).map((a: any) => ({
      action: a.action,
      previousProductName: a.previousProductName ?? undefined,
      newProductName: a.newProductName ?? undefined,
      previousPrice: a.previousPrice ?? undefined,
      newPrice: a.newPrice ?? undefined,
      previousConsumerNo: a.previousConsumerNo ?? undefined,
      newConsumerNo: a.newConsumerNo ?? undefined,
      changedByName: a.changedByName ?? undefined,
      timestamp: new Date(a.timestamp).toISOString(),
    })),
  };
}

const emptyStock = (): CylinderStockDTO => ({
  kg5: { full: 0, empty: 0 },
  kg14: { full: 0, empty: 0 },
  kg19: { full: 0, empty: 0 },
});

export async function getTransactionsForDate(dateString: string): Promise<TransactionDTO[]> {
  await connectDB();
  const docs = await Transaction.find({ dateString }).sort({ createdAt: -1 }).lean();
  return docs.map(toTransactionDTO);
}

/** Deleted rows are kept for audit but never count toward totals. */
export function computeTotals(rows: TransactionDTO[]): Totals {
  let cashIn = 0;
  let cashOut = 0;
  let count = 0;
  for (const r of rows) {
    if (r.status === "DELETED") continue;
    count++;
    if (r.type === "CASH_IN") cashIn += r.price;
    else cashOut += r.price;
  }
  // round to paise to avoid float drift in display
  const round = (n: number) => Math.round(n * 100) / 100;
  return { cashIn: round(cashIn), cashOut: round(cashOut), net: round(cashIn - cashOut), count };
}

export async function getDayEnd(dateString: string): Promise<DayEndDTO | null> {
  await connectDB();
  const log: any = await DayEndLog.findOne({ dateString }).populate("finalizedBy", "name username").lean();
  if (!log) return null;
  const stock = { ...emptyStock(), ...(log.cylinderStock ?? {}) };
  return {
    dateString: log.dateString,
    isFinalized: !!log.isFinalized,
    finalizedAt: log.finalizedAt ? new Date(log.finalizedAt).toISOString() : null,
    finalizedByName: log.finalizedBy ? log.finalizedBy.name || log.finalizedBy.username : null,
    deliveries: (log.deliveries ?? []).map((d: any) => ({
      deliveryBoyId: d.deliveryBoyId ? String(d.deliveryBoyId) : null,
      deliveryBoyName: d.deliveryBoyName,
      billsPrinted: d.billsPrinted ?? 0,
      cylindersDelivered: d.cylindersDelivered ?? 0,
    })),
    cylinderStock: {
      kg5: { full: stock.kg5?.full ?? 0, empty: stock.kg5?.empty ?? 0 },
      kg14: { full: stock.kg14?.full ?? 0, empty: stock.kg14?.empty ?? 0 },
      kg19: { full: stock.kg19?.full ?? 0, empty: stock.kg19?.empty ?? 0 },
    },
  };
}

export function blankDayEnd(dateString: string): DayEndDTO {
  return {
    dateString,
    isFinalized: false,
    finalizedAt: null,
    finalizedByName: null,
    deliveries: [],
    cylinderStock: emptyStock(),
  };
}

export async function isDayFinalized(dateString: string) {
  await connectDB();
  const log = await DayEndLog.findOne({ dateString }).select("isFinalized").lean();
  return !!log?.isFinalized;
}

/** Active delivery boys from the Owner Dashboard roster. */
export async function getActiveDeliveryBoys(): Promise<{ id: string; name: string }[]> {
  await connectDB();
  const boys = await DeliveryBoy.find({ isActive: { $ne: false } }).select("name").sort({ name: 1 }).lean();
  return boys.map((b) => ({ id: String(b._id), name: b.name }));
}

/**
 * Day-end rows = every active delivery boy (with today's saved counts, or zeros)
 * plus anyone already saved today who has since been deactivated, so no counts disappear.
 */
export function mergeRoster(roster: { id: string; name: string }[], saved: DeliveryDTO[]): DeliveryDTO[] {
  const byId = new Map(saved.filter((d) => d.deliveryBoyId).map((d) => [d.deliveryBoyId!, d]));
  const rows: DeliveryDTO[] = roster.map((b) => {
    const s = byId.get(b.id);
    byId.delete(b.id);
    return { deliveryBoyId: b.id, deliveryBoyName: b.name, billsPrinted: s?.billsPrinted ?? 0, cylindersDelivered: s?.cylindersDelivered ?? 0 };
  });
  const leftovers = saved.filter((d) => !d.deliveryBoyId || byId.has(d.deliveryBoyId));
  return rows.concat(leftovers);
}
