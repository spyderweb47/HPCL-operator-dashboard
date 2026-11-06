import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Link } from "react-router";
import { Bike, Check, Loader2, Plus, X } from "lucide-react";
import { mutate } from "@/lib/api";
import { formatINR } from "@/lib/format";
import { useQuery } from "@/lib/useQuery";
import type { DeliverySalesDTO, OfficeSettingsDTO, SalesSuggestions } from "@/lib/types";
import { AutoInput } from "./AutoInput";
import { Card } from "./DayEndViews";

// ---------------------------------------------------------------------------------------------
// Delivery boy sales: per delivery boy, what he sold (usually one product, sometimes more or none),
// the money he reported (cash and online on each product line; system payments one by one, with the consumer
// number and an optional name, at the fixed System price from Settings), and what he spent
// from it (fuel, service…). Product and expense names autocomplete from what was used before;
// new names are remembered once saved. Each card saves itself shortly after typing stops.
// ---------------------------------------------------------------------------------------------

const AUTOSAVE_MS = 700;
const LINES_MAX = 10;
const SYSTEM_LINES_MAX = 50;
const CONSUMER_NO_DIGITS = 6;
const NO_SUGGESTIONS: SalesSuggestions = { products: [], expenses: [], consumers: [] };

/** What a card sends to the backend (and what the summary adds up). */
export type SalesBody = {
  /** Each product line carries the cash and online money received for it. */
  products: { name: string; quantity: string; cash: string; online: string }[];
  systemPayments: { consumerNo: string; name: string; amount: string }[];
  expenses: { name: string; amount: string }[];
};

/** Money and cylinder totals of one or more sheets. */
export type SalesSummary = {
  sold: [string, number][];
  cash: number;
  online: number;
  /** System payments: their own total, outside "received". */
  system: number;
  systemCount: number;
  expenses: number;
  /** cash − expenses: what the delivery boys should hand over. */
  handOver: number;
  boys: number;
};

const num = (v: string | number) => {
  const n = typeof v === "number" ? v : parseFloat(v);
  return Number.isFinite(n) ? n : 0;
};
const paise = (n: number) => Math.round(n * 100) / 100;

function toBody(s: DeliverySalesDTO): SalesBody {
  const text = (n: number) => (n ? String(n) : "");
  return {
    products: s.products.map((p) => ({ name: p.name, quantity: text(p.quantity), cash: text(p.cash), online: text(p.online) })),
    systemPayments: s.systemPayments.map((p) => ({ consumerNo: p.consumerNo, name: p.name ?? "", amount: text(p.amount) })),
    expenses: s.expenses.map((e) => ({ name: e.name, amount: text(e.amount) })),
  };
}

/** For comparing what's typed with what's saved: same numbers and text, however they're written. */
const canon = (b: SalesBody) =>
  JSON.stringify([
    b.products.map((p) => [p.name.trim().replace(/\s+/g, " "), num(p.quantity), num(p.cash), num(p.online)]),
    b.systemPayments.map((p) => [p.consumerNo, p.name.trim().replace(/\s+/g, " "), num(p.amount)]),
    b.expenses.map((e) => [e.name.trim().replace(/\s+/g, " "), num(e.amount)]),
  ]);

export function summarize(bodies: SalesBody[]): SalesSummary {
  const sold = new Map<string, number>();
  let cash = 0,
    online = 0,
    system = 0,
    systemCount = 0,
    expenses = 0,
    boys = 0;
  for (const b of bodies) {
    let any = false;
    for (const p of b.products) {
      cash += num(p.cash);
      online += num(p.online);
      if (num(p.cash) || num(p.online)) any = true;
      const q = num(p.quantity);
      if (!q || !p.name.trim()) continue;
      sold.set(p.name.trim(), (sold.get(p.name.trim()) ?? 0) + q);
      any = true;
    }
    const spent = b.expenses.reduce((s, e) => s + num(e.amount), 0);
    const sys = b.systemPayments.reduce((s, p) => s + num(p.amount), 0);
    system += sys;
    systemCount += b.systemPayments.filter((p) => num(p.amount)).length;
    expenses += spent;
    if (any || sys || spent) boys++;
  }
  return {
    sold: [...sold.entries()].sort((a, b) => b[1] - a[1]),
    cash: paise(cash),
    online: paise(online),
    system: paise(system),
    systemCount,
    expenses: paise(expenses),
    handOver: paise(cash - expenses),
    boys,
  };
}

