import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { usePlanStore } from "@/state/planStore";
import { describeError } from "./errors";
import { queryKeys } from "./queries";
import { rpc } from "./rpc";
import type { WritePlan } from "./types";

/**
 * Ask the backend for a WritePlan and hand it to the DiffDialog. Resolves with the
 * apply result when the user confirms, or `null` when they cancel.
 */
export function useRequestPlan() {
  const open = usePlanStore((s) => s.open);
  return async (build: () => Promise<WritePlan>) => {
    try {
      const plan = await build();
      return await open(plan);
    } catch (err) {
      toast.error(describeError(err));
      return null;
    }
  };
}

export function useApplyPlan() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (plan: WritePlan) => rpc("apply_write_plan", { plan }),
    onSuccess: (result, plan) => {
      void qc.invalidateQueries({ queryKey: queryKeys.snapshot });
      void qc.invalidateQueries({ queryKey: queryKeys.file(plan.path) });
      for (const extra of plan.extraFiles) {
        void qc.invalidateQueries({ queryKey: queryKeys.file(extra.path) });
      }
      void qc.invalidateQueries({ queryKey: ["dir"] });
      void qc.invalidateQueries({ queryKey: queryKeys.backups(plan.path) });
      toast.success(plan.description, {
        description: result.backupPath ? "Backup saved" : undefined,
      });
    },
    onError: (err) => toast.error(describeError(err)),
  });
}
