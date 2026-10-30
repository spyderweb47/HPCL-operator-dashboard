import { useEffect, useRef, useState, useTransition } from "react";
import { Link } from "react-router";
import { AlertTriangle, Bike, CheckCircle2, Cylinder, Loader2, Lock, Save } from "lucide-react";
import { mutate } from "@/lib/api";
import { formatDateTime, formatLongDate, formatTime } from "@/lib/date";
import { formatINR } from "@/lib/format";
import type { CylinderStockDTO, DayEndDTO, DeliveryDTO, Totals } from "@/lib/types";
import { Card, DeliveriesView, STOCK_ROWS, StockView } from "./DayEndViews";

type Props = { initial: DayEndDTO; totals: Totals };

const numberInput =
  "tabular w-full rounded-md border border-slate-300 px-3 py-2 text-right outline-none focus:border-hp-blue focus:ring-2 focus:ring-hp-blue/20 disabled:bg-slate-50";

const toCount = (v: string) => {
  const n = parseInt(v.replace(/\D/g, ""), 10);
  return Number.isFinite(n) ? n : 0;
};

export function DayEndForm({ initial, totals }: Props) {
  const [deliveries, setDeliveries] = useState<DeliveryDTO[]>(initial.deliveries);
  const [stock, setStock] = useState<CylinderStockDTO>(initial.cylinderStock);
  const [dirty, setDirty] = useState(false);
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [pending, startTransition] = useTransition();
  const tableRef = useRef<HTMLTableSectionElement>(null);

  // Warn before leaving with unsaved counts.
  useEffect(() => {
    if (!dirty) return;
    const handler = (e: BeforeUnloadEvent) => e.preventDefault();
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [dirty]);

  const save = () => {
    startTransition(async () => {
      const res = await mutate<{ savedAt: string }>("save_day_end", { deliveries, cylinderStock: stock });
      if (res.ok) {
        setSavedAt(res.data.savedAt);
        setDirty(false);
        setError(null);
      } else setError(res.error);
    });
  };

  // Ctrl/Cmd+S saves progress.
  const saveRef = useRef(save);
  useEffect(() => {
    saveRef.current = save;
  });
  useEffect(() => {
    if (initial.isFinalized) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        saveRef.current();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [initial.isFinalized]);

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
        <div className="grid gap-4 lg:grid-cols-2">
          <DeliveriesView deliveries={initial.deliveries} />
          <StockView stock={initial.cylinderStock} />
        </div>
      </div>
    );
  }

  const updateDelivery = (i: number, patch: Partial<DeliveryDTO>) => {
    setDeliveries((ds) => ds.map((d, j) => (j === i ? { ...d, ...patch } : d)));
    setDirty(true);
  };

  /** Enter moves down the column, like a spreadsheet. */
  const focusNextRow = (i: number, col: "bills" | "cyl") => {
    tableRef.current?.querySelector<HTMLInputElement>(`input[data-row="${i + 1}"][data-col="${col}"]`)?.focus();
  };

  const updateStock = (key: keyof CylinderStockDTO, field: "full" | "empty", v: string) => {
    setStock((s) => ({ ...s, [key]: { ...s[key], [field]: toCount(v) } }));
    setDirty(true);
  };

  const doFinalize = () => {
    startTransition(async () => {
      const res = await mutate("finalize_day", { deliveries, cylinderStock: stock });
      if (res.ok) {
        setDirty(false);
        setConfirming(false);
      } else {
        setError(res.error);
        setConfirming(false);
      }
    });
  };

  const totalBills = deliveries.reduce((s, d) => s + d.billsPrinted, 0);
  const totalCyl = deliveries.reduce((s, d) => s + d.cylindersDelivered, 0);

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-xl font-bold text-slate-800">Day End & Stock Reconciliation</h1>
          <p className="text-sm text-slate-500">{formatLongDate(initial.dateString)}</p>
        </div>
        <div className="flex items-center gap-3">
          <span className="text-sm text-slate-500">
            {dirty ? (
              <span className="font-medium text-amber-700">Unsaved changes</span>
            ) : savedAt ? (
              `Saved at ${formatTime(savedAt)}`
            ) : null}
          </span>
          <button
            type="button"
            onClick={save}
            disabled={pending}
            title="Save progress (Ctrl+S)"
            className="flex items-center gap-2 rounded-lg border border-hp-blue bg-white px-4 py-2 font-semibold text-hp-blue hover:bg-blue-50 disabled:opacity-60"
          >
            {pending && !confirming ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
            Save Progress
          </button>
        </div>
      </div>

      {error && (
        <p role="alert" className="rounded-lg border border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
          {error}
        </p>
      )}

      <Card
        title="Delivery Boy Tracker"
        icon={Bike}
        aside={
          <span className="tabular text-sm text-slate-500">
            Bills {totalBills} · Delivered {totalCyl}
          </span>
        }
      >
        <table className="w-full text-sm">
          <thead className="bg-slate-50 text-left text-xs uppercase tracking-wide text-slate-500">
            <tr>
              <th className="px-4 py-2">Delivery Boy</th>
              <th className="w-44 px-2 py-2 text-right">Bills Printed</th>
              <th className="w-44 px-4 py-2 text-right">Cylinders Delivered</th>
            </tr>
          </thead>
          <tbody ref={tableRef} className="divide-y divide-slate-100">
            {deliveries.length === 0 && (
              <tr>
                <td colSpan={3} className="px-4 py-6 text-center text-slate-400">
                  No delivery boys yet.{" "}
                  <Link to="/settings" className="font-medium text-hp-blue underline">
                    Add them in Settings
                  </Link>
                  .
                </td>
              </tr>
            )}
            {deliveries.map((d, i) => (
              <tr key={d.deliveryBoyId}>
                <td className="px-4 py-2 font-medium text-slate-800">{d.deliveryBoyName}</td>
                <td className="px-2 py-2">
                  <input
                    inputMode="numeric"
                    aria-label={`Bills printed by ${d.deliveryBoyName}`}
                    data-row={i}
                    data-col="bills"
                    value={d.billsPrinted || ""}
                    placeholder="0"
                    onFocus={(e) => e.target.select()}
                    onChange={(e) => updateDelivery(i, { billsPrinted: toCount(e.target.value) })}
                    onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), focusNextRow(i, "bills"))}
                    className={numberInput}
                  />
                </td>
                <td className="px-4 py-2">
                  <input
                    inputMode="numeric"
                    aria-label={`Cylinders delivered by ${d.deliveryBoyName}`}
                    data-row={i}
                    data-col="cyl"
                    value={d.cylindersDelivered || ""}
                    placeholder="0"
                    onFocus={(e) => e.target.select()}
                    onChange={(e) => updateDelivery(i, { cylindersDelivered: toCount(e.target.value) })}
                    onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), focusNextRow(i, "cyl"))}
                    className={numberInput}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </Card>

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
          deliveries={deliveries.filter((d) => d.billsPrinted || d.cylindersDelivered).length}
          totalBills={totalBills}
          totalCyl={totalCyl}
          pending={pending}
          onCancel={() => setConfirming(false)}
          onConfirm={doFinalize}
        />
      )}
    </div>
  );
}

function ConfirmFinalize(props: {
  date: string;
  totals: Totals;
  deliveries: number;
  totalBills: number;
  totalCyl: number;
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

  const rows = [
    ["Cash In", formatINR(props.totals.cashIn)],
    ["Cash Out", formatINR(props.totals.cashOut)],
    ["Net Cash", formatINR(props.totals.net)],
    ["Active entries", String(props.totals.count)],
    ["Delivery boys with counts", String(props.deliveries)],
    ["Bills printed / Delivered", `${props.totalBills} / ${props.totalCyl}`],
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
