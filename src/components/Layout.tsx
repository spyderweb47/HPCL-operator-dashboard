import { useEffect, useState } from "react";
import { Outlet } from "react-router";
import { Fuel } from "lucide-react";
import { formatLongDate, todayString } from "@/lib/date";
import { NavLinks } from "./NavLinks";
import { SyncBadge } from "./SyncBadge";

/** Today's business date, re-checked every 30 s so the header rolls over at midnight. */
export function useToday() {
  const [today, setToday] = useState(todayString);
  useEffect(() => {
    const t = setInterval(() => setToday(todayString()), 30_000);
    return () => clearInterval(t);
  }, []);
  return today;
}

export function Layout() {
  const today = useToday();
  return (
    <div className="flex min-h-screen flex-col">
      <header className="border-b-4 border-hp-red bg-hp-blue text-white">
        <div className="mx-auto flex max-w-7xl items-center justify-between gap-4 px-4 py-2.5">
          <div className="flex items-center gap-6">
            <div className="flex items-center gap-2 font-bold">
              <Fuel className="h-5 w-5" />
              <span>HPCL Operator</span>
            </div>
            <NavLinks />
          </div>
          <div className="flex items-center gap-4">
            <span className="hidden text-sm text-blue-100 lg:inline">{formatLongDate(today)}</span>
            <SyncBadge />
          </div>
        </div>
      </header>
      <main className="mx-auto w-full max-w-7xl flex-1 px-4 py-5">
        <Outlet />
      </main>
    </div>
  );
}
