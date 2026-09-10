import { Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { AgentBadge } from "@/components/AgentBadge";
import { FilesTab } from "@/components/FilesTab";
import { PathActions } from "@/components/PathActions";
import { RawFileEditor } from "@/components/RawFileEditor";
import { ScopeBadge } from "@/components/ScopeBadge";
import { Button } from "@/components/ui/button";
import { Tabs } from "@/components/ui/tabs";
import { type AnyEntity, entityDir, entityFile, entityName, entityScope } from "@/lib/entities";
import { useRequestPlan } from "@/lib/mutations";
import { rpc } from "@/lib/rpc";
import { useSelectedEntity } from "@/lib/useEntities";
import { AutomationDetail } from "@/screens/detail/AutomationDetail";
import { FrontmatterEntityDetail } from "@/screens/detail/FrontmatterEntityDetail";
import { HookDetail } from "@/screens/detail/HookDetail";
import { McpDetail } from "@/screens/detail/McpDetail";
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
  const tab = useUiStore((s) => s.detailTab);
  const setTab = useUiStore((s) => s.setDetailTab);
  const requestPlan = useRequestPlan();
  const [rawPath, setRawPath] = useState<string | null>(null);
  const id = entity?.data.id ?? null;
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset the raw-file override whenever the selection changes
  useEffect(() => setRawPath(null), [id]);

  if (!entity) {
    return (
      <div className="flex h-full items-center justify-center p-8 text-center text-sm text-muted-foreground">
        Select an item to see its details, edit it, or jump to its files.
      </div>
    );
  }

  const file = rawPath ?? entityFile(entity);
  const dir = entityDir(entity);
  const description = "description" in entity.data ? entity.data.description : null;
  const pluginHint =
    entity.data.scope.type === "plugin"
      ? `Provided by plugin ${entity.data.scope.pluginId}; edits are lost when the plugin updates.`
      : undefined;
  const deletable = deletablePath(entity);
  const openFile = (p: string) => {
    setRawPath(p);
    setTab("raw");
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="space-y-2 border-b px-4 py-3">
        <div className="flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <h2 className="truncate text-base font-semibold">{entityName(entity)}</h2>
            {description && <p className="line-clamp-2 text-xs text-muted-foreground">{description}</p>}
            <div className="mt-1.5 flex flex-wrap items-center gap-1">
              <AgentBadge agent={entity.data.agent} />
              <ScopeBadge scope={entityScope(entity)} />
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1">
            <PathActions path={dir ?? file} />
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
            { value: "raw", label: "Raw file", disabled: !file },
            { value: "files", label: "Files", disabled: !dir },
          ]}
        />
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-4">
        {tab === "overview" && <Overview entity={entity} onOpenFile={openFile} />}
        {tab === "raw" && file && <RawFileEditor key={file} path={file} readOnlyHint={pluginHint} />}
        {tab === "files" && dir && <FilesTab root={dir} onOpenFile={openFile} />}
      </div>
    </div>
  );
}