export const summarizeSaved = (sales: DeliverySalesDTO[]) => summarize(sales.map(toBody));

// ---- Editable sheet (today, not finalized) ----

type Props = {
  sales: DeliverySalesDTO[];
  /** Each card registers its "save now"; Finalize waits for all of them. */
  flushers: React.RefObject<Set<() => Promise<void>>>;
  onSummary: (s: SalesSummary) => void;
};

export function DeliverySalesSection({ sales, flushers, onSummary }: Props) {
  const { data: suggestions } = useQuery<SalesSuggestions>("sales_suggestions");
  const { data: settings } = useQuery<OfficeSettingsDTO>("get_office_settings");
  const systemPrice = settings?.systemPrice ?? null;
  // What each card currently shows (typed or saved), so the totals follow the typing.
  const [live, setLive] = useState<Record<string, SalesBody>>({});
  const report = useCallback((id: string, body: SalesBody) => setLive((l) => (l[id] === body ? l : { ...l, [id]: body })), []);
  const summary = summarize(sales.map((s) => live[s.deliveryBoyId] ?? toBody(s)));
  const key = JSON.stringify(summary);
  useEffect(() => onSummary(JSON.parse(key)), [key, onSummary]);

  return (
    <Card title="Delivery Boys" icon={Bike} clip={false} aside={<span className="text-sm text-slate-500">Sales, money and expenses</span>}>
      {sales.length === 0 ? (
        <p className="px-4 py-6 text-center text-sm text-slate-400">
          No delivery boys yet.{" "}
          <Link to="/settings" className="font-medium text-hp-blue underline">
            Add them in Settings
          </Link>
          .
        </p>
      ) : (
        <div className="text-sm">
          <div className={`${SHEET_COLS} border-b border-slate-200 bg-slate-50 py-2 text-[11px] font-semibold uppercase tracking-wide text-slate-500`}>
            <span>Delivery boy</span>
            <span className={LINE}>
              <span className="min-w-0 flex-1">Product</span>
              <span className={`${QTY} text-right`}>Qty</span>
              <span className={`${MONEY} text-right`}>Cash</span>
              <span className={`${MONEY} text-right text-violet-700`}>Online</span>
              <span className={X_SLOT} />
            </span>
            <span className="text-orange-700" title="One line per system payment: consumer number and name (optional). The price is fixed in Settings.">
              System · consumer · name{" "}
              {systemPrice ? (
                <span className="normal-case">({formatINR(systemPrice)} each)</span>
              ) : settings ? (
                <Link to="/settings" className="normal-case text-amber-700 underline">
                  (set the price in Settings)
                </Link>
              ) : null}
            </span>
            <span>Expenses · ₹</span>
            <span className="text-right" title="Cash received minus expenses">
              Hand over
            </span>
          </div>
          {sales.map((s) => (
            <SalesRow key={s.deliveryBoyId} initial={s} flushers={flushers} onLive={report} suggestions={suggestions ?? NO_SUGGESTIONS} systemPrice={systemPrice} />
          ))}
          <TotalsRow summary={summary} />
        </div>
      )}
    </Card>
  );
}

/** One line per delivery boy, columns shared with the header and totals. */
const SHEET_COLS = "grid grid-cols-[7.5rem_minmax(25rem,2.6fr)_minmax(16rem,1.6fr)_minmax(10rem,1fr)_8rem] items-start gap-x-2 px-4";
// A product line: product · qty · cash · online · ×. The header and totals use the same widths.
const LINE = "flex items-center gap-1";
const QTY = "w-14 shrink-0";
const MONEY = "w-[5.5rem] shrink-0";
const X_SLOT = "w-6 shrink-0"; // the × button

