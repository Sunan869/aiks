import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { getSourceDescriptors } from "./api/provider-catalog";
import type { SourceDescriptor } from "./api/provider-catalog-model";
import { formatSourceName } from "./source-display";

interface CatalogState {
  sources: SourceDescriptor[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}
const Context = createContext<CatalogState>({ sources: [], loading: true, error: null, refresh: async () => {} });

export function ProviderCatalogProvider({ children }: { children: ReactNode }) {
  const [sources, setSources] = useState<SourceDescriptor[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    setLoading(true);
    try { setSources(await getSourceDescriptors()); setError(null); }
    catch (e) { setError(String(e)); }
    finally { setLoading(false); }
  }, []);
  useEffect(() => {
    void refresh();
    // The engine may still be starting at initial mount; retry on actual startup.
    let unlisten: (() => void) | undefined;
    let disposed = false;
    import("@tauri-apps/api/event").then(({ listen }) => listen<{ step: string }>("startup-progress", e => {
      if (e.payload.step === "ready") void refresh();
    })).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(() => {});
    return () => { disposed = true; unlisten?.(); };
  }, [refresh]);
  return <Context.Provider value={{ sources, loading, error, refresh }}>{children}</Context.Provider>;
}

export function useSourceCatalog(): CatalogState { return useContext(Context); }
export function useSourceName(): (source: string | null | undefined) => string {
  const { sources } = useSourceCatalog();
  return source => sources.find(s => s.key === source)?.display_name ?? formatSourceName(source);
}
