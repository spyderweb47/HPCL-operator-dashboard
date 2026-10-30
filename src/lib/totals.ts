import type { Totals, TransactionDTO } from "./types";

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
