import { NavLink } from "react-router";
import { ClipboardList, History, Lock, Settings } from "lucide-react";

const LINKS = [
  { to: "/", label: "Day Register", icon: ClipboardList },
  { to: "/day-end", label: "Day End & Stock", icon: Lock },
  { to: "/history", label: "History", icon: History },
  { to: "/settings", label: "Settings", icon: Settings },
] as const;

export function NavLinks() {
  return (
    <nav className="flex gap-1">
      {LINKS.map(({ to, label, icon: Icon }) => (
        <NavLink
          key={to}
          to={to}
          end
          className={({ isActive }) =>
            `flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium ${
              isActive ? "bg-white text-hp-blue" : "text-blue-100 hover:bg-white/10 hover:text-white"
            }`
          }
        >
          <Icon className="h-4 w-4" />
          {label}
        </NavLink>
      ))}
    </nav>
  );
}
