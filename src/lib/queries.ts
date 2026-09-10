import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { rpc } from "./rpc";
import type { AppSettings } from "./types";

export const queryKeys = {
  snapshot: ["snapshot"] as const,
  settings: ["settings"] as const,
  file: (path: string) => ["file", path] as const,
  dir: (path: string) => ["dir", path] as const,
  backups: (path: string) => ["backups", path] as const,
};

export function useSnapshot() {
  return useQuery({
    queryKey: queryKeys.snapshot,
    queryFn: () => rpc("scan_all", {}),
    staleTime: 5_000,
  });
}

export function useAppSettings() {
  return useQuery({
    queryKey: queryKeys.settings,
    queryFn: () => rpc("get_app_settings", {}),
    staleTime: Number.POSITIVE_INFINITY,
  });
}

export function useSaveAppSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (settings: AppSettings) => rpc("set_app_settings", { settings }),
    onSuccess: (saved) => {
      qc.setQueryData(queryKeys.settings, saved);
      void qc.invalidateQueries({ queryKey: queryKeys.snapshot });
    },
  });
}

export function useFileText(path: string | null) {
  return useQuery({
    queryKey: queryKeys.file(path ?? ""),
    queryFn: () => rpc("get_file_text", { path: path ?? "" }),
    enabled: path !== null,
  });
}

export function useDirListing(path: string | null) {
  return useQuery({
    queryKey: queryKeys.dir(path ?? ""),
    queryFn: () => rpc("list_dir", { path: path ?? "" }),
    enabled: path !== null,
  });
}
