import { Plus } from "lucide-react";
import { useState } from "react";
import { DetailPane } from "@/components/DetailPane";
import { EntityList, type Row } from "@/components/EntityList";
import { SkillFilterBar } from "@/components/SkillFilterBar";
import { TopBar } from "@/components/TopBar";
import { Button } from "@/components/ui/button";
import {
  type AnyEntity,
  basename,
  KIND_LABELS,
  locationFilterLabel,
  MEMORY_SOURCE_LABELS,
  memorySourceDescription,
} from "@/lib/entities";
import { useRequestPlan } from "@/lib/mutations";
import { useSnapshot } from "@/lib/queries";
import { rpc } from "@/lib/rpc";
import type { EntityKind } from "@/lib/types";
import { useFilteredEntities } from "@/lib/useEntities";
import { NewAutomationDialog, NewEntityDialog, NewMcpDialog } from "@/screens/NewDialogs";
import { useUiStore } from "@/state/uiStore";

function useRowBuilder() {
  const requestPlan = useRequestPlan();
  return (entity: AnyEntity, duplicateCount: number): Row => {
    const base: Row = { entity, duplicateCount };
    switch (entity.kind) {
      case "skill": {
        const s = entity.data;
        return {
          ...base,
          subtitle: s.description ?? s.dir,
          enabled: s.enabled,
          warning:
            s.allowImplicitInvocation === null
              ? "Invocation policy could not be read; see scan warnings"
              : undefined,
          onToggle: (enabled) =>
            void requestPlan(() => rpc("preview_set_skill_enabled", { skillId: s.id, enabled })),
        };
      }
      case "mcpServer": {
        const m = entity.data;
        const perProject = m.agent === "claude" && (m.scope.type === "user" || m.scope.type === "plugin");
        const disabledIn = Array.isArray(m.extra.disabledInProjects)
          ? (m.extra.disabledInProjects as string[])
          : [];
        return {
          ...base,
          subtitle:
            m.transport.type === "stdio"
              ? `${m.transport.command} ${m.transport.args.join(" ")}`.trim()
              : m.transport.url,
          enabled: perProject
            ? {
                enabled: true,
                editable: false,
                toggle: null,
                reason:
                  disabledIn.length > 0
                    ? `Disabled in ${disabledIn.map(basename).join(", ")}; toggle per project in the details`
                    : "Toggle per project in the details",
              }
            : m.enabled,
          onToggle: (enabled) =>
            void requestPlan(() =>
              rpc("preview_set_mcp_enabled", {
                target: { agent: m.agent, scope: m.scope, file: m.origin.file, name: m.name },
                enabled,
                projectRoot: null,
              }),
            ),
          needsAuth: m.needsAuth,
          warning: disabledIn.length > 0 ? `Disabled in ${disabledIn.length} project(s)` : undefined,
        };
      }
      case "plugin": {
        const p = entity.data;
        return {
          ...base,
          subtitle: p.description ?? p.marketplace,
          enabled: p.enabled,
          onToggle: (enabled) =>
            void requestPlan(() =>
              rpc("preview_set_plugin_enabled", { agent: p.agent, pluginId: p.id, scope: p.scope, enabled }),
            ),
          warning: p.unresolvedMarketplace
            ? "Unknown marketplace"
            : !p.installed
              ? "Not installed"
              : undefined,
        };
      }
      case "memory":
        return { ...base, subtitle: entity.data.paths[0] ?? entity.data.file };
      case "subAgent":
        return { ...base, subtitle: entity.data.description ?? entity.data.model ?? undefined };
      case "slashCommand":
        return { ...base, subtitle: entity.data.description ?? entity.data.origin.file };
      case "hook":
        return {
          ...base,
          subtitle: `${entity.data.entries.length} hook${entity.data.entries.length === 1 ? "" : "s"}`,
        };
      case "automation": {
        const a = entity.data;
        return {
          ...base,
          subtitle: a.rrule ?? a.kind,
          enabled: { enabled: a.status === "ACTIVE", editable: true, toggle: null, reason: a.status },
          onToggle: (on) =>
            void requestPlan(() =>
              rpc("preview_set_automation_status", { id: a.id, status: on ? "ACTIVE" : "PAUSED" }),
            ),
        };
      }
    }
  };
}

const CREATABLE: EntityKind[] = ["skill", "subAgent", "slashCommand", "mcpServer", "automation"];

export function EntityScreen({ kind }: { kind: EntityKind }) {
  const filtered = useFilteredEntities(kind);
  const location = useUiStore((s) => s.locationFilter);
  const setLocation = useUiStore((s) => s.setLocationFilter);
  const entityListCollapsed = useUiStore((s) => s.entityListCollapsed);
  const memorySource = useUiStore((s) => s.memorySource);
  const snapshot = useSnapshot();
  const buildRow = useRowBuilder();
  const [creating, setCreating] = useState(false);
  const rows = filtered.map((f) => buildRow(f.entity, f.duplicateCount));
  const label = KIND_LABELS[kind];

  return (
    <div className="flex h-full min-h-0 flex-col">
      <TopBar
        actions={
          CREATABLE.includes(kind) ? (
            <Button size="sm" className="h-8 gap-1 text-xs" onClick={() => setCreating(true)}>
              <Plus className="h-3.5 w-3.5" /> New {label.singular.toLowerCase()}
            </Button>
          ) : undefined
        }
      />
      {kind === "skill" && <SkillFilterBar count={rows.length} loading={snapshot.isPending} />}
      <div className="flex min-h-0 flex-1">
        <div
          id="entity-list"
          hidden={entityListCollapsed}
          className="w-[380px] shrink-0 overflow-y-auto border-r"
        >
          {kind === "memory" && memorySource !== "all" && (
            <div className="space-y-1 border-b px-3 py-2">
              <h2 className="text-sm font-medium">{MEMORY_SOURCE_LABELS[memorySource]}</h2>
              <p className="break-words text-xs text-muted-foreground">
                {memorySourceDescription(memorySource, snapshot.data?.homes)}
              </p>
            </div>
          )}
          <EntityList
            rows={rows}
            emptyText={
              location.type === "all"
                ? `No ${label.plural.toLowerCase()} match.`
                : `No ${label.plural.toLowerCase()} match in ${locationFilterLabel(location)}.`
            }
            emptyAction={
              location.type !== "all" ? (
                <Button
                  variant="outline"
                  size="sm"
                  className="mt-3"
                  onClick={() => setLocation({ type: "all" })}
                >
                  Clear location filter
                </Button>
              ) : undefined
            }
          />
        </div>
        <div className="min-w-0 flex-1">
          <DetailPane />
        </div>
      </div>
      {creating && kind === "mcpServer" && <NewMcpDialog onClose={() => setCreating(false)} />}
      {creating && kind === "automation" && <NewAutomationDialog onClose={() => setCreating(false)} />}
      {creating && (kind === "skill" || kind === "subAgent" || kind === "slashCommand") && (
        <NewEntityDialog kind={kind} onClose={() => setCreating(false)} />
      )}
    </div>
  );
}
