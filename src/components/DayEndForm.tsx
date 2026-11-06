import { useCallback, useEffect, useRef, useState, useTransition } from "react";
import { AlertTriangle, Check, CheckCircle2, Cylinder, Loader2, Lock } from "lucide-react";
import { mutate } from "@/lib/api";
import { formatDateTime, formatLongDate, formatTime } from "@/lib/date";
import { formatINR } from "@/lib/format";
import { PAYMENT_LABEL } from "@/lib/payment";
import type { CylinderStockDTO, DayEndDTO, DeliverySalesDTO, Totals } from "@/lib/types";
import { Card, STOCK_ROWS, StockView } from "./DayEndViews";
import { DeliverySalesSection, DeliverySalesView, summarizeSaved, type SalesSummary } from "./DeliverySales";

type Props = { initial: DayEndDTO; totals: Totals; sales: DeliverySalesDTO[] };

const numberInput =
  "tabular w-full rounded-md border border-slate-300 px-3 py-2 text-right outline-none focus:border-hp-blue focus:ring-2 focus:ring-hp-blue/20 disabled:bg-slate-50";

/** Wait this long after the last keystroke, then save. */
const AUTOSAVE_MS = 700;

// Only the stock count: delivery counts saved by older versions are left as they are.
type SaveBody = { cylinderStock: CylinderStockDTO };

const toCount = (v: string) => {
  const n = parseInt(v.replace(/\D/g, ""), 10);
  return Number.isFinite(n) ? n : 0;
};

