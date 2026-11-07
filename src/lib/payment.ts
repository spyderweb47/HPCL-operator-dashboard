import { formatINR } from "./format";
import type { PaymentMode } from "./types";

export const PAYMENT_MODES: PaymentMode[] = ["CASH", "ONLINE", "SYSTEM"];

export const PAYMENT_LABEL: Record<PaymentMode, string> = { CASH: "Cash", ONLINE: "Online", SYSTEM: "System" };

/** Part online, part cash: how much of the price was paid each way. Null when it's all one type. */
export function splitOf(price: number, mode: PaymentMode | undefined, onlineAmount: number | null | undefined) {
  if (mode !== "ONLINE" || onlineAmount == null || onlineAmount <= 0 || onlineAmount >= price) return null;
  return { online: onlineAmount, cash: Math.round((price - onlineAmount) * 100) / 100 };
}

/** "₹400.00 online + ₹500.00 cash" */
export const splitText = (s: { online: number; cash: number }) => `${formatINR(s.online)} online + ${formatINR(s.cash)} cash`;

/** Tag colours for the non-cash types (cash is the default and shows no tag). */
export const PAYMENT_TAG: Record<Exclude<PaymentMode, "CASH">, string> = {
  ONLINE: "bg-violet-100 text-violet-800",
  SYSTEM: "bg-orange-100 text-orange-800",
};
