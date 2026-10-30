import { useState, type FormEvent, type KeyboardEvent } from "react";
import { Bike, Check, Copy, Loader2, Monitor, Pencil, Plus, RotateCcw, UserX, X } from "lucide-react";
import { mutate } from "@/lib/api";
import { formatDateTime } from "@/lib/date";
import { useQuery } from "@/lib/useQuery";
import type { AppInfoDTO, DeliveryBoyDTO, SyncStatusDTO } from "@/lib/types";
import { CloudSettings } from "@/components/CloudSettings";
import { Card } from "@/components/DayEndViews";
import { LoadError } from "@/components/LoadError";

export function SettingsPage() {
  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-xl font-bold text-slate-800">Settings</h1>
        <p className="text-sm text-slate-500">Saved on this PC and synced to the cloud.</p>
      </div>
      <DeliveryBoysSettings />
      <CloudSettings />
      <DeviceInfo />
    </div>
  );
}

function DeliveryBoysSettings() {
  const { data: boys, error: loadError } = useQuery<DeliveryBoyDTO[]>("list_delivery_boys");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null); // "add" or a delivery boy id
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  const run = async (key: string, cmd: string, args: Record<string, unknown>, done: string) => {
    setBusy(key);
    setError(null);
    const res = await mutate(cmd, args);
    setBusy(null);
    if (!res.ok) {
      setError(res.error);
      setFlash(null);
      return false;
    }
    setFlash(done);
    return true;
  };

  const add = async (e: FormEvent) => {
    e.preventDefault();
    const n = name.trim();
    if (!n) return setError("Enter the delivery boy's name.");
    if (await run("add", "add_delivery_boy", { name: n }, `Added ${n}.`)) setName("");
  };

  const saveRename = async (b: DeliveryBoyDTO) => {
    if (draft.trim() === b.name) return setEditingId(null);
    if (await run(b.id, "rename_delivery_boy", { id: b.id, name: draft }, `Renamed to ${draft.trim()}.`)) setEditingId(null);
  };

  const onRenameKey = (e: KeyboardEvent<HTMLInputElement>, b: DeliveryBoyDTO) => {
    if (e.key === "Enter") {
      e.preventDefault();
      saveRename(b);
    } else if (e.key === "Escape") {
      e.preventDefault();
      setEditingId(null);
    }
  };

  const active = boys?.filter((b) => b.isActive).length ?? 0;

  return (
    <Card
      title="Delivery Boys"
      icon={Bike}
      aside={boys && <span className="text-sm text-slate-500">{active} active</span>}
    >
      <form onSubmit={add} className="flex flex-wrap items-center gap-2 border-b border-slate-100 px-4 py-3">
        <input
          autoFocus
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            setError(null);
          }}
          placeholder="Delivery boy's name"
          aria-label="Delivery boy's name"
          maxLength={120}
          className="w-72 rounded-lg border border-slate-300 px-3 py-2 outline-none focus:border-hp-blue focus:ring-2 focus:ring-hp-blue/20"
        />
        <button
          type="submit"
          disabled={busy === "add"}
          className="flex items-center gap-1.5 rounded-lg bg-hp-blue px-4 py-2 font-semibold text-white hover:bg-hp-blue-dark disabled:opacity-60"
        >
          {busy === "add" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Plus className="h-4 w-4" />}
          Add
        </button>
        <span className="text-sm">
          {error ? (
            <span role="alert" className="font-medium text-red-600">
              {error}
            </span>
          ) : flash ? (
            <span className="font-medium text-emerald-700">{flash}</span>
          ) : (
            <span className="text-slate-400">They appear on the Day End page in this order (A–Z).</span>
          )}
        </span>
      </form>

      {loadError ? (
        <div className="p-4">
          <LoadError message={loadError} />
        </div>
      ) : (
        <table className="w-full text-sm">
          <thead className="bg-slate-50 text-left text-xs uppercase tracking-wide text-slate-500">
            <tr>
              <th className="px-4 py-2">Name</th>
              <th className="w-32 px-2 py-2">Status</th>
              <th className="w-40 px-2 py-2">Added</th>
              <th className="w-48 px-4 py-2 text-right">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {boys?.length === 0 && (
              <tr>
                <td colSpan={4} className="px-4 py-8 text-center text-slate-400">
                  No delivery boys yet. Add the first one above.
                </td>
              </tr>
            )}
            {boys?.map((b) => {
              const editing = editingId === b.id;
              return (
                <tr key={b.id} className={b.isActive ? "" : "bg-slate-50 text-slate-400"}>
                  <td className="px-4 py-2">
                    {editing ? (
                      <input
                        autoFocus
                        value={draft}
                        maxLength={120}
                        aria-label={`New name for ${b.name}`}
                        onChange={(e) => setDraft(e.target.value)}
                        onKeyDown={(e) => onRenameKey(e, b)}
                        onFocus={(e) => e.target.select()}
                        className="w-full max-w-sm rounded border border-slate-300 px-2 py-1 outline-none focus:border-hp-blue"
                      />
                    ) : (
                      <span className={`font-medium ${b.isActive ? "text-slate-800" : ""}`}>{b.name}</span>
                    )}
                  </td>
                  <td className="px-2 py-2">
                    <span
                      className={`rounded px-2 py-0.5 text-xs font-bold ${
                        b.isActive ? "bg-emerald-100 text-emerald-800" : "bg-slate-200 text-slate-500"
                      }`}
                    >
                      {b.isActive ? "ACTIVE" : "INACTIVE"}
                    </span>
                  </td>
                  <td className="px-2 py-2 text-slate-500">{formatDateTime(b.createdAt)}</td>
                  <td className="px-4 py-2">
                    <div className="flex justify-end gap-1">
                      {busy === b.id ? (
                        <Loader2 className="h-4 w-4 animate-spin text-slate-400" />
                      ) : editing ? (
                        <>
                          <SmallButton label="Save (Enter)" onClick={() => saveRename(b)} className="text-emerald-700 hover:bg-emerald-50">
                            <Check className="h-4 w-4" />
                          </SmallButton>
                          <SmallButton label="Cancel (Esc)" onClick={() => setEditingId(null)} className="text-slate-600 hover:bg-slate-100">
                            <X className="h-4 w-4" />
                          </SmallButton>
                        </>
                      ) : (
                        <>
                          <SmallButton
                            label="Rename"
                            onClick={() => {
                              setEditingId(b.id);
                              setDraft(b.name);
                              setError(null);
                            }}
                            className="text-hp-blue hover:bg-blue-50"
                          >
                            <Pencil className="h-4 w-4" />
                          </SmallButton>
                          {b.isActive ? (
                            <SmallButton
                              label="Deactivate"
                              text="Deactivate"
                              onClick={() => run(b.id, "set_delivery_boy_active", { id: b.id, isActive: false }, `${b.name} deactivated.`)}
                              className="text-red-600 hover:bg-red-50"
                            >
                              <UserX className="h-4 w-4" />
                            </SmallButton>
                          ) : (
                            <SmallButton
                              label="Reactivate"
                              text="Reactivate"
                              onClick={() => run(b.id, "set_delivery_boy_active", { id: b.id, isActive: true }, `${b.name} reactivated.`)}
                              className="text-emerald-700 hover:bg-emerald-50"
                            >
                              <RotateCcw className="h-4 w-4" />
                            </SmallButton>
                          )}
                        </>
                      )}
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
      <p className="border-t border-slate-100 px-4 py-2 text-xs text-slate-400">
        Deactivate instead of deleting: past day-end records keep their names, and they can be reactivated any time.
      </p>
    </Card>
  );
}

function SmallButton(props: { label: string; text?: string; onClick: () => void; className: string; children: React.ReactNode }) {
  return (
    <button
      type="button"
      title={props.label}
      aria-label={props.label}
      onClick={props.onClick}
      className={`flex items-center gap-1 rounded px-1.5 py-1 text-xs font-medium ${props.className}`}
    >
      {props.children}
      {props.text}
    </button>
  );
}

const SYNC_TEXT: Record<SyncStatusDTO["state"], string> = {
  online: "Connected · up to date",
  syncing: "Uploading…",
  connecting: "Connecting…",
  offline: "Offline · will upload when the internet is back",
  error: "Problem syncing",
  not_configured: "Not set up · data stays on this PC",
};

function DeviceInfo() {
  const { data: info } = useQuery<AppInfoDTO>("app_info");
  const { data: sync } = useQuery<SyncStatusDTO>("sync_status", {}, 3000);
  const [copied, setCopied] = useState(false);
  if (!info) return null;

  const copy = async () => {
    await navigator.clipboard.writeText(info.machineId).catch(() => {});
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const rows: [string, React.ReactNode][] = [
    [
      "Machine ID",
      <span className="flex items-center gap-2">
        <code className="rounded bg-slate-100 px-2 py-0.5 font-mono text-sm font-semibold tracking-wider text-slate-800">{info.machineId}</code>
        <button type="button" onClick={copy} title="Copy" className="rounded p-1 text-slate-500 hover:bg-slate-100">
          {copied ? <Check className="h-4 w-4 text-emerald-600" /> : <Copy className="h-4 w-4" />}
        </button>
      </span>,
    ],
    ["Computer name", info.deviceName],
    [
      "Cloud sync",
      sync && (
        <span>
          {SYNC_TEXT[sync.state]}
          {sync.pending > 0 && ` · ${sync.pending} change${sync.pending === 1 ? "" : "s"} waiting`}
          {sync.lastSyncAt && <span className="text-slate-400"> · last synced {formatDateTime(sync.lastSyncAt)}</span>}
          {(sync.state === "error" || sync.state === "offline") && sync.lastError && (
            <span className="mt-1 block break-all text-xs text-red-600">{sync.lastError}</span>
          )}
        </span>
      ),
    ],
    ["History kept on this PC", `Last ${info.retentionDays} days (older days load from the cloud when online)`],
    ["Data folder", <code className="font-mono text-xs text-slate-500">{info.dataDir}</code>],
    ["App version", info.appVersion],
  ];

  return (
    <Card title="This Computer" icon={Monitor}>
      <dl className="divide-y divide-slate-100 text-sm">
        {rows.map(([k, v]) => (
          <div key={k} className="flex items-center gap-4 px-4 py-2.5">
            <dt className="w-52 shrink-0 text-slate-500">{k}</dt>
            <dd className="text-slate-800">{v}</dd>
          </div>
        ))}
      </dl>
    </Card>
  );
}
