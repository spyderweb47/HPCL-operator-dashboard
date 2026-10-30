import { Cloud, CloudOff, HardDrive, Loader2, TriangleAlert } from "lucide-react";
import { formatDateTime } from "@/lib/date";
import { useQuery } from "@/lib/useQuery";
import type { SyncStatusDTO } from "@/lib/types";

/** Header pill: is everything entered on this PC also in the cloud yet? */
export function SyncBadge() {
  const { data: s } = useQuery<SyncStatusDTO>("sync_status", {}, 1500);
  if (!s) return <div className="h-7 w-28 animate-pulse rounded-md bg-white/10" />;

  const pending = s.pending > 0 ? `${s.pending} waiting` : null;
  const last = s.lastSyncAt ? `Last synced ${formatDateTime(s.lastSyncAt)}` : "Not synced yet";
  const view = {
    online: { icon: Cloud, text: pending ?? "Synced", tone: "bg-emerald-500/20 text-emerald-100", title: last },
    syncing: { icon: Loader2, text: `Syncing${pending ? ` · ${pending}` : ""}`, tone: "bg-white/10 text-blue-50", title: last },
    connecting: { icon: Loader2, text: "Connecting…", tone: "bg-white/10 text-blue-50", title: "Connecting to the cloud" },
    offline: {
      icon: CloudOff,
      text: `Offline${pending ? ` · ${pending}` : ""}`,
      tone: "bg-amber-400/25 text-amber-100",
      title: `Saved safely on this PC. It uploads automatically when the connection is back.\n${last}${s.lastError ? `\n\n${s.lastError}` : ""}`,
    },
    error: { icon: TriangleAlert, text: `Sync problem${pending ? ` · ${pending}` : ""}`, tone: "bg-red-500/30 text-red-50", title: `${s.lastError ?? ""}\n${last}` },
    not_configured: { icon: HardDrive, text: "This PC only", tone: "bg-white/10 text-blue-100", title: "Cloud sync is not set up (Settings → Cloud Database). Data is saved on this PC." },
  }[s.state];
  const Icon = view.icon;
  return (
    <span title={view.title} className={`flex items-center gap-1.5 rounded-md px-2.5 py-1 text-sm font-medium ${view.tone}`}>
      <Icon className={`h-4 w-4 ${Icon === Loader2 ? "animate-spin" : ""}`} />
      {view.text}
    </span>
  );
}
