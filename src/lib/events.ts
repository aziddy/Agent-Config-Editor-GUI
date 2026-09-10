import { useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { useUiStore } from "@/state/uiStore";
import { queryKeys } from "./queries";

interface ConfigChanged {
  paths: string[];
}

/** Refresh queries when the backend reports that config files changed on disk. */
export function useConfigChangedListener() {
  const qc = useQueryClient();
  const markExternallyChanged = useUiStore((s) => s.markExternallyChanged);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen<ConfigChanged>("config-changed", (event) => {
      const paths = event.payload.paths;
      void qc.invalidateQueries({ queryKey: queryKeys.snapshot });
      for (const p of paths) {
        void qc.invalidateQueries({ queryKey: queryKeys.file(p) });
        markExternallyChanged(p);
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [qc, markExternallyChanged]);
}
