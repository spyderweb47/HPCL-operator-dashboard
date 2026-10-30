"use server";

import { Types } from "mongoose";
import { refresh } from "next/cache";
import { AuthError, requireActiveUser } from "@/lib/auth";
import { connectDB } from "@/lib/db";
import { todayString } from "@/lib/date";
import { PRODUCT_COLLATION, Product, Transaction, TRANSACTION_TYPES, type TransactionType } from "@/lib/models";
import { isDayFinalized, toTransactionDTO } from "@/lib/queries";
import type { ActionResult, TransactionDTO } from "@/lib/types";

const MAX_PRICE = 10_000_000;

function cleanName(raw: unknown) {
  return String(raw ?? "").replace(/\s+/g, " ").trim().slice(0, 120);
}

function parsePrice(raw: unknown): number | null {
  if (raw === null || raw === undefined || raw === "") return null;
  const n = typeof raw === "number" ? raw : Number(String(raw).trim());
  if (!Number.isFinite(n) || n <= 0 || n > MAX_PRICE) return NaN;
  return Math.round(n * 100) / 100;
}

const CONSUMER_NO_DIGITS = 6;

/** Exactly 6 digits (spaces/dashes from copy-paste are stripped). "" -> null; invalid -> false. */
function parseConsumerNo(raw: unknown): string | null | false {
  const s = String(raw ?? "").replace(/[\s-]/g, "");
  if (!s) return null;
  if (!new RegExp(`^\\d{${CONSUMER_NO_DIGITS}}$`).test(s)) return false;
  return s;
}

const CONSUMER_REQUIRED = `Enter the ${CONSUMER_NO_DIGITS}-digit consumer number, or tick “Other product”.`;
const CONSUMER_INVALID = `Consumer number must be exactly ${CONSUMER_NO_DIGITS} digits.`;

function fail(err: unknown): { ok: false; error: string } {
  if (err instanceof AuthError) return { ok: false, error: err.message };
  console.error(err);
  return { ok: false, error: "Something went wrong. Please try again." };
}

/**
 * Adaptive catalog: find the product case-insensitively, create it if new,
 * and record the price that was just used so the next entry gets it as the ghost price.
 * Returns the canonical (stored) product name.
 */
async function touchProduct(name: string, price: number): Promise<string> {
  const upsert = () =>
    Product.findOneAndUpdate(
      { name },
      { $set: { lastUsedPrice: price }, $setOnInsert: { name } },
      { upsert: true, returnDocument: "after", collation: PRODUCT_COLLATION }
    ).lean();
  try {
    return (await upsert())?.name ?? name;
  } catch (err) {
    // Two operators adding the same new item at once: the loser retries and updates the winner's doc.
    if ((err as { code?: number }).code === 11000) return (await upsert())?.name ?? name;
    throw err;
  }
}

export type CreateTransactionInput = {
  type: TransactionType;
  productName: string;
  /** Omit/empty to use the product's lastUsedPrice (the ghost price fast path). */
  price?: number | string | null;
  /** Cash In only. Required unless isOtherProduct. */
  consumerNo?: string | null;
  isOtherProduct?: boolean;
};

export async function createTransaction(input: CreateTransactionInput): Promise<ActionResult<TransactionDTO>> {
  try {
    const user = await requireActiveUser();
    const type: TransactionType = TRANSACTION_TYPES.includes(input.type) ? input.type : "CASH_IN";
    const name = cleanName(input.productName);
    if (!name) return { ok: false, error: "Enter a product / item name." };

    let price = parsePrice(input.price);
    if (Number.isNaN(price)) return { ok: false, error: "Enter a valid price greater than 0." };

    const isCashIn = type === "CASH_IN";
    const isOtherProduct = isCashIn && input.isOtherProduct === true;
    const consumerNo = isCashIn ? parseConsumerNo(input.consumerNo) : null;
    if (consumerNo === false) return { ok: false, error: CONSUMER_INVALID };
    if (isCashIn && !isOtherProduct && !consumerNo) return { ok: false, error: CONSUMER_REQUIRED };

    const dateString = todayString();
    await connectDB();
    if (await isDayFinalized(dateString)) {
      return { ok: false, error: "Today's register is finalized. No more entries can be added." };
    }

    if (price === null) {
      const existing = await Product.findOne({ name }).collation(PRODUCT_COLLATION).lean();
      if (!existing) return { ok: false, error: `"${name}" is a new item — enter its price.` };
      price = existing.lastUsedPrice;
    }

    const productName = await touchProduct(name, price);

    const tx = await Transaction.create({
      dateString,
      type,
      productName,
      price,
      consumerNo,
      isOtherProduct,
      createdBy: user.id,
      createdByName: user.name,
      status: "ACTIVE",
      isDayFinalized: false,
      auditLog: [
        {
          action: "CREATED",
          newProductName: productName,
          newPrice: price,
          newConsumerNo: consumerNo ?? undefined,
          changedBy: user.id,
          changedByName: user.name,
          timestamp: new Date(),
        },
      ],
    });

    refresh();
    return { ok: true, data: toTransactionDTO(tx.toObject()) };
  } catch (err) {
    return fail(err);
  }
}

