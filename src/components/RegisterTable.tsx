import { useState, useTransition, type KeyboardEvent } from "react";
import { Bike, Check, CloudUpload, Eye, EyeOff, Loader2, Pencil, Trash2, X } from "lucide-react";
import { mutate } from "@/lib/api";
import { formatDateTime, formatTime } from "@/lib/date";
import { formatINR } from "@/lib/format";
import { PAYMENT_LABEL, PAYMENT_MODES, PAYMENT_TAG } from "@/lib/payment";
import { useQuery } from "@/lib/useQuery";
import type { AuditEntryDTO, OfficeSettingsDTO, PaymentMode, TransactionDTO } from "@/lib/types";

type Props = {
  rows: TransactionDTO[];
  editable: boolean;
  title: string;
};

const cn = (n?: string) => (n ? ` #${n}` : "");
const paid = (m?: PaymentMode) => (m && m !== "CASH" ? ` · ${PAYMENT_LABEL[m]}` : "");
const qty = (n?: number) => (n ? ` · Qty ${n}` : "");
const CONSUMER_NO_DIGITS = 6;
const OTHER_NOTE_MAX = 60;
const QUANTITY_DIGITS = 4;

function describeAudit(a: AuditEntryDTO) {
  const when = formatDateTime(a.timestamp);
  // Payment type is shown when it isn't cash, and on both sides whenever an edit changed it.
  const modeChanged = a.previousPaymentMode && a.newPaymentMode && a.previousPaymentMode !== a.newPaymentMode;
  const pm = (m?: PaymentMode) => (modeChanged ? ` · ${PAYMENT_LABEL[m ?? "CASH"]}` : paid(m));
  const prev = `${a.previousProductName} @ ${formatINR(a.previousPrice ?? 0)}${cn(a.previousConsumerNo)}${qty(a.previousQuantity)}${pm(a.previousPaymentMode)}`;
  const next = `${a.newProductName} @ ${formatINR(a.newPrice ?? 0)}${cn(a.newConsumerNo)}${qty(a.newQuantity)}${pm(a.newPaymentMode)}`;
  const by = a.user ? ` · by ${a.user}` : "";
  if (a.action === "CREATED") return `${when} · Created: ${next}${by}`;
  if (a.action === "DELETED") return `${when} · Deleted (was ${prev})${by}`;
  return `${when} · Edited: ${prev} → ${next}${by}`;
}

