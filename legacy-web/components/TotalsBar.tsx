import { ArrowDownLeft, ArrowUpRight, Hash, Wallet } from "lucide-react";
import { formatINR } from "@/lib/format";
import type { Totals } from "@/lib/types";

export function TotalsBar({ totals }: { totals: Totals }) {
  const cards = [
    { label: "Total Cash In", value: formatINR(totals.cashIn), icon: ArrowDownLeft, tone: "text-emerald-700 bg-emerald-50 border-emerald-200" },
    { label: "Total Cash Out", value: formatINR(totals.cashOut), icon: ArrowUpRight, tone: "text-rose-700 bg-rose-50 border-rose-200" },
    {
      label: "Net Cash",
      value: formatINR(totals.net),
      icon: Wallet,
      tone: totals.net < 0 ? "text-rose-800 bg-white border-slate-200" : "text-hp-blue bg-white border-slate-200",
    },
    { label: "Active Entries", value: String(totals.count), icon: Hash, tone: "text-slate-700 bg-white border-slate-200" },
  ];
  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
      {cards.map(({ label, value, icon: Icon, tone }) => (
        <div key={label} className={`rounded-xl border px-4 py-3 ${tone}`}>
          <div className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide opacity-80">
            <Icon className="h-3.5 w-3.5" /> {label}
          </div>
          <div className="tabular mt-1 text-2xl font-bold">{value}</div>
        </div>
      ))}
    </div>
  );
}
