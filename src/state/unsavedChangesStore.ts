import { create } from "zustand";
import type { UnsavedChoice } from "@/lib/editorSession";

interface UnsavedChangesState {
  path: string | null;
  resolver: ((choice: UnsavedChoice) => void) | null;
  confirm: (path: string) => Promise<UnsavedChoice>;
  finish: (choice: UnsavedChoice) => void;
}

export const useUnsavedChangesStore = create<UnsavedChangesState>((set, get) => ({
  path: null,
  resolver: null,
  confirm: (path) =>
    new Promise((resolve) => {
      get().resolver?.("cancel");
      set({ path, resolver: resolve });
    }),
  finish: (choice) => {
    const resolver = get().resolver;
    set({ path: null, resolver: null });
    resolver?.(choice);
  },
}));
