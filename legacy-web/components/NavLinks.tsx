"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { ClipboardList, History, Lock } from "lucide-react";

const LINKS = [
  { href: "/dashboard", label: "Day Register", icon: ClipboardList },
  { href: "/dashboard/day-end", label: "Day End & Stock", icon: Lock },
  { href: "/dashboard/history", label: "History", icon: History },
] as const;

export function NavLinks() {
  const pathname = usePathname();
  return (
    <nav className="flex gap-1">
      {LINKS.map(({ href, label, icon: Icon }) => {
        const active = pathname === href;
        return (
          <Link
            key={href}
            href={href}
            className={`flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium ${
              active ? "bg-white text-hp-blue" : "text-blue-100 hover:bg-white/10 hover:text-white"
            }`}
          >
            <Icon className="h-4 w-4" />
            {label}
          </Link>
        );
      })}
    </nav>
  );
}
