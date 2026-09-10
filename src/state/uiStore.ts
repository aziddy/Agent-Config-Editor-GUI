import { create } from "zustand";
import type { EntityKind } from "@/lib/types";

export type AgentFilter = "all" | "claude" | "codex";
export type NavKind = EntityKind | "projects" | "settings";
export type DetailTab = "overview" | "raw" | "files";

interface UiState {
  agentFilter: AgentFilter;
  nav: NavKind;
  selectedId: string | null;
  search: string;
  detailTab: DetailTab;
  showDuplicates: boolean;
  /** Files reported changed on disk while possibly open in an editor. */
  externallyChanged: Record<string, number>;
  markExternallyChanged: (path: string) => void;
  clearExternallyChanged: (path: string) => void;
  setAgentFilter: (f: AgentFilter) => void;
  setNav: (k: NavKind) => void;
  select: (id: string | null) => void;
  setSearch: (s: string) => void;
  setDetailTab: (t: DetailTab) => void;
  toggleDuplicates: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  agentFilter: "all",
  nav: "skill",
  selectedId: null,
  search: "",
  detailTab: "overview",
  showDuplicates: false,
  externallyChanged: {},
  markExternallyChanged: (path) =>
    set((s) => ({ externallyChanged: { ...s.externallyChanged, [path]: Date.now() } })),
  clearExternallyChanged: (path) =>
    set((s) => {
      const next = { ...s.externallyChanged };
      delete next[path];
      return { externallyChanged: next };
    }),
  setAgentFilter: (agentFilter) => set({ agentFilter, selectedId: null }),
  setNav: (nav) => set({ nav, selectedId: null, search: "", detailTab: "overview" }),
  select: (selectedId) => set({ selectedId, detailTab: "overview" }),
  setSearch: (search) => set({ search }),
  setDetailTab: (detailTab) => set({ detailTab }),
  toggleDuplicates: () => set((s) => ({ showDuplicates: !s.showDuplicates })),
}));
