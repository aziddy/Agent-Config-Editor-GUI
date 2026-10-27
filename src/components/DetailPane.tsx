import { Trash2 } from "lucide-react";
import { useState } from "react";
import { AgentBadge } from "@/components/AgentBadge";
import { FileEditor } from "@/components/FileEditor";
import { PathActions } from "@/components/PathActions";
import { RawFileEditor } from "@/components/RawFileEditor";
import { ScopeBadges } from "@/components/ScopeBadge";
import { Button } from "@/components/ui/button";
import { Tabs } from "@/components/ui/tabs";
import {
  type AnyEntity,
  entityAgents,
  entityDir,
  entityFile,
  entityName,
  entityScope,
  entityScopes,
} from "@/lib/entities";
import { useRequestPlan } from "@/lib/mutations";
import { rpc } from "@/lib/rpc";
import { useSelectedEntity } from "@/lib/useEntities";
import { AutomationDetail } from "@/screens/detail/AutomationDetail";
import { AutomationRuns } from "@/screens/detail/AutomationRuns";
import { FrontmatterEntityDetail } from "@/screens/detail/FrontmatterEntityDetail";
import { HookDetail } from "@/screens/detail/HookDetail";
import { McpDetail } from "@/screens/detail/McpDetail";
import { MemoryDetail } from "@/screens/detail/MemoryDetail";
import { PluginDetail } from "@/screens/detail/PluginDetail";
import { type DetailTab, useUiStore } from "@/state/uiStore";

function Overview({ entity, onOpenFile }: { entity: AnyEntity; onOpenFile: (p: string) => void }) {
  switch (entity.kind) {
    case "skill":
    case "subAgent":
    case "slashCommand":
      return <FrontmatterEntityDetail entity={entity} />;
    case "mcpServer":
      return <McpDetail server={entity.data} />;
    case "plugin":
      return <PluginDetail plugin={entity.data} />;
    case "hook":
      return <HookDetail hook={entity.data} />;
    case "automation":
      return <AutomationDetail automation={entity.data} onOpenFile={onOpenFile} />;
    case "memory":
      return <MemoryDetail memory={entity.data} onOpenFile={onOpenFile} />;
  }
}

function deletablePath(entity: AnyEntity): string | null {
  const scope = entityScope(entity);
  if (scope.type !== "user" && scope.type !== "project") return null;
  switch (entity.kind) {
    case "skill":
      return entity.data.isSystem || entity.data.lockManaged ? null : entity.data.dir;
    case "automation":
      return entity.data.dir;
    case "subAgent":
    case "slashCommand":
      return entity.data.origin.file;
    default:
      return null;
  }
}

export function DetailPane() {
  const entity = useSelectedEntity();
  const selectedId = useUiStore((s) => s.selectedId);
  const [retained, setRetained] = useState(entity);
  // A filesystem rescan may remove the entity. Keep its open buffer until the user leaves it.
  if (entity && entity !== retained) setRetained(entity);
  const visible = entity ?? (retained?.data.id === selectedId ? retained : null);

  if (!visible) {
    return (
      <div className="flex h-full items-center justify-center p-8 text-center text-sm text-muted-foreground">
        Select an item to see its details, edit it, or jump to its files.
      </div>
    );
  }
  return <SelectedDetailPane key={visible.data.id} entity={visible} unavailable={!entity} />;
}

function SelectedDetailPane({ entity, unavailable }: { entity: AnyEntity; unavailable: boolean }) {
  const tab = useUiStore((s) => s.detailTab);
  const setTab = useUiStore((s) => s.setDetailTab);
  const requestNavigation = useUiStore((s) => s.requestNavigation);
  const requestPlan = useRequestPlan();
  const [file, setFile] = useState(() => entityFile(entity));
  const managedMemory =
    entity.kind === "memory" && entity.data.scopes.some((scope) => scope.type === "managed");
  const dir = managedMemory ? null : entityDir(entity);
  const description = "description" in entity.data ? entity.data.description : null;
  const scope = entityScope(entity);
  const editorHint = managedMemory
    ? "Managed instructions are read-only."
    : entity.kind === "memory" && entity.data.generated && entity.data.agents.includes("codex")
      ? "Codex may regenerate this file. Keep required rules in instruction files."
      : scope.type === "plugin"
        ? `Provided by plugin ${scope.pluginId}; edits are lost when the plugin updates.`
        : undefined;
  const deletable = deletablePath(entity);
  const openFile = (p: string) => {
    if (p === file) {
      setTab("editor");
      return;
    }
    void requestNavigation(() => {
      setFile(p);
      setTab("editor");
    });
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="space-y-2 border-b px-4 py-3">
        <div className="flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <h2 className="truncate text-base font-semibold">{entityName(entity)}</h2>
            {description && <p className="line-clamp-2 text-xs text-muted-foreground">{description}</p>}
            <div className="mt-1.5 flex flex-wrap items-center gap-1">
              {entityAgents(entity).map((agent) => (
                <AgentBadge key={agent} agent={agent} />
              ))}
              <ScopeBadges scopes={entityScopes(entity)} />
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1">
            <PathActions path={dir ?? file} size="icon" />
            {deletable && (
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7 text-destructive"
                title="Delete (after preview)"
                onClick={() => void requestPlan(() => rpc("preview_delete_path", { path: deletable }))}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            )}
          </div>
        </div>
        <Tabs<DetailTab>
          value={tab}
          onValueChange={setTab}
          items={[
            { value: "overview", label: "Overview" },
            ...(entity.kind === "automation" && entity.data.agent === "codex"
              ? [{ value: "runs" as const, label: "Runs" }]
              : []),
            { value: "editor", label: "Editor", disabled: !file && !dir },
          ]}
        />
      </div>
      {unavailable && (
        <p role="alert" className="border-b px-4 py-2 text-xs text-destructive">
          This item is no longer available on disk. Your editor buffer has been kept.
        </p>
      )}
      <div hidden={tab !== "overview"} className="min-h-0 flex-1 overflow-auto p-4">
        {tab === "overview" && <Overview entity={entity} onOpenFile={openFile} />}
      </div>
      {entity.kind === "automation" && entity.data.agent === "codex" && (
        <div hidden={tab !== "runs"} className="min-h-0 flex-1">
          <AutomationRuns automation={entity.data} active={tab === "runs"} />
        </div>
      )}
      <div hidden={tab !== "editor"} className="min-h-0 flex-1 p-3">
        <FileEditor root={dir} selectedPath={file} onOpenFile={openFile}>
          {file ? (
            <RawFileEditor key={file} path={file} readOnly={managedMemory} readOnlyHint={editorHint} />
          ) : (
            <p className="p-3 text-sm text-muted-foreground">Select a file to edit.</p>
          )}
        </FileEditor>
      </div>
    </div>
  );
}