export function RegisterTable({ rows, editable, title }: Props) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftName, setDraftName] = useState("");
  const [draftPrice, setDraftPrice] = useState("");
  const [draftConsumer, setDraftConsumer] = useState("");
  const [draftPayment, setDraftPayment] = useState<PaymentMode>("CASH");
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showDeleted, setShowDeleted] = useState(true);
  const [, startTransition] = useTransition();
  // A System Cash In keeps its fixed price; one switched to System takes today's (Settings).
  const { data: officeSettings } = useQuery<OfficeSettingsDTO>("get_office_settings");
  const fixedPriceOf = (r: TransactionDTO) =>
    r.type === "CASH_IN" && draftPayment === "SYSTEM" ? (r.paymentMode === "SYSTEM" ? r.price : (officeSettings?.systemPrice ?? null)) : undefined;

  const deletedCount = rows.filter((r) => r.status === "DELETED").length;
  // Entries from more than one operator or PC of the office: show who made each one.
  const who = (r: TransactionDTO) => r.createdByUser ?? r.createdByName;
  const severalPcs = new Set(rows.map((r) => r.createdByUser ?? r.createdBy)).size > 1;
  const visible = showDeleted ? rows : rows.filter((r) => r.status !== "DELETED");

  const startEdit = (r: TransactionDTO) => {
    setEditingId(r.id);
    setDraftName(r.productName);
    setDraftPrice(String(r.price));
    setDraftConsumer(r.roleId ? String(r.quantity ?? "") : (r.consumerNo ?? ""));
    setDraftPayment(r.paymentMode ?? "CASH");
    setError(null);
  };

  const cancelEdit = () => {
    setEditingId(null);
    setError(null);
  };

  const saveEdit = (r: TransactionDTO) => {
    const id = r.id;
    setBusyId(id);
    startTransition(async () => {
      // Delivery boy entries edit the quantity in the box where others edit the consumer number.
      const res = await mutate("update_transaction", {
        id,
        productName: draftName,
        price: draftPrice,
        ...(r.roleId ? { quantity: draftConsumer || null } : { consumerNo: draftConsumer }),
        paymentMode: draftPayment,
      });
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
      const res = await mutate("delete_transaction", { id: r.id });
      setBusyId(null);
      if (!res.ok) setError(res.error);
    });
  };

  const onEditKeyDown = (e: KeyboardEvent<HTMLElement>, r: TransactionDTO) => {
    if (e.key === "Enter") {
      e.preventDefault();
      saveEdit(r);
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
              <th className="w-24 px-2 py-2">Time</th>
              <th className="w-36 px-2 py-2">Type</th>
              <th className="w-32 px-2 py-2">Consumer No</th>
              <th className="px-2 py-2">Product / Item</th>
              <th className="w-36 px-2 py-2 text-right">Amount</th>
              {editable && <th className="w-28 px-4 py-2 text-right">Actions</th>}
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {visible.length === 0 && (
              <tr>
                <td colSpan={editable ? 7 : 6} className="px-4 py-10 text-center text-slate-400">
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
                  <td className="tabular px-2 py-2">
                    <span className="flex items-center gap-1">
                      {formatTime(r.createdAt)}
                      {!r.synced && (
                        <span title="Saved on this PC. Waiting to upload to the cloud.">
                          <CloudUpload aria-label="Waiting to sync" className="h-3.5 w-3.5 text-amber-500" />
                        </span>
                      )}
                    </span>
                    {severalPcs && (
                      <span
                        title={`Entered${r.createdByUser ? ` by ${r.createdByUser}` : ""} on ${r.createdByName ?? "another PC"}`}
                        className="block max-w-24 truncate text-[10px] text-slate-400"
                      >
                        {who(r) ?? "Other PC"}
                      </span>
                    )}
                  </td>
                  <td className="whitespace-nowrap px-2 py-2">
                    <span
                      className={`rounded px-2 py-0.5 text-xs font-bold ${
                        deleted ? "bg-slate-200 text-slate-500" : cashIn ? "bg-emerald-100 text-emerald-800" : "bg-rose-100 text-rose-800"
                      }`}
                    >
                      {cashIn ? "CASH IN" : "CASH OUT"}
                    </span>
                    {editing ? (
                      <select
                        aria-label="Payment"
                        value={draftPayment}
                        onChange={(e) => setDraftPayment(e.target.value as PaymentMode)}
                        onKeyDown={(e) => onEditKeyDown(e, r)}
                        className="mt-1 block w-full rounded border border-slate-300 bg-white px-1 py-0.5 text-xs outline-none focus:border-hp-blue"
                      >
                        {PAYMENT_MODES.map((m) => (
                          <option key={m} value={m}>
                            {PAYMENT_LABEL[m]}
                          </option>
                        ))}
                      </select>
                    ) : (
                      r.paymentMode &&
                      r.paymentMode !== "CASH" && (
                        <span
                          title={`Paid ${PAYMENT_LABEL[r.paymentMode].toLowerCase()}: not counted in Net Cash`}
                          className={`ml-1 rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase ${
                            deleted ? "bg-slate-200 text-slate-500" : PAYMENT_TAG[r.paymentMode]
                          }`}
                        >
                          {PAYMENT_LABEL[r.paymentMode]}
                        </span>
                      )
                    )}
                    {r.roleName && (
                      <span title={`Entry for delivery boy ${r.roleName}`} className="mt-0.5 flex max-w-36 items-center gap-1 text-[11px] font-medium text-amber-800">
                        <Bike className="h-3 w-3 shrink-0" />
                        <span className="truncate">{r.roleName}</span>
                      </span>
                    )}
                  </td>
                  <td className="tabular max-w-40 px-2 py-2">
                    {editing && cashIn && r.roleId ? (
                      <input
                        autoFocus
                        inputMode="numeric"
                        maxLength={QUANTITY_DIGITS}
                        aria-label="Quantity"
                        value={draftConsumer}
                        placeholder={r.isOtherProduct ? "Optional" : "Qty"}
                        onFocus={(e) => e.target.select()}
                        onChange={(e) => setDraftConsumer(e.target.value.replace(/\D/g, "").slice(0, QUANTITY_DIGITS))}
                        onKeyDown={(e) => onEditKeyDown(e, r)}
                        className="w-full rounded border border-slate-300 px-2 py-1 outline-none focus:border-hp-blue"
                      />
                    ) : editing && cashIn ? (
                      <input
                        autoFocus
                        inputMode={r.isOtherProduct ? "text" : "numeric"}
                        maxLength={r.isOtherProduct ? OTHER_NOTE_MAX : CONSUMER_NO_DIGITS}
                        aria-label={r.isOtherProduct ? "Note (optional)" : "Consumer number"}
                        value={draftConsumer}
                        placeholder={r.isOtherProduct ? "Optional · any text" : "6 digits"}
                        onFocus={(e) => e.target.select()}
                        onChange={(e) => {
                          const v = e.target.value;
                          setDraftConsumer(r.isOtherProduct ? v.slice(0, OTHER_NOTE_MAX) : v.replace(/\D/g, "").slice(0, CONSUMER_NO_DIGITS));
                        }}
                        onKeyDown={(e) => onEditKeyDown(e, r)}
                        className="w-full rounded border border-slate-300 px-2 py-1 outline-none focus:border-hp-blue"
                      />
                    ) : !cashIn ? (
                      <span className="text-slate-300">—</span>
                    ) : (
                      <span className="flex min-w-0 items-center gap-1.5">
                        {r.quantity != null && (
                          <span className={deleted ? "line-through" : "font-medium text-slate-800"}>Qty {r.quantity}</span>
                        )}
                        {r.consumerNo && (
                          <span title={r.consumerNo} className={`truncate ${deleted ? "line-through" : "font-medium text-slate-800"}`}>
                            {r.consumerNo}
                          </span>
                        )}
                        {r.isOtherProduct && (
                          <span className="shrink-0 rounded bg-sky-100 px-1.5 py-0.5 text-[10px] font-semibold uppercase text-sky-800">Other</span>
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
                        onKeyDown={(e) => onEditKeyDown(e, r)}
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
                    {editing && fixedPriceOf(r) !== undefined ? (
                      <input
                        readOnly
                        aria-label="Price"
                        title="Fixed System payment price (Settings)"
                        value={fixedPriceOf(r)?.toFixed(2) ?? ""}
                        placeholder="Set in Settings"
                        onKeyDown={(e) => onEditKeyDown(e, r)}
                        className="w-28 rounded border border-orange-300 bg-orange-50 px-2 py-1 text-right text-orange-900 outline-none placeholder:text-xs focus:border-hp-blue"
                      />
                    ) : editing ? (
                      <input
                        inputMode="decimal"
                        value={draftPrice}
                        onChange={(e) => setDraftPrice(e.target.value.replace(/[^\d.]/g, ""))}
                        onKeyDown={(e) => onEditKeyDown(e, r)}
                        className="w-28 rounded border border-slate-300 px-2 py-1 text-right outline-none focus:border-hp-blue"
                      />
                    ) : (
                      <span className={deleted ? "line-through" : `font-semibold ${cashIn ? "text-emerald-700" : "text-rose-700"}`}>
                        {cashIn ? "+" : "−"} {formatINR(r.price)}
                      </span>
                    )}
                  </td>
                  {editable && (
                    <td className="px-4 py-2">
                      <div className="flex justify-end gap-1">
                        {busy ? (
                          <Loader2 className="h-4 w-4 animate-spin text-slate-400" />
                        ) : editing ? (
                          <>
                            <IconButton label="Save (Enter)" onClick={() => saveEdit(r)} className="text-emerald-700 hover:bg-emerald-50">
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