function TotalsRow({ summary: s }: { summary: SalesSummary }) {
  return (
    <div className={`${SHEET_COLS} rounded-b-xl border-t-2 border-slate-200 bg-slate-50 py-2.5 font-semibold`}>
      <span className="text-slate-800">Total</span>
      <span className={`${LINE} items-start`}>
        <span className="min-w-0 flex-1 font-normal text-slate-600">
          {s.sold.length ? s.sold.map(([name, q]) => `${name} × ${q}`).join(" · ") : <span className="text-slate-400">nothing sold yet</span>}
        </span>
        <span className={`tabular ${MONEY} text-right`}>{formatINR(s.cash)}</span>
        <span className={`tabular ${MONEY} text-right text-violet-800`}>{formatINR(s.online)}</span>
        <span className={X_SLOT} />
      </span>
      <span className="tabular pr-7 text-right text-orange-800">
        {s.systemCount ? <span className="mr-1.5 text-xs font-normal text-slate-500">{s.systemCount} ×</span> : null}
        {formatINR(s.system)}
      </span>
      {/* Under the expense amounts (the × buttons take the last 1.75rem). */}
      <span className="tabular pr-7 text-right text-rose-700">{formatINR(s.expenses)}</span>
      <span className={`tabular text-right text-base ${s.handOver < 0 ? "text-rose-700" : "text-hp-blue"}`}>{formatINR(s.handOver)}</span>
    </div>
  );
}

type SaveState = "idle" | "waiting" | "saving" | "saved" | "failed";

/** Rows typed on screen keep a stable key; a blank product row is always there to type into. */
type Draft = SalesBody & { keys: { products: number[]; expenses: number[]; system: number[] } };
let nextKey = 1;
const BLANK_LINE = { name: "", quantity: "", cash: "", online: "" };
const hasValues = (p: SalesBody["products"][number]) => !!(p.quantity || p.cash || p.online);
const withKeys = (b: SalesBody): Draft => {
  const products = b.products.length ? b.products : [BLANK_LINE];
  return {
    ...b,
    products,
    keys: { products: products.map(() => nextKey++), expenses: b.expenses.map(() => nextKey++), system: b.systemPayments.map(() => nextKey++) },
  };
};

// Widths are set where each box is used.
const box =
  "tabular rounded-md border border-slate-300 px-2.5 py-1.5 outline-none focus:border-hp-blue focus:ring-2 focus:ring-hp-blue/20";

