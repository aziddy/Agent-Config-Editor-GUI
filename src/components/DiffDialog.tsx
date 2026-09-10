import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { useApplyPlan } from "@/lib/mutations";
import { usePlanStore } from "@/state/planStore";
import { DiffView } from "./DiffView";

/** Mounted once at the app root; shows whatever plan is pending in the plan store. */
export function DiffDialog() {
  const plan = usePlanStore((s) => s.plan);
  const finish = usePlanStore((s) => s.finish);
  const apply = useApplyPlan();
  const noChange = plan?.warnings.includes("no changes") ?? false;

  return (
    <Dialog
      open={plan !== null}
      onOpenChange={(open) => {
        if (!open && !apply.isPending) finish(null);
      }}
      title={plan?.description ?? ""}
      description={plan ? plan.path : undefined}
      className="max-w-3xl"
    >
      {plan && (
        <div className="flex max-h-[70vh] flex-col gap-3">
          {plan.warnings.length > 0 && (
            <ul className="space-y-1 rounded-md border border-amber-500/40 bg-amber-500/10 p-2 text-xs">
              {plan.warnings.map((w) => (
                <li key={w} className="flex items-start gap-2">
                  <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-600" />
                  <span>{w}</span>
                </li>
              ))}
            </ul>
          )}
          {plan.extraFiles.length > 0 && (
            <p className="text-xs text-muted-foreground">
              Also creates: {plan.extraFiles.map((f) => f.path).join(", ")}
            </p>
          )}
          {plan.newText === null ? (
            <p className="text-sm">
              This will delete <span className="font-mono">{plan.path}</span>. A backup is kept first.
            </p>
          ) : (
            <DiffView diff={plan.unifiedDiff} className="min-h-0 flex-1" />
          )}
          <div className="flex justify-end gap-2 pb-2">
            <Button variant="outline" onClick={() => finish(null)} disabled={apply.isPending}>
              Cancel
            </Button>
            <Button
              variant={plan.newText === null ? "destructive" : "default"}
              disabled={apply.isPending || noChange}
              onClick={() => apply.mutate(plan, { onSuccess: (r) => finish(r), onError: () => finish(null) })}
            >
              {apply.isPending ? "Applying…" : plan.newText === null ? "Delete" : "Apply"}
            </Button>
          </div>
        </div>
      )}
    </Dialog>
  );
}
