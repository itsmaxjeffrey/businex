import { useCallback, useEffect, useState } from "react";
import { api } from "./api";

/** Minimal data-fetch hook with reload support. */
export function useData<T>(path: string | null) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => {
    if (!path) { setLoading(false); return; }
    setLoading(true);
    api.get<T>(path)
      .then((d) => { setData(d); setError(null); })
      .catch((e) => setError(e?.message ?? "Failed to load"))
      .finally(() => setLoading(false));
  }, [path]);

  useEffect(() => { reload(); }, [reload]);

  return { data, loading, error, reload };
}