export function DayEndForm({ initial, totals, sales }: Props) {
  const [stock, setStock] = useState<CylinderStockDTO>(initial.cylinderStock);
  const [saveState, setSaveState] = useState<"idle" | "waiting" | "saving" | "saved" | "failed">("idle");
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [pending, startTransition] = useTransition();
  // Delivery boy cards save themselves; Finalize first waits for any typing still to be saved.
  const salesFlushers = useRef(new Set<() => Promise<void>>());
  const [salesSummary, setSalesSummary] = useState<SalesSummary>(() => summarizeSaved(sales));

  // ---- Auto-save: every change is saved shortly after the operator stops typing ----
  const edited = useRef(false); // nothing to save until the operator changes something
  const unsaved = useRef<SaveBody | null>(null);
  // Saves run one after another, so an older save can never land after a newer one.
  const queue = useRef<Promise<void>>(Promise.resolve());
  const saving = useRef(0);
  // The counts as last loaded or saved. Each save sends them along, so only the cells typed
  // here are written and counts typed on another PC of the office meanwhile are kept.
  const baseline = useRef<SaveBody>({ cylinderStock: initial.cylinderStock });
  const adopting = useRef(false);

  const flush = useCallback(() => {
    const body = unsaved.current;
    if (!body) return queue.current;
    unsaved.current = null;
    setSaveState("saving");
    saving.current++;
    queue.current = queue.current.then(async () => {
      const res = await mutate<{ savedAt: string }>("save_day_end", { ...body, base: baseline.current });
      saving.current--;
      if (res.ok) {
        baseline.current = body;
        setSavedAt(res.data.savedAt);
        setError(null);
      } else {
        setError(res.error);
        unsaved.current ??= body; // retried with the next change or when leaving the page
      }
      if (!unsaved.current) setSaveState("saved");
      else if (!res.ok) setSaveState("failed");
    });
    return queue.current;
  }, []);

  // Counts saved on another PC show up here as they arrive, unless this PC has unsaved typing
  // (then they show after the save, which keeps both).
  useEffect(() => {
    if (unsaved.current || saving.current > 0) return;
    const server = { cylinderStock: initial.cylinderStock };
    baseline.current = server;
    if (JSON.stringify(server.cylinderStock) === JSON.stringify(stock)) return;
    adopting.current = true; // not the operator's typing: nothing to save
    setStock(server.cylinderStock);
    // Only when new data arrives, not on every keystroke.
  }, [initial]);

  useEffect(() => {
    if (adopting.current) {
      adopting.current = false;
      return;
    }
    if (!edited.current || initial.isFinalized) return;
    unsaved.current = { cylinderStock: stock };
    setSaveState("waiting");
    const t = setTimeout(flush, AUTOSAVE_MS);
    return () => clearTimeout(t);
  }, [stock, flush, initial.isFinalized]);

  // Leaving the page (or the day rolling over) saves whatever is still waiting.
  useEffect(() => () => void flush(), [flush]);

  // Warn before closing while a save is still waiting or failed.
  const unsafe = saveState === "waiting" || saveState === "saving" || saveState === "failed";
  useEffect(() => {
    if (!unsafe) return;
    const handler = (e: BeforeUnloadEvent) => e.preventDefault();
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [unsafe]);

  // Ctrl/Cmd+S still works: saves right away instead of waiting.
  useEffect(() => {
    if (initial.isFinalized) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        flush();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [flush, initial.isFinalized]);

  if (initial.isFinalized) {
    return (
      <div className="space-y-4">
        <div className="flex items-center gap-3 rounded-xl border border-emerald-300 bg-emerald-50 px-4 py-3 text-emerald-900">
          <CheckCircle2 className="h-5 w-5" />
          <p className="font-medium">
            {formatLongDate(initial.dateString)} is finalized
            {initial.finalizedAt ? ` at ${formatDateTime(initial.finalizedAt)}` : ""}. All entries are locked.
          </p>
        </div>
        <DeliverySalesView sales={sales} />
        <StockView stock={initial.cylinderStock} />
      </div>
    );
  }

  const updateStock = (key: keyof CylinderStockDTO, field: "full" | "empty", v: string) => {
    edited.current = true;
    setStock((s) => ({ ...s, [key]: { ...s[key], [field]: toCount(v) } }));
  };

  const doFinalize = () => {
    startTransition(async () => {
      // Finalize saves these same numbers, so drop the waiting auto-save and let any running one finish.
      unsaved.current = null;
      await Promise.all([queue.current, ...[...salesFlushers.current].map((f) => f())]);
      const res = await mutate("finalize_day", { cylinderStock: stock, base: baseline.current });
      if (res.ok) {
        setConfirming(false);
      } else {
        setError(res.error);
        setConfirming(false);
        unsaved.current = { cylinderStock: stock }; // not finalized: still save the numbers
        flush();
      }
    });
  };


  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-xl font-bold text-slate-800">Day End & Stock Reconciliation</h1>
          <p className="text-sm text-slate-500">{formatLongDate(initial.dateString)}</p>
        </div>
        <SaveStatus state={saveState} savedAt={savedAt} />
      </div>

      {error && (
        <p role="alert" className="rounded-lg border border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
          {error}
        </p>
      )}

      <DeliverySalesSection sales={sales} flushers={salesFlushers} onSummary={setSalesSummary} />

      <Card title="Cylinder Stock Count" icon={Cylinder}>
        <div className="grid gap-4 p-4 md:grid-cols-3">
          {STOCK_ROWS.map(({ key, label }) => (
            <div key={key} className="rounded-lg border border-slate-200 p-3">
              <p className="mb-2 text-center text-base font-bold text-slate-800">{label}</p>
              <div className="grid grid-cols-2 gap-2">
                {(["full", "empty"] as const).map((field) => (
                  <label key={field} className="text-xs font-semibold uppercase tracking-wide text-slate-500">
                    {field}
                    <input
                      inputMode="numeric"
                      value={stock[key][field] || ""}
                      placeholder="0"
                      onFocus={(e) => e.target.select()}
                      onChange={(e) => updateStock(key, field, e.target.value)}
                      className={`${numberInput} mt-1 text-base`}
                    />
                  </label>
                ))}
              </div>
            </div>
          ))}
        </div>
      </Card>

      <div className="flex flex-col items-center gap-2 rounded-xl border-2 border-dashed border-hp-red/40 bg-white p-5">
        <p className="text-sm text-slate-600">
          Finalizing saves these numbers and permanently locks today&apos;s register. This cannot be undone.
        </p>
        <button
          type="button"
          onClick={() => setConfirming(true)}
          disabled={pending}
          className="flex items-center gap-2 rounded-lg bg-hp-red px-8 py-3 text-lg font-bold text-white shadow hover:bg-red-700 disabled:opacity-60"
        >
          <Lock className="h-5 w-5" /> Finalize Day
        </button>
      </div>

      {confirming && (
        <ConfirmFinalize
          date={initial.dateString}
          totals={totals}
          sales={salesSummary}
          pending={pending}
          onCancel={() => setConfirming(false)}
          onConfirm={doFinalize}
        />
      )}
    </div>
  );
}

function SaveStatus({ state, savedAt }: { state: "idle" | "waiting" | "saving" | "saved" | "failed"; savedAt: string | null }) {
  if (state === "idle") return <span className="text-sm text-slate-400">Changes save automatically</span>;
  if (state === "failed") return <span className="text-sm font-medium text-red-600">Not saved · will retry</span>;
  if (state === "saved")
    return (
      <span className="flex items-center gap-1.5 text-sm text-emerald-700">
        <Check className="h-4 w-4" /> Saved{savedAt ? ` at ${formatTime(savedAt)}` : ""}
      </span>
    );
  return (
    <span className="flex items-center gap-1.5 text-sm text-slate-500">
      <Loader2 className="h-4 w-4 animate-spin" /> Saving…
    </span>
  );
}

function ConfirmFinalize(props: {
  date: string;
  totals: Totals;
  sales: SalesSummary;
  pending: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { onCancel, pending } = props;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && !pending && onCancel();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel, pending]);

  const t = props.totals;
  // Online/System money never passes through the drawer: listed on their own, only when used.
  const received = (m: "ONLINE" | "SYSTEM") => t.inByMode[m] - t.outByMode[m];
  const rows = [
    ["Cash In", formatINR(t.cashIn)],
    ["Cash Out", formatINR(t.cashOut)],
    [t.hasNonCash ? "Net Cash (in drawer)" : "Net Cash", formatINR(t.net)],
    ...(["ONLINE", "SYSTEM"] as const)
      .filter((m) => t.inByMode[m] || t.outByMode[m])
      .map((m) => [`${PAYMENT_LABEL[m]} (in − out)`, formatINR(received(m))]),
    ["Active entries", String(t.count)],
    // Delivery boys, when any reported today.
    ...(props.sales.boys
      ? [
          ["Delivery boys sold", props.sales.sold.map(([n, q]) => `${n} × ${q}`).join(", ") || "—"],
          ["Delivery boys received", `${formatINR(props.sales.cash + props.sales.online)} (online ${formatINR(props.sales.online)})`],
          ...(props.sales.systemCount
            ? [[`Delivery boys system payments (${props.sales.systemCount})`, formatINR(props.sales.system)]]
            : []),
          ["Delivery boy expenses", formatINR(props.sales.expenses)],
          ["Cash to collect from delivery boys", formatINR(props.sales.handOver)],
        ]
      : []),
  ];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 p-4" role="dialog" aria-modal="true" aria-labelledby="finalize-title">
      <div className="w-full max-w-md rounded-xl bg-white p-6 shadow-xl">
        <div className="mb-4 flex items-start gap-3">
          <div className="rounded-full bg-red-100 p-2 text-hp-red">
            <AlertTriangle className="h-5 w-5" />
          </div>
          <div>
            <h2 id="finalize-title" className="text-lg font-bold">
              Finalize {formatLongDate(props.date)}?
            </h2>
            <p className="text-sm text-slate-600">After this, no one can add, edit or delete today&apos;s entries.</p>
          </div>
        </div>
        <dl className="mb-5 divide-y divide-slate-100 rounded-lg border border-slate-200 text-sm">
          {rows.map(([k, v]) => (
            <div key={k} className="flex justify-between px-3 py-1.5">
              <dt className="text-slate-500">{k}</dt>
              <dd className="tabular font-semibold">{v}</dd>
            </div>
          ))}
        </dl>
        <div className="flex justify-end gap-2">
          <button
            type="button"
            autoFocus
            onClick={onCancel}
            disabled={pending}
            className="rounded-lg border border-slate-300 px-4 py-2 font-medium hover:bg-slate-50"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={props.onConfirm}
            disabled={pending}
            className="flex items-center gap-2 rounded-lg bg-hp-red px-4 py-2 font-bold text-white hover:bg-red-700 disabled:opacity-60"
          >
            {pending ? <Loader2 className="h-4 w-4 animate-spin" /> : <Lock className="h-4 w-4" />}
            Yes, finalize day
          </button>
        </div>
      </div>
    </div>
  );
}