/** Shared guard for edit/delete: only today's, non-finalized, non-deleted rows. */
async function loadEditable(id: string) {
  if (!Types.ObjectId.isValid(id)) return { error: "Transaction not found." } as const;
  const tx = await Transaction.findById(id).lean();
  if (!tx) return { error: "Transaction not found." } as const;
  if (tx.isDayFinalized || (await isDayFinalized(tx.dateString))) {
    return { error: "This day is finalized and can no longer be changed." } as const;
  }
  if (tx.dateString !== todayString()) return { error: "Only today's entries can be changed." } as const;
  if (tx.status === "DELETED") return { error: "This entry was already deleted." } as const;
  return { tx } as const;
}

export async function updateTransaction(
  id: string,
  input: { productName: string; price: number | string; consumerNo?: string | null }
): Promise<ActionResult> {
  try {
    const user = await requireActiveUser();
    await connectDB();
    const loaded = await loadEditable(id);
    if ("error" in loaded) return { ok: false, error: loaded.error! };
    const { tx } = loaded;

    const name = cleanName(input.productName);
    if (!name) return { ok: false, error: "Product / item name cannot be empty." };
    const price = parsePrice(input.price);
    if (price === null || Number.isNaN(price)) return { ok: false, error: "Enter a valid price greater than 0." };

    // Consumer number only applies to Cash In; the Other-product flag is fixed at entry time.
    let consumerNo: string | null = tx.consumerNo ?? null;
    if (tx.type === "CASH_IN") {
      const parsed = parseConsumerNo(input.consumerNo);
      if (parsed === false) return { ok: false, error: CONSUMER_INVALID };
      if (!parsed && !tx.isOtherProduct) return { ok: false, error: "Consumer number is required for this entry." };
      consumerNo = parsed;
    }
    const prevConsumerNo = tx.consumerNo ?? null;

    if (name === tx.productName && price === tx.price && consumerNo === prevConsumerNo) {
      return { ok: true, data: undefined };
    }

    // Only touch the catalog when the product or price actually changed.
    const productName = name === tx.productName && price === tx.price ? tx.productName : await touchProduct(name, price);

    // Optimistic concurrency: only apply if nobody else changed the row since we read it.
    const res = await Transaction.updateOne(
      {
        _id: tx._id,
        productName: tx.productName,
        price: tx.price,
        consumerNo: prevConsumerNo,
        status: { $ne: "DELETED" },
        isDayFinalized: false,
      },
      {
        $set: { productName, price, consumerNo, status: "EDITED" },
        $push: {
          auditLog: {
            action: "UPDATED",
            previousProductName: tx.productName,
            newProductName: productName,
            previousPrice: tx.price,
            newPrice: price,
            previousConsumerNo: prevConsumerNo ?? undefined,
            newConsumerNo: consumerNo ?? undefined,
            changedBy: user.id,
            changedByName: user.name,
            timestamp: new Date(),
          },
        },
      }
    );
    if (res.modifiedCount !== 1) {
      return { ok: false, error: "This entry was changed by someone else. Refresh and try again." };
    }

    refresh();
    return { ok: true, data: undefined };
  } catch (err) {
    return fail(err);
  }
}

/** Soft delete — the document is never removed, only flagged and audited. */
export async function deleteTransaction(id: string): Promise<ActionResult> {
  try {
    const user = await requireActiveUser();
    await connectDB();
    const loaded = await loadEditable(id);
    if ("error" in loaded) return { ok: false, error: loaded.error! };
    const { tx } = loaded;

    const res = await Transaction.updateOne(
      { _id: tx._id, status: { $ne: "DELETED" }, isDayFinalized: false },
      {
        $set: { status: "DELETED" },
        $push: {
          auditLog: {
            action: "DELETED",
            previousProductName: tx.productName,
            previousPrice: tx.price,
            previousConsumerNo: tx.consumerNo ?? undefined,
            changedBy: user.id,
            changedByName: user.name,
            timestamp: new Date(),
          },
        },
      }
    );
    if (res.modifiedCount !== 1) return { ok: false, error: "This entry could not be deleted. Refresh and try again." };

    refresh();
    return { ok: true, data: undefined };
  } catch (err) {
    return fail(err);
  }
}
