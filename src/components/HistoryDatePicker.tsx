import { useNavigate } from "react-router";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { shiftDateString } from "@/lib/date";

export function HistoryDatePicker({ date, today }: { date: string; today: string }) {
  const navigate = useNavigate();
  const go = (d: string) => {
    if (d > today) return;
    navigate(`/history?date=${d}`);
  };

  const yesterday = shiftDateString(today, -1);
  const dayBefore = shiftDateString(today, -2);
  const quick = [
    { label: "Yesterday", value: yesterday },
    { label: "Day before", value: dayBefore },
  ];

  return (
    <div className="flex flex-wrap items-center gap-2">
      {quick.map((q) => (
        <button
          key={q.label}
          type="button"
          onClick={() => go(q.value)}
          className={`rounded-md border px-3 py-1.5 text-sm font-medium ${
            date === q.value ? "border-hp-blue bg-hp-blue text-white" : "border-slate-300 bg-white text-slate-700 hover:bg-slate-50"
          }`}
        >
          {q.label}
        </button>
      ))}
      <div className="flex items-center rounded-md border border-slate-300 bg-white">
        <button type="button" aria-label="Previous day" onClick={() => go(shiftDateString(date, -1))} className="px-2 py-1.5 hover:bg-slate-50">
          <ChevronLeft className="h-4 w-4" />
        </button>
        <input
          type="date"
          aria-label="Select date"
          value={date}
          max={today}
          onChange={(e) => e.target.value && go(e.target.value)}
          className="border-x border-slate-300 px-2 py-1 text-sm outline-none"
        />
        <button
          type="button"
          aria-label="Next day"
          disabled={date >= today}
          onClick={() => go(shiftDateString(date, 1))}
          className="px-2 py-1.5 hover:bg-slate-50 disabled:opacity-30"
        >
          <ChevronRight className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}
