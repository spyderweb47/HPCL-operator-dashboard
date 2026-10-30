import { useEffect, useState, type FormEvent } from "react";
import { CheckCircle2, Database, Eye, EyeOff, Loader2, PlugZap, Save, TriangleAlert, Unplug } from "lucide-react";
import { call, mutate } from "@/lib/api";
import { useQuery } from "@/lib/useQuery";
import type { CloudSettingsDTO } from "@/lib/types";
import { Card } from "./DayEndViews";

const input =
  "w-full rounded-lg border border-slate-300 px-3 py-2 outline-none focus:border-hp-blue focus:ring-2 focus:ring-hp-blue/20";
const label = "mb-1 block text-xs font-semibold uppercase tracking-wide text-slate-500";

type Result = { ok: boolean; text: string } | null;

/** Settings → Cloud Database: where this PC uploads its data. Takes effect immediately. */
export function CloudSettings() {
  const { data: saved } = useQuery<CloudSettingsDTO>("get_cloud_settings");
  const [form, setForm] = useState({ connectionString: "", username: "", password: "", database: "" });
  const [showPassword, setShowPassword] = useState(false);
  const [busy, setBusy] = useState<"test" | "save" | "clear" | null>(null);
  const [result, setResult] = useState<Result>(null);
  const [loadedFor, setLoadedFor] = useState<CloudSettingsDTO | null>(null);

  // Fill the form from what's saved (once per saved value). The password is never sent back to the screen.
  useEffect(() => {
    if (!saved || saved === loadedFor) return;
    setLoadedFor(saved);
    setForm({ connectionString: saved.connectionString, username: saved.username, password: "", database: saved.database });
  }, [saved, loadedFor]);

  const set = (k: keyof typeof form) => (e: { target: { value: string } }) => {
    setForm((f) => ({ ...f, [k]: e.target.value }));
    setResult(null);
  };

  const test = async () => {
    setBusy("test");
    setResult(null);
    try {
      setResult({ ok: true, text: await call<string>("test_cloud_settings", form) });
    } catch (err) {
      setResult({ ok: false, text: typeof err === "string" ? err : "Test failed." });
    } finally {
      setBusy(null);
    }
  };

  const save = async (e: FormEvent) => {
    e.preventDefault();
    setBusy("save");
    const res = await mutate<CloudSettingsDTO>("save_cloud_settings", form);
    setBusy(null);
    setResult(
      res.ok
        ? { ok: true, text: `Saved. Syncing to ${res.data.host} · ${res.data.database}. Watch the status at the top right.` }
        : { ok: false, text: res.error },
    );
  };

  const clear = async () => {
    if (!window.confirm("Stop syncing this PC to the cloud?\nEverything stays saved on this PC.")) return;
    setBusy("clear");
    const res = await mutate<CloudSettingsDTO>("clear_cloud_settings");
    setBusy(null);
    setResult(res.ok ? { ok: true, text: "Cloud sync turned off. Data stays on this PC." } : { ok: false, text: res.error });
  };

  const keepsPassword = saved?.hasPassword && !form.password;

  return (
    <Card
      title="Cloud Database"
      icon={Database}
      aside={
        saved && (
          <span className={`text-sm ${saved.configured ? "text-emerald-700" : "text-slate-500"}`}>
            {saved.configured
              ? `${saved.host} · ${saved.database}${saved.source === "environment" ? " (from environment variable)" : ""}`
              : "Not set up"}
          </span>
        )
      }
    >
      <form onSubmit={save} className="space-y-3 p-4">
        <div>
          <label htmlFor="cloud-cs" className={label}>
            MongoDB connection string
          </label>
          <input
            id="cloud-cs"
            spellCheck={false}
            autoComplete="off"
            value={form.connectionString}
            onChange={set("connectionString")}
            placeholder="mongodb+srv://user:<db_password>@cluster0.xxxxx.mongodb.net/"
            className={`${input} font-mono text-sm`}
          />
          <p className="mt-1 text-xs text-slate-400">
            Paste it from MongoDB Atlas → Connect → Drivers. A username and password typed below replace the ones in the string.
          </p>
        </div>
        <div className="grid gap-3 md:grid-cols-3">
          <div>
            <label htmlFor="cloud-user" className={label}>
              Username
            </label>
            <input id="cloud-user" autoComplete="off" spellCheck={false} value={form.username} onChange={set("username")} className={input} />
          </div>
          <div>
            <label htmlFor="cloud-pass" className={label}>
              Password
            </label>
            <div className="relative">
              <input
                id="cloud-pass"
                type={showPassword ? "text" : "password"}
                autoComplete="new-password"
                value={form.password}
                onChange={set("password")}
                placeholder={saved?.hasPassword ? "Saved · leave blank to keep" : ""}
                className={`${input} pr-10`}
              />
              <button
                type="button"
                tabIndex={-1}
                onClick={() => setShowPassword((v) => !v)}
                title={showPassword ? "Hide password" : "Show password"}
                className="absolute inset-y-0 right-0 px-3 text-slate-400 hover:text-slate-600"
              >
                {showPassword ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
              </button>
            </div>
          </div>
          <div>
            <label htmlFor="cloud-db" className={label}>
              Database name
            </label>
            <input
              id="cloud-db"
              autoComplete="off"
              spellCheck={false}
              value={form.database}
              onChange={set("database")}
              placeholder="hpcl_dealership"
              className={input}
            />
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2 pt-1">
          <button
            type="button"
            onClick={test}
            disabled={!!busy || !form.connectionString.trim()}
            className="flex items-center gap-1.5 rounded-lg border border-hp-blue bg-white px-4 py-2 font-semibold text-hp-blue hover:bg-blue-50 disabled:opacity-50"
          >
            {busy === "test" ? <Loader2 className="h-4 w-4 animate-spin" /> : <PlugZap className="h-4 w-4" />}
            Test connection
          </button>
          <button
            type="submit"
            disabled={!!busy || !form.connectionString.trim()}
            className="flex items-center gap-1.5 rounded-lg bg-hp-blue px-4 py-2 font-semibold text-white hover:bg-hp-blue-dark disabled:opacity-50"
          >
            {busy === "save" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
            Save &amp; connect
          </button>
          {saved?.configured && saved.source === "settings" && (
            <button
              type="button"
              onClick={clear}
              disabled={!!busy}
              className="ml-auto flex items-center gap-1.5 rounded-lg px-3 py-2 text-sm font-medium text-red-600 hover:bg-red-50 disabled:opacity-50"
            >
              <Unplug className="h-4 w-4" /> Turn off cloud sync
            </button>
          )}
        </div>

        {result ? (
          <p
            role={result.ok ? "status" : "alert"}
            className={`flex items-start gap-2 rounded-lg px-3 py-2 text-sm ${
              result.ok ? "bg-emerald-50 text-emerald-800" : "bg-red-50 text-red-700"
            }`}
          >
            {result.ok ? <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" /> : <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />}
            <span className="break-all">{result.text}</span>
          </p>
        ) : (
          <p className="text-xs text-slate-400">
            {keepsPassword ? "The saved password is kept unless you type a new one. " : ""}
            Changing the cluster or database uploads everything on this PC (last 180 days) to the new one. The connection string
            is stored only on this PC.
          </p>
        )}
      </form>
    </Card>
  );
}
