import { splitOf } from "./payment";
import type { PaymentMode, Totals, TransactionDTO } from "./types";

// round to paise to avoid float drift in display
const round = (n: number) => Math.round(n * 100) / 100;
const zero = (): Record<PaymentMode, number> => ({ CASH: 0, ONLINE: 0, SYSTEM: 0 });

/** Deleted rows are kept for audit but never count toward totals. */
export function computeTotals(rows: TransactionDTO[]): Totals {
  const inByMode = zero();
  const outByMode = zero();
  let count = 0;
  for (const r of rows) {
    if (r.status === "DELETED") continue;
    count++;
    const byMode = r.type === "CASH_IN" ? inByMode : outByMode;
    const split = splitOf(r.price, r.paymentMode, r.onlineAmount);
    if (split) {
      // Part online, part cash: each part counts where it was paid.
      byMode.ONLINE += split.online;
      byMode.CASH += split.cash;
    } else byMode[r.paymentMode ?? "CASH"] += r.price;
  }
  const sum = (m: Record<PaymentMode, number>) => m.CASH + m.ONLINE + m.SYSTEM;
  for (const m of [inByMode, outByMode]) for (const k of Object.keys(m) as PaymentMode[]) m[k] = round(m[k]);
  return {
    cashIn: round(sum(inByMode)),
    cashOut: round(sum(outByMode)),
    net: round(inByMode.CASH - outByMode.CASH),
    count,
    inByMode,
    outByMode,
    hasNonCash: [inByMode, outByMode].some((m) => m.ONLINE !== 0 || m.SYSTEM !== 0),
  };
}
