import { Suspense } from "react";
import { Fuel, LogOut } from "lucide-react";
import { logout } from "@/app/actions/auth";
import { requireSession } from "@/lib/auth";
import { formatLongDate, todayString } from "@/lib/date";
import { NavLinks } from "@/components/NavLinks";

async function UserBadge() {
  const session = await requireSession();
  return (
    <div className="flex items-center gap-4">
      <span className="hidden text-sm text-blue-100 md:inline">{formatLongDate(todayString())}</span>
      <span className="rounded-md bg-white/10 px-2.5 py-1 text-sm font-medium">{session.name}</span>
      <form action={logout}>
        <button
          type="submit"
          title="Log out"
          className="flex items-center gap-1.5 rounded-md px-2 py-1 text-sm text-blue-100 hover:bg-white/10 hover:text-white"
        >
          <LogOut className="h-4 w-4" /> Logout
        </button>
      </form>
    </div>
  );
}

export default function DashboardLayout({ children }: LayoutProps<"/dashboard">) {
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
          <Suspense fallback={<div className="h-7 w-40 animate-pulse rounded bg-white/10" />}>
            <UserBadge />
          </Suspense>
        </div>
      </header>
      <main className="mx-auto w-full max-w-7xl flex-1 px-4 py-5">{children}</main>
    </div>
  );
}
