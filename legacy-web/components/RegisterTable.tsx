"use client";

import { useState, useTransition, type KeyboardEvent } from "react";
import { Check, Eye, EyeOff, Loader2, Pencil, Trash2, X } from "lucide-react";
import { deleteTransaction, updateTransaction } from "@/app/actions/transactions";
import { formatDateTime, formatTime } from "@/lib/date";
import { formatINR } from "@/lib/format";
import type { AuditEntryDTO, TransactionDTO } from "@/lib/types";

type Props = {
  rows: TransactionDTO[];
  editable: boolean;
  title: string;
};

const cn = (n?: string) => (n ? ` #${n}` : "");

function describeAudit(a: AuditEntryDTO) {
  const who = a.changedByName ?? "unknown";
  const when = formatDateTime(a.timestamp);
  const prev = `${a.previousProductName} @ ${formatINR(a.previousPrice ?? 0)}${cn(a.previousConsumerNo)}`;
  const next = `${a.newProductName} @ ${formatINR(a.newPrice ?? 0)}${cn(a.newConsumerNo)}`;
  if (a.action === "CREATED") return `${when} · Created by ${who}: ${next}`;
  if (a.action === "DELETED") return `${when} · Deleted by ${who} (was ${prev})`;
  return `${when} · Edited by ${who}: ${prev} → ${next}`;
}

