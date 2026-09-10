import { create } from "zustand";
import type { ApplyResult, WritePlan } from "@/lib/types";

interface PlanState {
  plan: WritePlan | null;
  resolver: ((result: ApplyResult | null) => void) | null;
  open: (plan: WritePlan) => Promise<ApplyResult | null>;
  finish: (result: ApplyResult | null) => void;
}

export const usePlanStore = create<PlanState>((set, get) => ({
  plan: null,
  resolver: null,
  open: (plan) =>
    new Promise<ApplyResult | null>((resolve) => {
      get().resolver?.(null);
      set({ plan, resolver: resolve });
    }),
  finish: (result) => {
    const { resolver } = get();
    set({ plan: null, resolver: null });
    resolver?.(result);
  },
}));
