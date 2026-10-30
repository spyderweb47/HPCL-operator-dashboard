import { useQuery } from "@/lib/useQuery";
import { computeTotals } from "@/lib/totals";
import type { DayEndPageDTO } from "@/lib/types";
import { DayEndForm } from "@/components/DayEndForm";
import { LoadError } from "@/components/LoadError";
import { PageSkeleton } from "@/components/Skeleton";

export function DayEndPage() {
  const { data, error } = useQuery<DayEndPageDTO>("get_day_end", {}, 60_000);
  if (!data) return error ? <LoadError message={error} /> : <PageSkeleton />;
  // key: start fresh after finalize or at the day rollover; otherwise the form keeps what's typed.
  return (
    <DayEndForm
      key={`${data.date}-${data.dayEnd.isFinalized}`}
      initial={data.dayEnd}
      totals={computeTotals(data.transactions)}
    />
  );
}
