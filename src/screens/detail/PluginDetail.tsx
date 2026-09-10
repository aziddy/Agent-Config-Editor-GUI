import { ExternalLink } from "lucide-react";
import { EnabledSwitch } from "@/components/EnabledSwitch";
import { InfoRow } from "@/components/FieldRow";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { basename, findEntity } from "@/lib/entities";
import { useRequestPlan } from "@/lib/mutations";
import { useSnapshot } from "@/lib/queries";
import { rpc } from "@/lib/rpc";
import type { Plugin } from "@/lib/types";
import { useUiStore } from "@/state/uiStore";

function ContribList({ title, ids }: { title: string; ids: string[] }) {
  const snapshot = useSnapshot();
  const select = useUiStore((s) => s.select);
  const setNav = useUiStore((s) => s.setNav);
  if (ids.length === 0) return null;
  return (
    <div>
      <p className="mb-1 text-xs font-medium text-muted-foreground">
        {title} ({ids.length})
      </p>
      <ul className="space-y-0.5">
        {ids.map((id) => {
          const e = snapshot.data ? findEntity(snapshot.data, id) : null;
          const label = e ? (e.kind === "hook" ? e.data.event : e.data.name) : basename(id);
          return (
            <li key={id}>
              <button
                type="button"
                className="text-left text-sm underline-offset-2 hover:underline"
                disabled={!e}
                onClick={() => {
                  if (e) {
                    setNav(e.kind);
                    select(id);
                  }
                }}
              >
                {label}
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

export function PluginDetail({ plugin }: { plugin: Plugin }) {
  const requestPlan = useRequestPlan();
  return (
    <div className="space-y-4">
      <div className="space-y-1.5">
        <InfoRow label="Enabled">
          <div className="flex items-center gap-2">
            <EnabledSwitch
              state={plugin.enabled}
              onChange={(enabled) =>
                void requestPlan(() =>
                  rpc("preview_set_plugin_enabled", {
                    agent: plugin.agent,
                    pluginId: plugin.id,
                    scope: plugin.scope,
                    enabled,
                  }),
                )
              }
            />
            {plugin.enabled.reason && (
              <span className="text-xs text-muted-foreground">{plugin.enabled.reason}</span>
            )}
          </div>
        </InfoRow>
        <InfoRow label="Id">
          <span className="font-mono text-xs">{plugin.id}</span>
        </InfoRow>
        <InfoRow label="Marketplace">
          <span className="flex items-center gap-1.5">
            {plugin.marketplace}
            {plugin.unresolvedMarketplace && <Badge variant="warning">unknown marketplace</Badge>}
          </span>
        </InfoRow>
        <InfoRow label="Status">
          <div className="flex flex-wrap gap-1">
            <Badge variant={plugin.installed ? "success" : "destructive"}>
              {plugin.installed ? "installed" : "not installed"}
            </Badge>
            {plugin.inUse && (
              <Badge variant="muted" title="A running agent process is using this plugin">
                in use
              </Badge>
            )}
            {plugin.version && <Badge variant="outline">{plugin.version}</Badge>}
          </div>
        </InfoRow>
        {plugin.description && <InfoRow label="Description">{plugin.description}</InfoRow>}
        {plugin.author && <InfoRow label="Author">{plugin.author}</InfoRow>}
        {plugin.installPath && (
          <InfoRow label="Install path">
            <span className="font-mono text-xs">{plugin.installPath}</span>
          </InfoRow>
        )}
        {plugin.homepage && (
          <InfoRow label="Homepage">
            <Button
              variant="link"
              size="sm"
              className="h-auto gap-1 p-0 text-xs"
              onClick={() => void rpc("open_url", { url: plugin.homepage ?? "" })}
            >
              {plugin.homepage} <ExternalLink className="h-3 w-3" />
            </Button>
          </InfoRow>
        )}
        {plugin.staleDirs.length > 0 && (
          <InfoRow label="Stale versions">
            <span className="text-xs text-muted-foreground">
              {plugin.staleDirs.length} older copies still on disk (managed by the agent)
            </span>
          </InfoRow>
        )}
      </div>
      <div className="grid grid-cols-2 gap-4">
        <ContribList title="Skills" ids={plugin.contributions.skills} />
        <ContribList title="MCP servers" ids={plugin.contributions.mcpServers} />
        <ContribList title="Subagents" ids={plugin.contributions.agents} />
        <ContribList title="Commands" ids={plugin.contributions.commands} />
      </div>
    </div>
  );
}
