import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import { call, dataStore } from "./api";

type QueryState<T> = { data: T | null; error: string | null; key: string | null };

/**
 * Loads `cmd(args)` and reloads it whenever any write happens (notifyDataChanged) or every
 * `refreshMs`. Previous data stays on screen while reloading, so the register never flickers.
 */
export function useQuery<T>(cmd: string, args: Record<string, unknown> = {}, refreshMs?: number) {
  const key = JSON.stringify([cmd, args]);
  const version = useSyncExternalStore(dataStore.subscribe, dataStore.version);
  const [tick, setTick] = useState(0);
  const [state, setState] = useState<QueryState<T>>({ data: null, error: null, key: null });

  useEffect(() => {
    let stale = false;
    const [c, a] = JSON.parse(key) as [string, Record<string, unknown>];
    call<T>(c, a).then(
      (data) => !stale && setState({ data, error: null, key }),
      (err) => !stale && setState((s) => ({ ...s, error: typeof err === "string" ? err : "Could not load data.", key })),
    );
    return () => {
      stale = true;
    };
  }, [key, version, tick]);

  useEffect(() => {
    if (!refreshMs) return;
    const t = setInterval(() => setTick((n) => n + 1), refreshMs);
    return () => clearInterval(t);
  }, [refreshMs]);

  const reload = useCallback(() => setTick((n) => n + 1), []);
  // Data from a different query (e.g. the previous history date) is not shown as this one's.
  const current = state.key === key;
  return { data: current ? state.data : null, error: current ? state.error : null, loading: !current, reload };
}

