import { AlertCircle, Copy, KeyRound } from "lucide-react";
import type { ReactNode } from "react";
import { AgentBadge } from "@/components/AgentBadge";
import { EnabledSwitch } from "@/components/EnabledSwitch";
import { ScopeBadge } from "@/components/ScopeBadge";
import { type AnyEntity, entityName, entityScope } from "@/lib/entities";
import type { EnabledState } from "@/lib/types";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/state/uiStore";

export interface Row {
  entity: AnyEntity;
  duplicateCount: number;
  subtitle?: ReactNode;
  enabled?: EnabledState;
  onToggle?: (enabled: boolean) => void;
  needsAuth?: boolean;
  warning?: string;
}

export function EntityList({ rows, emptyText }: { rows: Row[]; emptyText: string }) {
  const selectedId = useUiStore((s) => s.selectedId);
  const select = useUiStore((s) => s.select);
  if (rows.length === 0) {
    return <p className="p-6 text-center text-sm text-muted-foreground">{emptyText}</p>;
  }
  return (
    <ul className="divide-y">
      {rows.map((row) => {
        const id = row.entity.data.id;
        const active = id === selectedId;
        return (
          <li
            key={id}
            className={cn(
              "flex items-start gap-2 px-3 py-2 transition-colors",
              active ? "bg-accent" : "hover:bg-accent/50",
            )}
          >
            <button type="button" onClick={() => select(id)} className="min-w-0 flex-1 text-left">
              <div className="flex items-center gap-1.5">
                <span className="truncate text-sm font-medium">{entityName(row.entity)}</span>
                {row.duplicateCount > 0 && (
                  <span
                    className="inline-flex items-center gap-0.5 rounded bg-muted px-1 text-[10px] text-muted-foreground"
                    title={`Identical copy in ${row.duplicateCount} other project${row.duplicateCount === 1 ? "" : "s"}`}
                  >
                    <Copy className="h-2.5 w-2.5" />+{row.duplicateCount}
                  </span>
                )}
                {row.needsAuth && (
                  <KeyRound className="h-3 w-3 text-amber-600" aria-label="Needs authentication" />
                )}
                {row.warning && <AlertCircle className="h-3 w-3 text-destructive" aria-label={row.warning} />}
              </div>
              {row.subtitle && <div className="truncate text-xs text-muted-foreground">{row.subtitle}</div>}
              <div className="mt-1 flex flex-wrap items-center gap-1">
                <AgentBadge agent={row.entity.data.agent} />
                <ScopeBadge scope={entityScope(row.entity)} />
              </div>
            </button>
            {row.enabled && (
              <div className="pt-0.5">
                <EnabledSwitch state={row.enabled} onChange={row.onToggle} />
              </div>
            )}
          </li>
        );
      })}
    </ul>
  );
}
