import { Lock } from "lucide-react";
import { useQuery } from "@/lib/useQuery";
import { computeTotals } from "@/lib/totals";
import type { RegisterDTO } from "@/lib/types";
import { EntryBar } from "@/components/EntryBar";
import { LoadError } from "@/components/LoadError";
import { RegisterTable } from "@/components/RegisterTable";
import { PageSkeleton } from "@/components/Skeleton";
import { TotalsBar } from "@/components/TotalsBar";

export function RegisterPage() {
  // Reloads after every write; the interval also picks up sync ticks and the midnight rollover.
  const { data, error } = useQuery<RegisterDTO>("get_register", {}, 5000);
  if (!data) return error ? <LoadError message={error} /> : <PageSkeleton />;

  return (
    <div className="space-y-4">
      {data.isFinalized ? (
        <div className="flex items-center gap-3 rounded-xl border border-amber-300 bg-amber-50 px-4 py-3 text-amber-900">
          <Lock className="h-5 w-5" />
          <p className="font-medium">Today&apos;s register has been finalized. Entries are locked.</p>
        </div>
      ) : (
        <EntryBar key={data.date} />
      )}
      <TotalsBar totals={computeTotals(data.transactions)} />
      <RegisterTable rows={data.transactions} editable={!data.isFinalized} title="Today's Day Register" />
    </div>
  );
}
