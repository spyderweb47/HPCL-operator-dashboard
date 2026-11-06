import { useId, useRef, useState, type KeyboardEvent } from "react";
import { Plus } from "lucide-react";

const LIMIT = 8;
const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

/** Names starting with the text first, then names containing it; list order kept (most used first). */
function match(all: string[], text: string) {
  const q = text.trim().toLowerCase();
  if (!q) return [];
  const starts = all.filter((n) => n.toLowerCase().startsWith(q));
  const contains = all.filter((n) => !n.toLowerCase().startsWith(q) && n.toLowerCase().includes(q));
  return [...starts, ...contains].slice(0, LIMIT);
}

type Props = {
  value: string;
  onChange: (v: string) => void;
  /** Known names, most used first. */
  suggestions: string[];
  placeholder?: string;
  ariaLabel: string;
  className: string;
  /** Marks the input for "focus the row just added". */
  dataKey?: string;
  maxLength?: number;
};

/**
 * Like the register's product box: type, ↑↓ to choose, Tab/Enter fills in the highlighted name
 * (then the cursor moves on), Esc keeps exactly what was typed (a new name, remembered once saved).
 */
export function AutoInput({ value, onChange, suggestions, placeholder, ariaLabel, className, dataKey, maxLength = 60 }: Props) {
  const listId = useId();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  // Text for which Esc was pressed: Tab then keeps it as typed.
  const dismissed = useRef<string | null>(null);

  const items = match(suggestions, value);
  const exact = items.findIndex((n) => same(n, value));
  const showList = open && items.length > 0 && dismissed.current !== value;
  const isNew = value.trim() !== "" && !suggestions.some((n) => same(n, value));

  const choose = (name: string) => {
    onChange(name);
    setOpen(false);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    switch (e.key) {
      case "ArrowDown":
        if (items.length) {
          e.preventDefault();
          setOpen(true);
          dismissed.current = null;
          setActive((i) => (showList ? (i + 1) % items.length : Math.max(0, exact)));
        }
        break;
      case "ArrowUp":
        if (showList) {
          e.preventDefault();
          setActive((i) => (i - 1 + items.length) % items.length);
        }
        break;
      case "Tab":
      case "Enter":
        // Fill in the highlighted name; the cursor still moves on (Tab natively, Enter via the card).
        if (e.shiftKey || !showList) break;
        choose(items[Math.min(active, items.length - 1)]);
        break;
      case "Escape":
        if (showList) {
          e.preventDefault();
          e.stopPropagation();
          dismissed.current = value;
          setOpen(false);
        }
        break;
    }
  };

  return (
    <div className="relative min-w-0 flex-1">
      <input
        data-auto={dataKey}
        role="combobox"
        aria-label={ariaLabel}
        aria-expanded={showList}
        aria-controls={listId}
        aria-activedescendant={showList ? `${listId}-${active}` : undefined}
        autoComplete="off"
        spellCheck={false}
        maxLength={maxLength}
        value={value}
        placeholder={placeholder}
        onChange={(e) => {
          const v = e.target.value;
          onChange(v);
          dismissed.current = null;
          setOpen(true);
          const list = match(suggestions, v);
          setActive(Math.max(0, list.findIndex((n) => same(n, v))));
        }}
        onKeyDown={onKeyDown}
        onBlur={() => setOpen(false)}
        className={`w-full ${className}`}
      />
      {showList && (
        <ul id={listId} role="listbox" className="absolute z-30 mt-1 max-h-64 w-full min-w-48 overflow-auto rounded-lg border border-slate-200 bg-white py-1 text-sm shadow-lg">
          {items.map((name, i) => (
            <li
              id={`${listId}-${i}`}
              key={name}
              role="option"
              aria-selected={i === active}
              onMouseDown={(e) => {
                e.preventDefault(); // keep the cursor in the box
                choose(name);
              }}
              onMouseEnter={() => setActive(i)}
              className={`cursor-pointer px-3 py-1.5 ${i === active ? "bg-blue-50 text-hp-blue" : "text-slate-800"}`}
            >
              {name}
            </li>
          ))}
          {isNew && (
            <li className="flex items-center gap-1.5 border-t border-slate-100 px-3 py-1.5 text-xs text-slate-500">
              <Plus className="h-3 w-3" /> <kbd className="rounded border px-1 font-mono">Esc</kbd> to add &ldquo;{value.trim()}&rdquo; as new
            </li>
          )}
        </ul>
      )}
    </div>
  );
}