function SalesRow({
  initial,
  flushers,
  onLive,
  suggestions,
  systemPrice,
}: {
  initial: DeliverySalesDTO;
  flushers: Props["flushers"];
  onLive: (id: string, b: SalesBody) => void;
  suggestions: SalesSuggestions;
  /** New system payment lines take this price (fixed in Settings). */
  systemPrice: number | null;
}) {
  const id = initial.deliveryBoyId;
  const usual = initial.usualProduct;
  const [draft, setDraft] = useState<Draft>(() => withKeys(toBody(initial)));
  const [state, setState] = useState<SaveState>("idle");
  const [error, setError] = useState<string | null>(null);

  // A blank first product row with a quantity or money is the usual product (shown in grey).
  const body = useCallback(
    (d: Draft): SalesBody => ({
      products: d.products
        .map((p, i) => ({ ...p, name: p.name.trim() || (i === 0 && hasValues(p) && usual ? usual : p.name) }))
        .filter((p) => p.name.trim() || hasValues(p)),
      systemPayments: d.systemPayments.filter((p) => p.consumerNo || p.name.trim()),
      // A row just added and still blank isn't saved (nor is it taken away while typing).
      expenses: d.expenses.filter((e) => e.name.trim() || e.amount),
    }),
    [usual],
  );

  const edited = useRef(false);
  const unsaved = useRef<SalesBody | null>(null);
  const queue = useRef<Promise<void>>(Promise.resolve());
  const saving = useRef(0);
  const adopting = useRef(false);

  const flush = useCallback(() => {
    const b = unsaved.current;
    if (!b) return queue.current;
    unsaved.current = null;
    setState("saving");
    saving.current++;
    queue.current = queue.current.then(async () => {
      const res = await mutate("save_delivery_sales", { deliveryBoyId: id, ...b });
      saving.current--;
      if (res.ok) setError(null);
      else {
        setError(res.error);
        unsaved.current ??= b; // retried with the next change or when leaving the page
      }
      setState(unsaved.current ? (res.ok ? "waiting" : "failed") : "saved");
    });
    return queue.current;
  }, [id]);

  useEffect(() => {
    const set = flushers.current;
    set.add(flush);
    return () => void set.delete(flush);
  }, [flush, flushers]);

  // Saved changes from the other PC show here, unless this card has typing of its own waiting.
  useEffect(() => {
    if (unsaved.current || saving.current > 0) return;
    const server = toBody(initial);
    if (canon(server) === canon(body(draft))) return;
    adopting.current = true; // not the operator's typing: nothing to save
    setDraft(withKeys(server));
    // Only when new data arrives, not on every keystroke.
  }, [initial]);

  useEffect(() => {
    onLive(id, body(draft));
    if (adopting.current) {
      adopting.current = false;
      return;
    }
    if (!edited.current) return;
    unsaved.current = body(draft);
    setState("waiting");
    const t = setTimeout(flush, AUTOSAVE_MS);
    return () => clearTimeout(t);
  }, [draft, body, flush, id, onLive]);

  useEffect(() => () => void flush(), [flush]);

  const unsafe = state === "waiting" || state === "saving" || state === "failed";
  useEffect(() => {
    if (!unsafe) return;
    const handler = (e: BeforeUnloadEvent) => e.preventDefault();
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [unsafe]);

  const change = (f: (d: Draft) => Draft) => {
    edited.current = true;
    setDraft(f);
  };
  const setProduct = (i: number, patch: Partial<Draft["products"][number]>) =>
    change((d) => ({ ...d, products: d.products.map((p, j) => (j === i ? { ...p, ...patch } : p)) }));
  const setExpense = (i: number, patch: Partial<Draft["expenses"][number]>) =>
    change((d) => ({ ...d, expenses: d.expenses.map((e, j) => (j === i ? { ...e, ...patch } : e)) }));
  const setSystem = (i: number, patch: Partial<Draft["systemPayments"][number]>) =>
    change((d) => ({ ...d, systemPayments: d.systemPayments.map((p, j) => (j === i ? { ...p, ...patch } : p)) }));
  /** A consumer number typed before fills in the name it had (if the name box is still empty). */
  const setConsumerNo = (i: number, raw: string) => {
    const consumerNo = raw.replace(/\D/g, "").slice(0, CONSUMER_NO_DIGITS);
    const known = consumerNo.length === CONSUMER_NO_DIGITS ? suggestions.consumers.find((c) => c.consumerNo === consumerNo) : undefined;
    change((d) => ({
      ...d,
      systemPayments: d.systemPayments.map((p, j) => (j === i ? { ...p, consumerNo, name: p.name || known?.name || "" } : p)),
    }));
  };
  const addSystem = () => {
    change((d) => ({
      ...d,
      systemPayments: [...d.systemPayments, { consumerNo: "", name: "", amount: systemPrice ? String(systemPrice) : "" }],
      keys: { ...d.keys, system: [...d.keys.system, nextKey++] },
    }));
    focusLast("input[data-consumer]");
  };
  const removeSystem = (i: number) =>
    change((d) => ({
      ...d,
      systemPayments: d.systemPayments.filter((_, j) => j !== i),
      keys: { ...d.keys, system: d.keys.system.filter((_, j) => j !== i) },
    }));

  const cardRef = useRef<HTMLDivElement>(null);
  // A row just added gets the cursor once it's on screen.
  const focusAfterRender = useRef<string | null>(null);
  useEffect(() => {
    const selector = focusAfterRender.current;
    if (!selector) return;
    focusAfterRender.current = null;
    const all = cardRef.current?.querySelectorAll<HTMLInputElement>(selector);
    all?.[all.length - 1]?.focus();
  });
  const focusLast = (selector: string) => {
    focusAfterRender.current = selector;
  };
  const addProduct = () => {
    change((d) => ({
      ...d,
      products: [...d.products, BLANK_LINE],
      keys: { ...d.keys, products: [...d.keys.products, nextKey++] },
    }));
    focusLast('input[data-auto="product"]');
  };
  const removeProduct = (i: number) =>
    change((d) => {
      const products = d.products.filter((_, j) => j !== i);
      const keys = d.keys.products.filter((_, j) => j !== i);
      // Always leave one row to type into.
      return products.length
        ? { ...d, products, keys: { ...d.keys, products: keys } }
        : { ...d, products: [BLANK_LINE], keys: { ...d.keys, products: [nextKey++] } };
    });
  const addExpense = () => {
    change((d) => ({
      ...d,
      expenses: [...d.expenses, { name: "", amount: "" }],
      keys: { ...d.keys, expenses: [...d.keys.expenses, nextKey++] },
    }));
    focusLast('input[data-auto="expense"]');
  };
  const removeExpense = (i: number) =>
    change((d) => ({
      ...d,
      expenses: d.expenses.filter((_, j) => j !== i),
      keys: { ...d.keys, expenses: d.keys.expenses.filter((_, j) => j !== i) },
    }));

  /** Enter moves to the next box of the row, like Tab. */
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "Enter" || !(e.target instanceof HTMLInputElement)) return;
    e.preventDefault();
    const all = [...(cardRef.current?.querySelectorAll<HTMLElement>("input, select") ?? [])];
    all[all.indexOf(e.target) + 1]?.focus();
  };

  const money = (v: string) => v.replace(/[^\d.]/g, "").replace(/^(\d*\.?\d{0,2}).*$/, "$1");
  const t = summarize([body(draft)]);
  // System payments are counted on their own, not in "received".
  const received = paise(t.cash + t.online);

  return (
    <div ref={cardRef} onKeyDown={onKeyDown} data-boy-row={initial.deliveryBoyName} className={`${SHEET_COLS} border-b border-slate-100 py-2`}>
      <div className="min-w-0 pt-1.5">
        <p className="flex items-center gap-1.5 font-semibold text-slate-800">
          <Bike className="h-4 w-4 shrink-0 text-amber-600" />
          <span className="truncate" title={initial.deliveryBoyName}>
            {initial.deliveryBoyName}
          </span>
        </p>
        <CardStatus state={state} />
      </div>

      <div>
        <div className="space-y-1">
          {draft.products.map((p, i) => (
            <div key={draft.keys.products[i]} className={LINE}>
              <AutoInput
                dataKey="product"
                ariaLabel={`Product sold by ${initial.deliveryBoyName}`}
                value={p.name}
                suggestions={suggestions.products}
                placeholder={i === 0 && usual ? usual : "Product"}
                onChange={(name) => setProduct(i, { name })}
                className={`${box} ${i === 0 && usual ? "placeholder:italic placeholder:text-slate-400" : ""}`}
              />
              <input
                inputMode="numeric"
                aria-label={`Quantity sold by ${initial.deliveryBoyName}`}
                value={p.quantity}
                placeholder="Qty"
                maxLength={4}
                onFocus={(e) => e.target.select()}
                onChange={(e) => setProduct(i, { quantity: e.target.value.replace(/\D/g, "").slice(0, 4) })}
                className={`${box} ${QTY} text-right`}
              />
              {(
                [
                  ["cash", "Cash"],
                  ["online", "Online"],
                ] as const
              ).map(([k, label]) => (
                <input
                  key={k}
                  inputMode="decimal"
                  aria-label={`${label} from ${initial.deliveryBoyName}`}
                  value={p[k]}
                  placeholder="0"
                  onFocus={(e) => e.target.select()}
                  onChange={(e) => setProduct(i, { [k]: money(e.target.value) })}
                  className={`${box} ${MONEY} text-right`}
                />
              ))}
              <RemoveButton
                label="Remove product"
                onClick={() => removeProduct(i)}
                hidden={draft.products.length === 1 && !p.name && !hasValues(p)}
              />
            </div>
          ))}
        </div>
        {draft.products.length < LINES_MAX && <AddButton text="Product" onClick={addProduct} />}
      </div>


      <div>
        {draft.systemPayments.length > 0 && (
          <div className="space-y-1">
            {draft.systemPayments.map((x, i) => (
              <div key={draft.keys.system[i]} className="flex items-center gap-1">
                <input
                  data-consumer
                  inputMode="numeric"
                  aria-label={`System payment consumer number, ${initial.deliveryBoyName}`}
                  value={x.consumerNo}
                  placeholder="6 digits"
                  maxLength={CONSUMER_NO_DIGITS}
                  onFocus={(e) => e.target.select()}
                  onChange={(e) => setConsumerNo(i, e.target.value)}
                  className={`${box} w-[5.5rem] shrink-0 text-center tracking-wider placeholder:tracking-normal ${
                    x.consumerNo && x.consumerNo.length < CONSUMER_NO_DIGITS ? "border-amber-400" : ""
                  }`}
                />
                <input
                  aria-label={`System payment name, ${initial.deliveryBoyName}`}
                  value={x.name}
                  maxLength={60}
                  placeholder="Name"
                  title="Consumer's name (optional)"
                  onChange={(e) => setSystem(i, { name: e.target.value })}
                  className={`${box} min-w-0 flex-1`}
                />
                <span
                  aria-label={`System payment amount, ${initial.deliveryBoyName}`}
                  title="Fixed System payment price (Settings)"
                  className="tabular w-16 shrink-0 text-right text-sm text-orange-800"
                >
                  {num(x.amount) ? formatINR(num(x.amount)) : "—"}
                </span>
                <RemoveButton label="Remove system payment" onClick={() => removeSystem(i)} />
              </div>
            ))}
          </div>
        )}
        {draft.systemPayments.length < SYSTEM_LINES_MAX && <AddButton text="System" onClick={addSystem} />}
      </div>

      <div>
        {draft.expenses.length > 0 && (
          <div className="space-y-1">
            {draft.expenses.map((x, i) => (
              <div key={draft.keys.expenses[i]} className="flex items-center gap-1">
                <AutoInput
                  dataKey="expense"
                  ariaLabel={`Expense of ${initial.deliveryBoyName}`}
                  value={x.name}
                  maxLength={40}
                  suggestions={suggestions.expenses}
                  placeholder="Fuel, service…"
                  onChange={(name) => setExpense(i, { name })}
                  className={box}
                />
                <input
                  inputMode="decimal"
                  aria-label="Expense amount"
                  value={x.amount}
                  placeholder="₹ 0"
                  onFocus={(e) => e.target.select()}
                  onChange={(e) => setExpense(i, { amount: money(e.target.value) })}
                  className={`${box} w-16 shrink-0 text-right`}
                />
                <RemoveButton label="Remove expense" onClick={() => removeExpense(i)} />
              </div>
            ))}
          </div>
        )}
        {draft.expenses.length < LINES_MAX && <AddButton text="Expense" onClick={addExpense} />}
      </div>

      <div className="pt-1.5 text-right" title="Cash received minus expenses">
        <b className={`tabular block ${t.handOver < 0 ? "text-rose-700" : "text-hp-blue"}`}>{formatINR(t.handOver)}</b>
        {received > 0 && <span className="tabular whitespace-nowrap text-[11px] text-slate-500">of {formatINR(received)} received</span>}
      </div>

      {error && (
        <p role="alert" className="col-span-full mt-1 text-sm font-medium text-red-600">
          {error}
        </p>
      )}
    </div>
  );
}

