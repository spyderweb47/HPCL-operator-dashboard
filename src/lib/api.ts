import { invoke } from "@tauri-apps/api/core";
import type { ActionResult } from "./types";

/**
 * The whole backend lives in Rust (src-tauri). Every call goes through one `call` command,
 * so the same dispatcher also serves `npm run bridge` for testing the UI in a plain browser.
 */
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function call<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (inTauri) return invoke<T>("call", { cmd, args });
  const res = await fetch("/bridge/call", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ cmd, args }),
  });
  const body = (await res.json()) as { ok: boolean; data?: T; error?: string };
  if (!body.ok) throw body.error ?? "Unknown error";
  return body.data as T;
}

const listeners = new Set<() => void>();
let dataVersion = 0;

export const dataStore = {
  subscribe(l: () => void) {
    listeners.add(l);
    return () => listeners.delete(l);
  },
  version: () => dataVersion,
};

/** Tell every open view that local data changed, so it reloads. */
export function notifyDataChanged() {
  dataVersion++;
  listeners.forEach((l) => l());
}

/** A write: errors become `{ ok: false }` with the backend's message, success refreshes every view. */
export async function mutate<T = undefined>(cmd: string, args: Record<string, unknown> = {}): Promise<ActionResult<T>> {
  try {
    const data = await call<T>(cmd, args);
    notifyDataChanged();
    return { ok: true, data };
  } catch (err) {
    return { ok: false, error: typeof err === "string" ? err : "Something went wrong. Please try again." };
  }
}
