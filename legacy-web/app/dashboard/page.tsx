import { Suspense } from "react";
import { Lock } from "lucide-react";
import { requireSession } from "@/lib/auth";
import { todayString } from "@/lib/date";
import { computeTotals, getTransactionsForDate, isDayFinalized } from "@/lib/queries";
import { EntryBar } from "@/components/EntryBar";
import { RegisterTable } from "@/components/RegisterTable";
import { TotalsBar } from "@/components/TotalsBar";
import { PageSkeleton } from "@/components/Skeleton";

async function Register() {
  await requireSession();
  const today = todayString();
  const [rows, finalized] = await Promise.all([getTransactionsForDate(today), isDayFinalized(today)]);

  return (
    <div className="space-y-4">
      {finalized ? (
        <div className="flex items-center gap-3 rounded-xl border border-amber-300 bg-amber-50 px-4 py-3 text-amber-900">
          <Lock className="h-5 w-5" />
          <p className="font-medium">Today&apos;s register has been finalized. Entries are locked.</p>
        </div>
      ) : (
        <EntryBar />
      )}
      <TotalsBar totals={computeTotals(rows)} />
      <RegisterTable rows={rows} editable={!finalized} title="Today's Day Register" />
    </div>
  );
}

export default function DashboardPage() {
  return (
    <Suspense fallback={<PageSkeleton />}>
      <Register />
    </Suspense>
  );
}