function CardStatus({ state }: { state: SaveState }) {
  if (state === "idle") return null;
  if (state === "failed") return <span className="text-xs font-medium text-red-600">Not saved</span>;
  if (state === "saved")
    return (
      <span className="flex items-center gap-1 text-xs text-emerald-700">
        <Check className="h-3.5 w-3.5" /> Saved
      </span>
    );
  return (
    <span className="flex items-center gap-1 text-xs text-slate-500">
      <Loader2 className="h-3.5 w-3.5 animate-spin" /> Saving…
    </span>
  );
}

function AddButton({ text, onClick }: { text: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="mt-1 flex items-center gap-1 rounded px-1 py-0.5 text-xs font-medium text-hp-blue hover:bg-blue-50"
    >
      <Plus className="h-3.5 w-3.5" /> {text}
    </button>
  );
}

function RemoveButton({ label, onClick, hidden }: { label: string; onClick: () => void; hidden?: boolean }) {
  return (
    <button
      type="button"
      tabIndex={-1}
      title={label}
      aria-label={label}
      onClick={onClick}
      className={`rounded p-1 text-slate-400 hover:bg-red-50 hover:text-red-600 ${hidden ? "invisible" : ""}`}
    >
      <X className="h-4 w-4" />
    </button>
  );
}

