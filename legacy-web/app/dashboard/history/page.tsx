import { Suspense } from "react";
import { CalendarX2, CheckCircle2, CircleDashed } from "lucide-react";
import { requireSession } from "@/lib/auth";
import { formatDateTime, formatLongDate, isValidDateString, shiftDateString, todayString } from "@/lib/date";
import { computeTotals, getDayEnd, getTransactionsForDate } from "@/lib/queries";
import { DeliveriesView, StockView } from "@/components/DayEndViews";
import { HistoryDatePicker } from "@/components/HistoryDatePicker";
import { RegisterTable } from "@/components/RegisterTable";
import { TotalsBar } from "@/components/TotalsBar";
import { PageSkeleton } from "@/components/Skeleton";

async function HistoryContent({ searchParams }: { searchParams: PageProps<"/dashboard/history">["searchParams"] }) {
  await requireSession();
  const today = todayString();
  const raw = (await searchParams).date;
  const requested = Array.isArray(raw) ? raw[0] : raw;
  const date = isValidDateString(requested) && requested <= today ? requested : shiftDateString(today, -1);

  const [rows, log] = await Promise.all([getTransactionsForDate(date), getDayEnd(date)]);
  const empty = rows.length === 0 && !log;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-slate-200 bg-white px-4 py-3 shadow-sm">
        <div>
          <h1 className="text-lg font-bold text-slate-800">{formatLongDate(date)}</h1>
          <p className="mt-0.5 flex items-center gap-1.5 text-sm">
            {log?.isFinalized ? (
              <span className="flex items-center gap-1.5 text-emerald-700">
                <CheckCircle2 className="h-4 w-4" /> Finalized
                {log.finalizedByName ? ` by ${log.finalizedByName}` : ""}
                {log.finalizedAt ? ` · ${formatDateTime(log.finalizedAt)}` : ""}
              </span>
            ) : (
              <span className="flex items-center gap-1.5 text-amber-700">
                <CircleDashed className="h-4 w-4" /> Not finalized
              </span>
            )}
            <span className="ml-2 rounded bg-slate-100 px-1.5 py-0.5 text-xs font-medium text-slate-500">Read-only</span>
          </p>
        </div>
        <HistoryDatePicker date={date} today={today} />
      </div>

      {empty ? (
        <div className="flex flex-col items-center gap-2 rounded-xl border border-dashed border-slate-300 bg-white py-16 text-slate-500">
          <CalendarX2 className="h-8 w-8" />
          <p>No records for this date.</p>
        </div>
      ) : (
        <>
          <TotalsBar totals={computeTotals(rows)} />
          <RegisterTable rows={rows} editable={false} title="Cash In / Cash Out Register" />
          <div className="grid gap-4 lg:grid-cols-2">
            <DeliveriesView deliveries={log?.deliveries ?? []} />
            {log ? (
              <StockView stock={log.cylinderStock} />
            ) : (
              <div className="flex items-center justify-center rounded-xl border border-dashed border-slate-300 bg-white p-8 text-slate-400">
                No closing stock recorded.
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
}

export default function HistoryPage({ searchParams }: PageProps<"/dashboard/history">) {
  return (
    <Suspense fallback={<PageSkeleton />}>
      <HistoryContent searchParams={searchParams} />
    </Suspense>
  );
}
