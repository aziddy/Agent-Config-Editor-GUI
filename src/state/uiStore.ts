import { create } from "zustand";
import type { LocationFilter } from "@/lib/entities";
import type { EntityKind } from "@/lib/types";

export type AgentFilter = "all" | "claude" | "codex";
export type NavKind = EntityKind | "projects" | "settings";
export type DetailTab = "overview" | "runs" | "editor";
export type NavigationGuard = () => boolean | Promise<boolean>;

interface UiState {
  agentFilter: AgentFilter;
  locationFilter: LocationFilter;
  nav: NavKind;
  selectedId: string | null;
  search: string;
  detailTab: DetailTab;
  showDuplicates: boolean;
  sidebarCollapsed: boolean;
  entityListCollapsed: boolean;
  editorTreeWidth: number;
  editorTreeCollapsed: boolean;
  navigationPending: boolean;
  navigationGuard: NavigationGuard | null;
  setNavigationGuard: (guard: NavigationGuard | null) => void;
  /** Commit a navigation only after the current editor permits leaving its draft. */
  requestNavigation: (commit: () => void) => Promise<boolean>;
  /** Files reported changed on disk while possibly open in an editor. */
  externallyChanged: Record<string, number>;
  markExternallyChanged: (path: string) => void;
  clearExternallyChanged: (path: string) => void;
  setAgentFilter: (f: AgentFilter) => Promise<boolean>;
  setLocationFilter: (f: LocationFilter) => Promise<boolean>;
  setNav: (k: NavKind) => Promise<boolean>;
  select: (id: string | null) => Promise<boolean>;
  navigateToEntity: (nav: EntityKind, selectedId: string) => Promise<boolean>;
  setSearch: (s: string) => void;
  setDetailTab: (t: DetailTab) => void;
  toggleDuplicates: () => void;
  toggleSidebar: () => void;
  toggleEntityList: () => void;
  setEditorTreeWidth: (width: number) => void;
  toggleEditorTree: () => void;
}

function sameLocation(a: LocationFilter, b: LocationFilter): boolean {
  if (a.type !== b.type) return false;
  if (a.type === "project" && b.type === "project") return a.root === b.root;
  if (a.type === "scope" && b.type === "scope") return a.scope === b.scope;
  return true;
}

export const useUiStore = create<UiState>((set, get) => ({
  agentFilter: "all",
  locationFilter: { type: "all" },
  nav: "skill",
  selectedId: null,
  search: "",
  detailTab: "overview",
  showDuplicates: false,
  sidebarCollapsed: false,
  entityListCollapsed: false,
  editorTreeWidth: 220,
  editorTreeCollapsed: false,
  navigationPending: false,
  navigationGuard: null,
  setNavigationGuard: (navigationGuard) => set({ navigationGuard }),
  requestNavigation: async (commit) => {
    const { navigationGuard, navigationPending } = get();
    if (navigationPending) return false;
    if (!navigationGuard) {
      commit();
      return true;
    }
    set({ navigationPending: true });
    try {
      if (!(await navigationGuard())) return false;
      commit();
      return true;
    } catch {
      // Failed or cancelled saves keep the current selection and draft intact.
      return false;
    } finally {
      set({ navigationPending: false });
    }
  },
  externallyChanged: {},
  markExternallyChanged: (path) =>
    set((s) => ({ externallyChanged: { ...s.externallyChanged, [path]: Date.now() } })),
  clearExternallyChanged: (path) =>
    set((s) => {
      const next = { ...s.externallyChanged };
      delete next[path];
      return { externallyChanged: next };
    }),
  setAgentFilter: (agentFilter) => {
    if (agentFilter === get().agentFilter) return Promise.resolve(true);
    return get().requestNavigation(() => set({ agentFilter, selectedId: null, detailTab: "overview" }));
  },
  setLocationFilter: (locationFilter) => {
    if (sameLocation(locationFilter, get().locationFilter)) return Promise.resolve(true);
    return get().requestNavigation(() => set({ locationFilter, selectedId: null, detailTab: "overview" }));
  },
  setNav: (nav) => {
    if (nav === get().nav) return Promise.resolve(true);
    return get().requestNavigation(() =>
      set((s) => ({
        nav,
        selectedId: null,
        search: "",
        detailTab: "overview",
        entityListCollapsed: nav !== "projects" && nav !== "settings" ? false : s.entityListCollapsed,
      })),
    );
  },
  select: (selectedId) => {
    if (selectedId === get().selectedId) return Promise.resolve(true);
    return get().requestNavigation(() => set({ selectedId, detailTab: "overview" }));
  },
  navigateToEntity: (nav, selectedId) => {
    if (nav === get().nav && selectedId === get().selectedId) return Promise.resolve(true);
    return get().requestNavigation(() =>
      set((s) => ({
        nav,
        selectedId,
        search: "",
        detailTab: "overview",
        entityListCollapsed: nav !== s.nav ? false : s.entityListCollapsed,
      })),
    );
  },
  setSearch: (search) => set({ search }),
  setDetailTab: (detailTab) => set({ detailTab }),
  toggleDuplicates: () => set((s) => ({ showDuplicates: !s.showDuplicates })),
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
  toggleEntityList: () => set((s) => ({ entityListCollapsed: !s.entityListCollapsed })),
  setEditorTreeWidth: (width) => {
    if (Number.isFinite(width)) set({ editorTreeWidth: Math.max(140, Math.min(360, width)) });
  },
  toggleEditorTree: () => set((s) => ({ editorTreeCollapsed: !s.editorTreeCollapsed })),
}));
