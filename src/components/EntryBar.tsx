import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { ArrowDownLeft, ArrowUpRight, CheckCircle2, CornerDownLeft, Loader2, Plus, Sparkles } from "lucide-react";
import { call, mutate } from "@/lib/api";
import { formatINR } from "@/lib/format";
import type { ProductSuggestion as Suggestion, TransactionDTO, TransactionType } from "@/lib/types";

const CONSUMER_NO_DIGITS = 6;

const sameName = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

export function EntryBar() {
  const [type, setType] = useState<TransactionType>("CASH_IN");
  const [consumerNo, setConsumerNo] = useState("");
  const [isOther, setIsOther] = useState(false);
  const [product, setProduct] = useState("");
  const [selected, setSelected] = useState<Suggestion | null>(null);
  const [price, setPrice] = useState("");
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  // A ref, not just state: a rapid second Enter must not create a duplicate entry.
  const [pending, setPending] = useState(false);
  const inFlight = useRef(false);

  const consumerRef = useRef<HTMLInputElement>(null);
  const productRef = useRef<HTMLInputElement>(null);
  const priceRef = useRef<HTMLInputElement>(null);
  // Focus requested for after the next render (e.g. Consumer No is disabled until type resets to Cash In).
  const focusAfterRender = useRef<"consumer" | "product" | "price" | null>(null);

  const isCashIn = type === "CASH_IN";
  const consumerRequired = isCashIn && !isOther;
  const query = product.trim();

  useEffect(() => {
    const target = focusAfterRender.current;
    if (!target) return;
    focusAfterRender.current = null;
    ({ consumer: consumerRef, product: productRef, price: priceRef })[target].current?.focus();
  });

  // ---- Adaptive autocomplete ----
  // One shared promise per query text, so a Tab pressed before the debounced lookup
  // returns can await the same request instead of skipping autocomplete.
  const lookupRef = useRef<{ q: string; promise: Promise<Suggestion[]> } | null>(null);
  // Text for which the operator pressed Esc: Tab then keeps exactly what they typed.
  const dismissedRef = useRef<string | null>(null);

  const lookup = useCallback((q: string) => {
    if (lookupRef.current?.q === q) return lookupRef.current.promise;
    const promise = call<Suggestion[]>("search_products", { q }).catch(() => [] as Suggestion[]);
    lookupRef.current = { q, promise };
    return promise;
  }, []);

  useEffect(() => {
    if (!query) return; // cleared synchronously in the onChange handler
    let stale = false;
    const timer = setTimeout(async () => {
      const products = await lookup(query);
      if (stale) return;
      const exactIdx = products.findIndex((p) => sameName(p.name, query));
      setSuggestions(products);
      setActive(Math.max(0, exactIdx));
      // Only pop the dropdown if the operator is still in the product box.
      setOpen(document.activeElement === productRef.current && products.length > 0 && dismissedRef.current !== query);
      // Typed the full name without picking from the list -> still show its ghost price.
      const exact = products[exactIdx];
      if (exact) setSelected((cur) => (cur && sameName(cur.name, exact.name) ? cur : exact));
    }, 120);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [query, lookup]);

  const CONSUMER_HINT = "Enter the 6-digit consumer number, or press F3 for “Other product”.";

  /** Problem with the consumer number for the current entry, if any. */
  const consumerProblem = () => {
    if (!isCashIn) return null;
    if (!consumerNo) return isOther ? null : CONSUMER_HINT;
    return consumerNo.length === CONSUMER_NO_DIGITS ? null : `Consumer number must be exactly ${CONSUMER_NO_DIGITS} digits.`;
  };

  /** After the product: Consumer No when it's required, otherwise straight to Price. */
  const focusAfterProduct = () => (consumerRequired ? consumerRef : priceRef).current?.focus();

  const switchType = (next: TransactionType) => {
    setType(next);
    setError(null);
    // Consumer No is disabled for Cash Out; don't leave the cursor stranded in it.
    if (next === "CASH_OUT" && document.activeElement === consumerRef.current) focusAfterRender.current = "price";
  };

  const toggleOther = (next: boolean) => {
    setIsOther(next);
    setError(null);
    // Ticking it from an empty Consumer No box means "no number" -> go straight to the price.
    if (next && !consumerNo && document.activeElement === consumerRef.current) priceRef.current?.focus();
  };

  // ---- F2 toggles Cash In / Cash Out, F3 toggles "Other product" (Cash In only) ----
  const hotkeys = useRef({ switchType, toggleOther, isCashIn, isOther });
  useEffect(() => {
    hotkeys.current = { switchType, toggleOther, isCashIn, isOther };
  });
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      const h = hotkeys.current;
      if (e.key === "F2") {
        e.preventDefault();
        h.switchType(h.isCashIn ? "CASH_OUT" : "CASH_IN");
      } else if (e.key === "F3") {
        e.preventDefault();
        if (h.isCashIn) h.toggleOther(!h.isOther);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const choose = (s: Suggestion) => {
    setProduct(s.name);
    setSelected(s);
    setOpen(false);
    setError(null);
    focusAfterProduct();
  };

  const resetForNext = () => {
    lookupRef.current = null; // prices may have just changed
    focusAfterRender.current = "product"; // every entry starts at the product
    setConsumerNo("");
    setIsOther(false); // safe default: consumer number required again
    setProduct("");
    setPrice("");
    setSelected(null);
    setSuggestions([]);
    setOpen(false);
    setType("CASH_IN"); // every new entry starts from the default
  };

  const submit = async () => {
    if (inFlight.current) return;
    const name = product.trim();
    if (!name) {
      setError("Enter a product / item name.");
      productRef.current?.focus();
      return;
    }
    const problem = consumerProblem();
    if (problem) {
      setError(problem);
      consumerRef.current?.focus();
      return;
    }
    const typedPrice = price.trim();
    // Empty price = fast path: the backend resolves the product's lastUsedPrice.
    if (!typedPrice && !selected && !suggestions.some((s) => sameName(s.name, name))) {
      setError(`"${name}" is a new item — enter its price.`);
      priceRef.current?.focus();
      return;
    }
    setError(null);
    inFlight.current = true;
    setPending(true);
    try {
      const res = await mutate<TransactionDTO>("create_transaction", {
        type,
        productName: name,
        price: typedPrice || null,
        consumerNo: isCashIn ? consumerNo : null,
        isOtherProduct: isCashIn && isOther,
      });
      if (res.ok) {
        const t = res.data;
        const who = t.type === "CASH_OUT" ? "" : t.consumerNo ? ` · #${t.consumerNo}` : " · Other product";
        setFlash(`${t.type === "CASH_IN" ? "Cash In" : "Cash Out"}${who} · ${t.productName} · ${formatINR(t.price)}`);
        resetForNext();
      } else {
        setError(res.error);
        (/consumer/i.test(res.error) ? consumerRef : priceRef).current?.focus();
      }
    } finally {
      inFlight.current = false;
      setPending(false);
    }
  };

  useEffect(() => {
    if (!flash) return;
    const t = setTimeout(() => setFlash(null), 2500);
    return () => clearTimeout(t);
  }, [flash]);

  /** Tab/Enter while the list isn't showing (yet): wait for this text's lookup, then complete. */
  const completeFromLookup = async (q: string) => {
    setOpen(false);
    if (dismissedRef.current !== q) {
      const products = await lookup(q);
      if (productRef.current?.value.trim() !== q) return; // operator kept typing
      const match = products.find((p) => sameName(p.name, q)) ?? products[0];
      if (match) return choose(match);
    }
    focusAfterProduct();
  };

  // ---- Consumer No keys: Enter/Tab -> Price (only once it's valid), Esc -> back to Product ----
  const onConsumerKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if ((e.key === "Enter" || e.key === "Tab") && !e.shiftKey) {
      e.preventDefault();
      const problem = consumerProblem();
      if (problem) return setError(problem);
      priceRef.current?.focus();
    } else if (e.key === "Escape") {
      e.preventDefault();
      productRef.current?.focus();
    }
  };

  // ---- Product box keys: arrows navigate, Tab/Enter autocomplete -> Consumer No / Price ----
  const onProductKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    const hasList = open && suggestions.length > 0;
    switch (e.key) {
      case "ArrowDown":
        if (suggestions.length) {
          e.preventDefault();
          setOpen(true);
          setActive((i) => (hasList ? (i + 1) % suggestions.length : 0));
        }
        break;
      case "ArrowUp":
        if (hasList) {
          e.preventDefault();
          setActive((i) => (i - 1 + suggestions.length) % suggestions.length);
        }
        break;
      case "Tab":
      case "Enter":
        if (e.key === "Tab" && e.shiftKey) break;
        if (!query && e.key === "Tab") break;
        e.preventDefault();
        if (hasList) choose(suggestions[active]);
        else if (query) completeFromLookup(query);
        break;
      case "Escape":
        setOpen(false);
        dismissedRef.current = query;
        break;
    }
  };

  // ---- Price box keys: Enter submits (empty = ghost price), Esc goes back ----
  const onPriceKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      submit();
    } else if (e.key === "Escape") {
      // One step back: Consumer No when it's in play, else the product.
      e.preventDefault();
      const back = isCashIn ? consumerRef : productRef;
      back.current?.focus();
      back.current?.select();
    }
  };

  const ghost = selected ? selected.lastUsedPrice.toFixed(2) : "";
  const showNewItemHint = open && query && !suggestions.some((s) => sameName(s.name, query));

  return (
    <section
      className={`rounded-xl border-2 bg-white p-4 shadow-sm transition-colors ${
        isCashIn ? "border-emerald-500" : "border-rose-500"
      }`}
    >
      <div className="flex flex-wrap items-end gap-3">
        {/* Type toggle */}
        <div>
          <span className="mb-1 block text-xs font-semibold uppercase tracking-wide text-slate-500">
            Type <kbd className="ml-1 rounded border border-slate-300 bg-slate-50 px-1 font-mono text-[10px]">F2</kbd>
          </span>
          <div className="flex overflow-hidden rounded-lg border border-slate-300" role="radiogroup" aria-label="Transaction type">
            <button
              type="button"
              tabIndex={-1}
              role="radio"
              aria-checked={isCashIn}
              onClick={() => switchType("CASH_IN")}
              className={`flex items-center gap-1.5 px-4 py-2.5 text-sm font-bold ${
                isCashIn ? "bg-emerald-600 text-white" : "bg-white text-slate-500 hover:bg-slate-50"
              }`}
            >
              <ArrowDownLeft className="h-4 w-4" /> Cash In
            </button>
            <button
              type="button"
              tabIndex={-1}
              role="radio"
              aria-checked={!isCashIn}
              onClick={() => switchType("CASH_OUT")}
              className={`flex items-center gap-1.5 px-4 py-2.5 text-sm font-bold ${
                !isCashIn ? "bg-rose-600 text-white" : "bg-white text-slate-500 hover:bg-slate-50"
              }`}
            >
              <ArrowUpRight className="h-4 w-4" /> Cash Out
            </button>
          </div>
        </div>

        {/* Product with autocomplete */}
        <div className="relative min-w-64 flex-1">
          <label htmlFor="entry-product" className="mb-1 block text-xs font-semibold uppercase tracking-wide text-slate-500">
            Product / Item
          </label>
          <input
            id="entry-product"
            ref={productRef}
            autoFocus
            autoComplete="off"
            spellCheck={false}
            role="combobox"
            aria-expanded={open}
            aria-controls="entry-product-list"
            aria-activedescendant={open ? `entry-opt-${active}` : undefined}
            value={product}
            onChange={(e) => {
              const v = e.target.value;
              setProduct(v);
              setError(null);
              if (selected && !sameName(selected.name, v)) setSelected(null);
              dismissedRef.current = null;
              if (!v.trim()) {
                setSuggestions([]);
                setOpen(false);
              }
            }}
            onKeyDown={onProductKeyDown}
            onFocus={() => suggestions.length && setOpen(true)}
            onBlur={() => setOpen(false)}
            placeholder="Start typing… (Tab to autocomplete)"
            className="w-full rounded-lg border border-slate-300 px-3 py-2.5 text-lg outline-none focus:border-hp-blue focus:ring-2 focus:ring-hp-blue/20"
          />
          {open && suggestions.length > 0 && (
            <ul
              id="entry-product-list"
              role="listbox"
              className="absolute z-20 mt-1 max-h-72 w-full overflow-auto rounded-lg border border-slate-200 bg-white py-1 shadow-lg"
            >
              {suggestions.map((s, i) => (
                <li
                  id={`entry-opt-${i}`}
                  key={s.name}
                  role="option"
                  aria-selected={i === active}
                  onMouseDown={(e) => {
                    e.preventDefault(); // keep focus; choose() moves it to price
                    choose(s);
                  }}
                  onMouseEnter={() => setActive(i)}
                  className={`flex cursor-pointer items-center justify-between px-3 py-2 ${
                    i === active ? "bg-blue-50 text-hp-blue" : "text-slate-800"
                  }`}
                >
                  <span className="font-medium">{s.name}</span>
                  <span className="tabular text-sm text-slate-500">{formatINR(s.lastUsedPrice)}</span>
                </li>
              ))}
              {showNewItemHint && (
                <li className="flex items-center gap-2 border-t border-slate-100 px-3 py-2 text-sm text-slate-500">
                  <Plus className="h-3.5 w-3.5" /> Press <kbd className="rounded border px-1 font-mono text-xs">Esc</kbd> to add{" "}
                  <b className="text-slate-700">&ldquo;{query}&rdquo;</b> as a new item instead
                </li>
              )}
            </ul>
          )}
        </div>

        {/* Consumer No (Cash In only, exactly 6 digits) */}
        <div className="w-36">
          <label htmlFor="entry-consumer" className="mb-1 block whitespace-nowrap text-xs font-semibold uppercase tracking-wide text-slate-500">
            Consumer No <span className={`text-red-600 ${consumerRequired ? "" : "invisible"}`}>*</span>
          </label>
          <input
            id="entry-consumer"
            ref={consumerRef}
            inputMode="numeric"
            autoComplete="off"
            maxLength={CONSUMER_NO_DIGITS}
            disabled={!isCashIn}
            aria-required={consumerRequired}
            value={isCashIn ? consumerNo : ""}
            onChange={(e) => {
              setConsumerNo(e.target.value.replace(/\D/g, "").slice(0, CONSUMER_NO_DIGITS));
              setError(null);
            }}
            onKeyDown={onConsumerKeyDown}
            onFocus={(e) => e.target.select()}
            placeholder={!isCashIn ? "N/A" : isOther ? "Optional" : "6 digits"}
            className={`tabular w-full rounded-lg border px-3 py-2.5 text-center text-lg tracking-widest outline-none focus:border-hp-blue focus:ring-2 focus:ring-hp-blue/20 disabled:bg-slate-50 placeholder:tracking-normal ${
              isCashIn && isOther ? "border-dashed border-slate-300" : "border-slate-300"
            }`}
          />
        </div>

        {/* Other product: Cash In without a consumer number. Fixed size so it never moves under the cursor. */}
        <div className="flex h-[50px] items-center">
          <label
            // Keep focus in whichever box the operator was typing in; the click only toggles.
            onMouseDown={(e) => e.preventDefault()}
            title="Other product: Cash In without a consumer number (F3)"
            className={`flex select-none items-center gap-1.5 whitespace-nowrap rounded-md border px-1.5 py-0.5 text-xs font-medium ${
              !isCashIn
                ? "cursor-not-allowed border-slate-200 text-slate-300"
                : isOther
                  ? "cursor-pointer border-hp-blue bg-blue-50 text-hp-blue"
                  : "cursor-pointer border-slate-300 text-slate-600 hover:bg-slate-50"
            }`}
          >
            <input
              type="checkbox"
              tabIndex={-1}
              checked={isCashIn && isOther}
              disabled={!isCashIn}
              onChange={(e) => toggleOther(e.target.checked)}
              className="h-3.5 w-3.5 cursor-pointer accent-hp-blue"
            />
            Other product
            <kbd className="font-mono text-[10px] text-slate-400">F3</kbd>
          </label>
        </div>

        {/* Price with ghost default */}
        <div className="w-44">
          <label htmlFor="entry-price" className="mb-1 block text-xs font-semibold uppercase tracking-wide text-slate-500">
            Price (₹)
          </label>
          <input
            id="entry-price"
            ref={priceRef}
            inputMode="decimal"
            autoComplete="off"
            value={price}
            onChange={(e) => {
              const v = e.target.value.replace(/[^\d.]/g, "");
              if (/^\d*\.?\d{0,2}$/.test(v)) setPrice(v);
              setError(null);
            }}
            onKeyDown={onPriceKeyDown}
            onFocus={(e) => e.target.select()}
            placeholder={ghost || "0.00"}
            className={`tabular w-full rounded-lg border border-slate-300 px-3 py-2.5 text-right text-lg outline-none focus:border-hp-blue focus:ring-2 focus:ring-hp-blue/20 ${
              ghost ? "placeholder:text-slate-400 placeholder:italic" : "placeholder:text-slate-300"
            }`}
          />
        </div>

        <button
          type="button"
          onClick={submit}
          disabled={pending}
          className={`flex h-[50px] items-center gap-2 rounded-lg px-6 text-base font-bold text-white shadow-sm disabled:opacity-60 ${
            isCashIn ? "bg-emerald-600 hover:bg-emerald-700" : "bg-rose-600 hover:bg-rose-700"
          }`}
        >
          {pending ? <Loader2 className="h-5 w-5 animate-spin" /> : <CornerDownLeft className="h-5 w-5" />}
          Done
        </button>
      </div>

      <div className="mt-2 flex min-h-5 flex-wrap items-center justify-between gap-2 text-sm">
        <div>
          {error ? (
            <span role="alert" className="font-medium text-red-600">
              {error}
            </span>
          ) : flash ? (
            <span className="flex items-center gap-1.5 font-medium text-emerald-700">
              <CheckCircle2 className="h-4 w-4" /> Saved: {flash}
            </span>
          ) : selected && !price ? (
            <span className="flex items-center gap-1.5 text-slate-500">
              <Sparkles className="h-4 w-4 text-amber-500" /> Press <kbd className="rounded border px-1 font-mono text-xs">Enter</kbd> to use
              last price {formatINR(selected.lastUsedPrice)}, or type a new price to update it.
            </span>
          ) : query && !open && !selected && !suggestions.some((s) => sameName(s.name, query)) ? (
            <span className="flex items-center gap-1.5 text-slate-500">
              <Plus className="h-4 w-4 text-hp-blue" /> New item — it will be added to the catalog when saved.
            </span>
          ) : null}
        </div>
        <p className="text-xs text-slate-400">
          <kbd className="font-mono">Tab</kbd> autocomplete · <kbd className="font-mono">↑↓</kbd> choose ·{" "}
          <kbd className="font-mono">Enter</kbd> next / save · <kbd className="font-mono">Esc</kbd> keep typed name / back
        </p>
      </div>
    </section>
  );
}
