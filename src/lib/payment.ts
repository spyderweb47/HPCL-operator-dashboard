import type { PaymentMode } from "./types";

export const PAYMENT_MODES: PaymentMode[] = ["CASH", "ONLINE", "SYSTEM"];

export const PAYMENT_LABEL: Record<PaymentMode, string> = { CASH: "Cash", ONLINE: "Online", SYSTEM: "System" };

/** Tag colours for the non-cash types (cash is the default and shows no tag). */
export const PAYMENT_TAG: Record<Exclude<PaymentMode, "CASH">, string> = {
  ONLINE: "bg-violet-100 text-violet-800",
  SYSTEM: "bg-orange-100 text-orange-800",
};