// ---- Read-only (finalized days, History) ----

export function DeliverySalesView({ sales }: { sales: DeliverySalesDTO[] }) {
  const rows = sales.filter((s) => s.products.some((p) => p.quantity > 0) || s.cash || s.online || s.system || s.expenses.some((e) => e.amount));
  if (rows.length === 0) return null;
  const total = summarizeSaved(rows);
  const spent = (s: DeliverySalesDTO) => paise(s.expenses.reduce((n, e) => n + e.amount, 0));
  return (
    <Card title="Delivery Boys" icon={Bike}>
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="bg-slate-50 text-left text-xs uppercase tracking-wide text-slate-500">
            <tr>
              <th className="px-4 py-2">Delivery Boy</th>
              <th className="px-2 py-2">Sold</th>
              <th className="px-2 py-2 text-right">Cash</th>
              <th className="px-2 py-2 text-right">Online</th>
              <th className="px-2 py-2 text-right">System</th>
              <th className="px-2 py-2 text-right">Expenses</th>
              <th className="px-4 py-2 text-right">Cash to hand over</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {rows.map((s) => (
              <tr key={s.deliveryBoyId}>
                <td className="px-4 py-2 font-medium text-slate-800">{s.deliveryBoyName}</td>
                <td className="px-2 py-2 text-slate-700">
                  {s.products.some((p) => p.quantity || p.cash || p.online) ? (
                    s.products
                      .filter((p) => p.quantity || p.cash || p.online)
                      .map((p, i) => (
                        <span key={i} className="block">
                          {p.name} × {p.quantity}
                          {/* Money per product, when he sold more than one. */}
                          {s.products.length > 1 && (p.cash || p.online) ? (
                            <span className="tabular ml-1.5 text-[11px] text-slate-500">
                              {[p.cash ? `cash ${formatINR(p.cash)}` : "", p.online ? `online ${formatINR(p.online)}` : ""].filter(Boolean).join(" · ")}
                            </span>
                          ) : null}
                        </span>
                      ))
                  ) : (
                    <span className="text-slate-300">—</span>
                  )}
                </td>
                <td className="tabular px-2 py-2 text-right">{formatINR(s.cash)}</td>
                <td className="tabular px-2 py-2 text-right">{formatINR(s.online)}</td>
                <td className="tabular px-2 py-2 text-right">
                  {s.systemPayments.length > 0 && <span className="mr-1 text-xs text-slate-500">{s.systemPayments.length} ×</span>}
                  {formatINR(s.system)}
                  {s.systemPayments.some((p) => p.amount) && (
                    <span className="block text-[11px] text-slate-500">
                      {s.systemPayments
                        .filter((p) => p.amount)
                        .map((p) => `#${p.consumerNo}${p.name ? ` ${p.name}` : ""} ${formatINR(p.amount)}`)
                        .join(", ")}
                    </span>
                  )}
                </td>
                <td className="tabular px-2 py-2 text-right text-rose-700">
                  {formatINR(spent(s))}
                  {s.expenses.some((e) => e.amount) && (
                    <span className="block text-[11px] text-slate-500">
                      {s.expenses
                        .filter((e) => e.amount)
                        .map((e) => `${e.name} ${formatINR(e.amount)}`)
                        .join(", ")}
                    </span>
                  )}
                </td>
                <td className="tabular px-4 py-2 text-right font-semibold">{formatINR(paise(s.cash - spent(s)))}</td>
              </tr>
            ))}
          </tbody>
          <tfoot className="border-t-2 border-slate-200 bg-slate-50 font-semibold">
            <tr>
              <td className="px-4 py-2">Total</td>
              <td className="px-2 py-2 font-normal text-slate-700">{total.sold.map(([n, q]) => `${n} × ${q}`).join(", ")}</td>
              <td className="tabular px-2 py-2 text-right">{formatINR(total.cash)}</td>
              <td className="tabular px-2 py-2 text-right">{formatINR(total.online)}</td>
              <td className="tabular px-2 py-2 text-right">
                {total.systemCount > 0 && <span className="mr-1 text-xs font-normal text-slate-500">{total.systemCount} ×</span>}
                {formatINR(total.system)}
              </td>
              <td className="tabular px-2 py-2 text-right text-rose-700">{formatINR(total.expenses)}</td>
              <td className="tabular px-4 py-2 text-right">{formatINR(total.handOver)}</td>
            </tr>
          </tfoot>
        </table>
      </div>
    </Card>
  );
}
