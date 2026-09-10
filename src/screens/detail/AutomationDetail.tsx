import { InfoRow } from "@/components/FieldRow";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { AutomationForm, draftFromAutomation, patchFromDrafts } from "@/forms/AutomationForm";
import { basename } from "@/lib/entities";
import { useRequestPlan } from "@/lib/mutations";
import { rpc } from "@/lib/rpc";
import type { Automation } from "@/lib/types";

export function AutomationDetail({
  automation,
  onOpenFile,
}: {
  automation: Automation;
  onOpenFile: (path: string) => void;
}) {
  const requestPlan = useRequestPlan();
  const active = automation.status === "ACTIVE";
  const fmt = (ms: number | null) => (ms ? new Date(ms).toLocaleString() : "—");
  return (
    <div className="space-y-4">
      <div className="space-y-1.5">
        <InfoRow label="Status">
          <div className="flex items-center gap-2">
            <Switch
              checked={active}
              aria-label={active ? "Active" : "Paused"}
              onCheckedChange={(on) =>
                void requestPlan(() =>
                  rpc("preview_set_automation_status", {
                    id: automation.id,
                    status: on ? "ACTIVE" : "PAUSED",
                  }),
                )
              }
            />
            <Badge variant={active ? "success" : "muted"}>{automation.status}</Badge>
            <span className="text-xs text-muted-foreground">{automation.kind}</span>
          </div>
        </InfoRow>
        <InfoRow label="Target">
          {automation.target.type === "project" ? (
            <span className="font-mono text-xs">
              {automation.target.projectId}
              {automation.target.local && automation.cwds[0] ? ` (${basename(automation.cwds[0])})` : ""}
            </span>
          ) : automation.target.type === "projectless" ? (
            "projectless"
          ) : (
            <span className="font-mono text-xs">{JSON.stringify(automation.target.raw)}</span>
          )}
        </InfoRow>
        <InfoRow label="Timestamps">
          <span className="text-xs text-muted-foreground">
            created {fmt(automation.createdAt)} · updated {fmt(automation.updatedAt)}
          </span>
        </InfoRow>
        {(automation.memoryPath || automation.reportPaths.length > 0) && (
          <InfoRow label="Run files">
            <div className="flex flex-wrap gap-1">
              {automation.memoryPath && (
                <button
                  type="button"
                  className="rounded border px-1.5 py-0.5 font-mono text-xs hover:bg-accent"
                  onClick={() => onOpenFile(automation.memoryPath ?? "")}
                >
                  memory.md
                </button>
              )}
              {automation.reportPaths.map((r) => (
                <button
                  key={r}
                  type="button"
                  className="rounded border px-1.5 py-0.5 font-mono text-xs hover:bg-accent"
                  onClick={() => onOpenFile(r)}
                >
                  {basename(r)}
                </button>
              ))}
            </div>
          </InfoRow>
        )}
      </div>
      <AutomationForm
        automation={automation}
        onSave={(draft) => {
          const patch = patchFromDrafts(draftFromAutomation(automation), draft);
          return requestPlan(() => rpc("preview_save_automation", { id: automation.id, patch }));
        }}
      />
    </div>
  );
}
