import { Suspense } from "react";
import { requireSession } from "@/lib/auth";
import { todayString } from "@/lib/date";
import { blankDayEnd, computeTotals, getActiveDeliveryBoys, getDayEnd, getTransactionsForDate, mergeRoster } from "@/lib/queries";
import { DayEndForm } from "@/components/DayEndForm";
import { PageSkeleton } from "@/components/Skeleton";

async function DayEnd() {
  await requireSession();
  const today = todayString();
  const [log, roster, rows] = await Promise.all([getDayEnd(today), getActiveDeliveryBoys(), getTransactionsForDate(today)]);
  const base = log ?? blankDayEnd(today);
  // A finalized day shows exactly what was locked; an open day shows the live roster.
  const initial = base.isFinalized ? base : { ...base, deliveries: mergeRoster(roster, base.deliveries) };
  // key: remount with fresh server state after finalize / day rollover
  return (
    <DayEndForm
      key={`${initial.dateString}-${initial.isFinalized}`}
      initial={initial}
      totals={computeTotals(rows)}
    />
  );
}

export default function DayEndPage() {
  return (
    <Suspense fallback={<PageSkeleton />}>
      <DayEnd />
    </Suspense>
  );
}
