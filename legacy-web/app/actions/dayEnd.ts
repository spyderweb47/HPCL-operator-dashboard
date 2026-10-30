"use server";

import { refresh } from "next/cache";
import { AuthError, requireActiveUser } from "@/lib/auth";
import { connectDB } from "@/lib/db";
import { todayString } from "@/lib/date";
import { Types } from "mongoose";
import { DayEndLog, DeliveryBoy, Transaction } from "@/lib/models";
import type { ActionResult, CylinderStockDTO, DeliveryDTO } from "@/lib/types";

export type DayEndInput = {
  deliveries: DeliveryDTO[];
  cylinderStock: CylinderStockDTO;
};

const count = (v: unknown) => {
  const n = Math.floor(Number(v));
  return Number.isFinite(n) && n >= 0 ? Math.min(n, 1_000_000) : 0;
};

/**
 * Delivery rows must reference the Owner Dashboard roster (or someone already saved for
 * today who was deactivated since). Names come from the DB, never from the client.
 */
async function sanitize(input: DayEndInput, dateString: string): Promise<{ error: string } | { value: DayEndInput }> {
  const [roster, existing] = await Promise.all([
    DeliveryBoy.find({ isActive: { $ne: false } }).select("name").lean(),
    DayEndLog.findOne({ dateString }).select("deliveries").lean(),
  ]);
  const allowed = new Map<string, string>(); // key -> canonical name
  for (const d of existing?.deliveries ?? []) {
    allowed.set(d.deliveryBoyId ? `id:${d.deliveryBoyId}` : `name:${d.deliveryBoyName.toLowerCase()}`, d.deliveryBoyName);
  }
  for (const b of roster) allowed.set(`id:${b._id}`, b.name); // current roster name wins (owner renames)

  const seen = new Set<string>();
  const deliveries: DeliveryDTO[] = [];
  for (const d of input.deliveries ?? []) {
    const id = d.deliveryBoyId && Types.ObjectId.isValid(d.deliveryBoyId) ? d.deliveryBoyId : null;
    const key = id ? `id:${id}` : `name:${String(d.deliveryBoyName ?? "").trim().toLowerCase()}`;
    const name = allowed.get(key);
    if (!name) return { error: "A delivery boy in this list is no longer available. Reload the page." };
    if (seen.has(key)) return { error: `"${name}" is listed more than once.` };
    seen.add(key);
    deliveries.push({
      deliveryBoyId: id,
      deliveryBoyName: name,
      billsPrinted: count(d.billsPrinted),
      cylindersDelivered: count(d.cylindersDelivered),
    });
  }
  const s = input.cylinderStock ?? ({} as CylinderStockDTO);
  const pair = (p?: { full: number; empty: number }) => ({ full: count(p?.full), empty: count(p?.empty) });
  return {
    value: { deliveries, cylinderStock: { kg5: pair(s.kg5), kg14: pair(s.kg14), kg19: pair(s.kg19) } },
  };
}

function fail(err: unknown): { ok: false; error: string } {
  if (err instanceof AuthError) return { ok: false, error: err.message };
  console.error(err);
  return { ok: false, error: "Something went wrong. Please try again." };
}

async function saveToday(value: DayEndInput) {
  const dateString = todayString();
  // Filter on isFinalized:false so a finalized log can never be overwritten.
  // A concurrent finalize makes the upsert hit the unique index -> treated as finalized.
  try {
    const res = await DayEndLog.updateOne(
      { dateString, isFinalized: false },
      { $set: { deliveries: value.deliveries, cylinderStock: value.cylinderStock }, $setOnInsert: { dateString } },
      { upsert: true }
    );
    return res.matchedCount + res.upsertedCount === 1;
  } catch (err) {
    if ((err as { code?: number }).code === 11000) return false;
    throw err;
  }
}

export async function saveDayEndProgress(input: DayEndInput): Promise<ActionResult<{ savedAt: string }>> {
  try {
    await requireActiveUser();
    await connectDB();
    const parsed = await sanitize(input, todayString());
    if ("error" in parsed) return { ok: false, error: parsed.error };
    if (!(await saveToday(parsed.value))) return { ok: false, error: "Today is already finalized." };
    refresh();
    return { ok: true, data: { savedAt: new Date().toISOString() } };
  } catch (err) {
    return fail(err);
  }
}

/** Saves the final numbers, locks the day log, then locks every transaction of the day. */
export async function finalizeDay(input: DayEndInput): Promise<ActionResult> {
  try {
    const user = await requireActiveUser();
    await connectDB();
    const dateString = todayString();
    const parsed = await sanitize(input, dateString);
    if ("error" in parsed) return { ok: false, error: parsed.error };

    if (!(await saveToday(parsed.value))) return { ok: false, error: "Today is already finalized." };

    const res = await DayEndLog.updateOne(
      { dateString, isFinalized: false },
      { $set: { isFinalized: true, finalizedBy: user.id, finalizedAt: new Date() } }
    );
    if (res.modifiedCount !== 1) return { ok: false, error: "Today is already finalized." };

    await Transaction.updateMany({ dateString }, { $set: { isDayFinalized: true } });

    refresh();
    return { ok: true, data: undefined };
  } catch (err) {
    return fail(err);
  }
}
