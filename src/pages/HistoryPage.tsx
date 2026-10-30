import { useSearchParams } from "react-router";
import { CalendarX2, CheckCircle2, CircleDashed, Cloud, CloudOff } from "lucide-react";
import { formatDateTime, formatLongDate, isValidDateString, shiftDateString, todayString } from "@/lib/date";
import { useQuery } from "@/lib/useQuery";
import { computeTotals } from "@/lib/totals";
import type { HistoryDayDTO } from "@/lib/types";
import { DeliveriesView, StockView } from "@/components/DayEndViews";
import { HistoryDatePicker } from "@/components/HistoryDatePicker";
import { LoadError } from "@/components/LoadError";
import { RegisterTable } from "@/components/RegisterTable";
import { PageSkeleton } from "@/components/Skeleton";
import { TotalsBar } from "@/components/TotalsBar";

export function HistoryPage() {
  const [params] = useSearchParams();
  const today = todayString();
  const requested = params.get("date");
  const date = isValidDateString(requested) && requested <= today ? requested : shiftDateString(today, -1);
  const { data, error, loading } = useQuery<HistoryDayDTO>("get_history_day", { date });

  const log = data?.dayEnd ?? null;
  const rows = data?.transactions ?? [];
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
                {log.finalizedAt ? ` · ${formatDateTime(log.finalizedAt)}` : ""}
              </span>
            ) : (
              <span className="flex items-center gap-1.5 text-amber-700">
                <CircleDashed className="h-4 w-4" /> Not finalized
              </span>
            )}
            <span className="ml-2 rounded bg-slate-100 px-1.5 py-0.5 text-xs font-medium text-slate-500">Read-only</span>
            {data?.source === "cloud" && (
              <span className="flex items-center gap-1 rounded bg-sky-100 px-1.5 py-0.5 text-xs font-medium text-sky-800">
                <Cloud className="h-3 w-3" /> Loaded from cloud
              </span>
            )}
          </p>
        </div>
        <HistoryDatePicker date={date} today={today} />
      </div>

      {data?.message && data.source !== "unavailable" && (
        <p className="rounded-lg border border-amber-200 bg-amber-50 px-4 py-2 text-sm text-amber-900">{data.message}</p>
      )}

      {loading ? (
        <PageSkeleton />
      ) : error ? (
        <LoadError message={error} />
      ) : data?.source === "unavailable" ? (
        <div className="flex flex-col items-center gap-2 rounded-xl border border-dashed border-amber-300 bg-white py-16 text-center text-slate-600">
          <CloudOff className="h-8 w-8 text-amber-600" />
          <p className="max-w-md">{data.message}</p>
        </div>
      ) : empty ? (
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