export function RegisterTable({ rows, editable, title }: Props) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftName, setDraftName] = useState("");
  const [draftPrice, setDraftPrice] = useState("");
  const [draftConsumer, setDraftConsumer] = useState("");
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showDeleted, setShowDeleted] = useState(true);
  const [, startTransition] = useTransition();

  const deletedCount = rows.filter((r) => r.status === "DELETED").length;
  const visible = showDeleted ? rows : rows.filter((r) => r.status !== "DELETED");

  const startEdit = (r: TransactionDTO) => {
    setEditingId(r.id);
    setDraftName(r.productName);
    setDraftPrice(String(r.price));
    setDraftConsumer(r.consumerNo ?? "");
    setError(null);
  };

  const cancelEdit = () => {
    setEditingId(null);
    setError(null);
  };

  const saveEdit = (id: string) => {
    setBusyId(id);
    startTransition(async () => {
      const res = await updateTransaction(id, { productName: draftName, price: draftPrice, consumerNo: draftConsumer });
      setBusyId(null);
      if (res.ok) {
        setEditingId(null);
        setError(null);
      } else setError(res.error);
    });
  };

  const remove = (r: TransactionDTO) => {
    if (!window.confirm(`Delete "${r.productName}" (${formatINR(r.price)})?\nThe entry stays in the audit trail.`)) return;
    setBusyId(r.id);
    setError(null);
    startTransition(async () => {
      const res = await deleteTransaction(r.id);
      setBusyId(null);
      if (!res.ok) setError(res.error);
    });
  };

  const onEditKeyDown = (e: KeyboardEvent<HTMLInputElement>, id: string) => {
    if (e.key === "Enter") {
      e.preventDefault();
      saveEdit(id);
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelEdit();
    }
  };

  return (
    <section className="overflow-hidden rounded-xl border border-slate-200 bg-white shadow-sm">
      <div className="flex items-center justify-between border-b border-slate-200 px-4 py-3">
        <h2 className="font-semibold text-slate-800">
          {title} <span className="ml-1 text-sm font-normal text-slate-500">({rows.length - deletedCount} entries)</span>
        </h2>
        {deletedCount > 0 && (
          <button
            type="button"
            onClick={() => setShowDeleted((v) => !v)}
            className="flex items-center gap-1.5 rounded-md px-2 py-1 text-sm text-slate-600 hover:bg-slate-100"
          >
            {showDeleted ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
            {showDeleted ? "Hide" : "Show"} {deletedCount} deleted
          </button>
        )}
      </div>

      {error && (
        <p role="alert" className="border-b border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
          {error}
        </p>
      )}

      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="bg-slate-50 text-left text-xs uppercase tracking-wide text-slate-500">
            <tr>
              <th className="w-12 px-4 py-2">#</th>
              <th className="w-20 px-2 py-2">Time</th>
              <th className="w-28 px-2 py-2">Type</th>
              <th className="w-32 px-2 py-2">Consumer No</th>
              <th className="px-2 py-2">Product / Item</th>
              <th className="w-36 px-2 py-2 text-right">Amount</th>
              <th className="w-32 px-2 py-2">Entered by</th>
              {editable && <th className="w-28 px-4 py-2 text-right">Actions</th>}
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {visible.length === 0 && (
              <tr>
                <td colSpan={editable ? 8 : 7} className="px-4 py-10 text-center text-slate-400">
                  No entries.
                </td>
              </tr>
            )}
            {visible.map((r) => {
              const serial = rows.length - rows.indexOf(r);
              const deleted = r.status === "DELETED";
              const editing = editingId === r.id;
              const busy = busyId === r.id;
              const canChange = editable && !deleted && !r.isDayFinalized;
              const history = r.auditLog.map(describeAudit).join("\n");
              const cashIn = r.type === "CASH_IN";
              return (
                <tr
                  key={r.id}
                  className={deleted ? "bg-slate-50 text-slate-400" : editing ? "bg-blue-50/60" : "hover:bg-slate-50"}
                >
                  <td className="tabular px-4 py-2 text-slate-400">{serial}</td>
                  <td className="tabular px-2 py-2">{formatTime(r.createdAt)}</td>
                  <td className="px-2 py-2">
                    <span
                      className={`rounded px-2 py-0.5 text-xs font-bold ${
                        deleted ? "bg-slate-200 text-slate-500" : cashIn ? "bg-emerald-100 text-emerald-800" : "bg-rose-100 text-rose-800"
                      }`}
                    >
                      {cashIn ? "CASH IN" : "CASH OUT"}
                    </span>
                  </td>
                  <td className="tabular px-2 py-2">
                    {editing && cashIn ? (
                      <input
                        autoFocus
                        inputMode="numeric"
                        maxLength={6}
                        aria-label="Consumer number"
                        value={draftConsumer}
                        placeholder={r.isOtherProduct ? "Optional" : "6 digits"}
                        onChange={(e) => setDraftConsumer(e.target.value.replace(/\D/g, "").slice(0, 6))}
                        onKeyDown={(e) => onEditKeyDown(e, r.id)}
                        className="w-full rounded border border-slate-300 px-2 py-1 outline-none focus:border-hp-blue"
                      />
                    ) : !cashIn ? (
                      <span className="text-slate-300">—</span>
                    ) : (
                      <span className="flex items-center gap-1.5">
                        {r.consumerNo && <span className={deleted ? "line-through" : "font-medium text-slate-800"}>{r.consumerNo}</span>}
                        {r.isOtherProduct && (
                          <span className="rounded bg-sky-100 px-1.5 py-0.5 text-[10px] font-semibold uppercase text-sky-800">Other</span>
                        )}
                      </span>
                    )}
                  </td>
                  <td className="px-2 py-2">
                    {editing ? (
                      <input
                        autoFocus={!cashIn}
                        value={draftName}
                        onChange={(e) => setDraftName(e.target.value)}
                        onKeyDown={(e) => onEditKeyDown(e, r.id)}
                        className="w-full rounded border border-slate-300 px-2 py-1 outline-none focus:border-hp-blue"
                      />
                    ) : (
                      <span className="flex items-center gap-2">
                        <span className={deleted ? "line-through" : "font-medium text-slate-800"}>{r.productName}</span>
                        {r.status === "EDITED" && (
                          <span title={history} className="cursor-help rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-semibold uppercase text-amber-800">
                            Edited
                          </span>
                        )}
                        {deleted && (
                          <span title={history} className="cursor-help rounded bg-slate-200 px-1.5 py-0.5 text-[10px] font-semibold uppercase text-slate-600">
                            Deleted
                          </span>
                        )}
                      </span>
                    )}
                  </td>
                  <td className="tabular px-2 py-2 text-right">
                    {editing ? (
                      <input
                        inputMode="decimal"
                        value={draftPrice}
                        onChange={(e) => setDraftPrice(e.target.value.replace(/[^\d.]/g, ""))}
                        onKeyDown={(e) => onEditKeyDown(e, r.id)}
                        className="w-28 rounded border border-slate-300 px-2 py-1 text-right outline-none focus:border-hp-blue"
                      />
                    ) : (
                      <span className={deleted ? "line-through" : `font-semibold ${cashIn ? "text-emerald-700" : "text-rose-700"}`}>
                        {cashIn ? "+" : "−"} {formatINR(r.price)}
                      </span>
                    )}
                  </td>
                  <td className="truncate px-2 py-2 text-slate-600">{r.createdByName}</td>
                  {editable && (
                    <td className="px-4 py-2">
                      <div className="flex justify-end gap-1">
                        {busy ? (
                          <Loader2 className="h-4 w-4 animate-spin text-slate-400" />
                        ) : editing ? (
                          <>
                            <IconButton label="Save (Enter)" onClick={() => saveEdit(r.id)} className="text-emerald-700 hover:bg-emerald-50">
                              <Check className="h-4 w-4" />
                            </IconButton>
                            <IconButton label="Cancel (Esc)" onClick={cancelEdit} className="text-slate-600 hover:bg-slate-100">
                              <X className="h-4 w-4" />
                            </IconButton>
                          </>
                        ) : canChange ? (
                          <>
                            <IconButton label="Edit" onClick={() => startEdit(r)} className="text-hp-blue hover:bg-blue-50">
                              <Pencil className="h-4 w-4" />
                            </IconButton>
                            <IconButton label="Delete" onClick={() => remove(r)} className="text-red-600 hover:bg-red-50">
                              <Trash2 className="h-4 w-4" />
                            </IconButton>
                          </>
                        ) : null}
                      </div>
                    </td>
                  )}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function IconButton({
  label,
  onClick,
  className,
  children,
}: {
  label: string;
  onClick: () => void;
  className: string;
  children: React.ReactNode;
}) {
  return (
    <button type="button" title={label} aria-label={label} onClick={onClick} className={`rounded p-1.5 ${className}`}>
      {children}
    </button>
  );
}
