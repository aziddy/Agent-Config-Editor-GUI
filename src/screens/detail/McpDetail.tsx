import { useQuery } from "@tanstack/react-query";
import { KeyRound } from "lucide-react";
import { useState } from "react";
import { EnabledSwitch } from "@/components/EnabledSwitch";
import { InfoRow } from "@/components/FieldRow";
import { PluginScopeGate } from "@/components/PluginScopeGate";
import { Badge } from "@/components/ui/badge";
import { Select } from "@/components/ui/select";
import { McpServerForm } from "@/forms/McpServerForm";
import { basename } from "@/lib/entities";
import { useRequestPlan } from "@/lib/mutations";
import { useSnapshot } from "@/lib/queries";
import { type McpTarget, rpc } from "@/lib/rpc";
import type { McpServer } from "@/lib/types";
import { useUiStore } from "@/state/uiStore";

export function McpDetail({ server }: { server: McpServer }) {
  const snapshot = useSnapshot();
  const requestPlan = useRequestPlan();
  const select = useUiStore((s) => s.select);
  const setNav = useUiStore((s) => s.setNav);
  const [unlocked, setUnlocked] = useState(false);
  const [projectRoot, setProjectRoot] = useState<string>("");
  const pluginScoped = server.scope.type === "plugin";
  const target: McpTarget = {
    agent: server.agent,
    scope: server.scope,
    file: server.origin.file,
    name: server.name,
  };
  const perProject =
    server.agent === "claude" && (server.scope.type === "user" || server.scope.type === "plugin");
  const projects = (snapshot.data?.projects ?? []).filter((p) => p.exists && p.trackedBy.includes("claude"));
  const disabledIn = Array.isArray(server.extra.disabledInProjects)
    ? (server.extra.disabledInProjects as string[])
    : [];
  const runtime = useQuery({
    queryKey: ["codexRuntime"],
    queryFn: () => rpc("codex_mcp_status", {}),
    enabled: server.agent === "codex",
    staleTime: 30_000,
    retry: false,
  });
  const rt = runtime.data?.find((r) => r.name === server.name);
  const extraEntries = Object.entries(server.extra).filter(([k]) => k !== "disabledInProjects");
  const providerPlugin = snapshot.data?.plugins.find(
    (p) => p.id === server.fromPlugin && p.agent === server.agent,
  );

  return (
    <div className="space-y-4">
      <div className="space-y-1.5">
        <InfoRow label="Enabled">
          <div className="flex flex-wrap items-center gap-2">
            {perProject ? (
              <>
                <Select
                  value={projectRoot}
                  onChange={(e) => setProjectRoot(e.target.value)}
                  className="h-7 w-56 text-xs"
                >
                  <option value="">Choose a project…</option>
                  {projects.map((p) => (
                    <option key={p.root} value={p.root}>
                      {basename(p.root)}
                      {disabledIn.includes(p.root) ? " (disabled)" : ""}
                    </option>
                  ))}
                </Select>
                <EnabledSwitch
                  state={{
                    enabled: projectRoot ? !disabledIn.includes(projectRoot) : true,
                    editable: projectRoot !== "",
                    toggle: null,
                    reason: projectRoot ? null : "Claude Code disables user and plugin servers per project",
                  }}
                  onChange={(enabled) =>
                    void requestPlan(() => rpc("preview_set_mcp_enabled", { target, enabled, projectRoot }))
                  }
                />
                {disabledIn.length > 0 && (
                  <span className="text-xs text-muted-foreground">
                    Disabled in {disabledIn.map(basename).join(", ")}
                  </span>
                )}
              </>
            ) : (
              <>
                <EnabledSwitch
                  state={server.enabled}
                  onChange={(enabled) =>
                    void requestPlan(() =>
                      rpc("preview_set_mcp_enabled", { target, enabled, projectRoot: null }),
                    )
                  }
                />
                {server.enabled.reason && (
                  <span className="text-xs text-muted-foreground">{server.enabled.reason}</span>
                )}
              </>
            )}
          </div>
        </InfoRow>
        <InfoRow label="Config file">
          <span className="font-mono text-xs">{server.origin.file}</span>
        </InfoRow>
        {server.fromPlugin && (
          <InfoRow label="Provided by">
            <button
              type="button"
              className="font-mono text-xs underline-offset-2 hover:underline"
              onClick={() => {
                if (providerPlugin) {
                  setNav("plugin");
                  select(providerPlugin.id);
                }
              }}
            >
              {server.fromPlugin}
            </button>
          </InfoRow>
        )}
        {(server.needsAuth || rt) && (
          <InfoRow label="Status">
            <div className="flex flex-wrap items-center gap-1.5">
              {server.needsAuth && (
                <Badge variant="warning" className="gap-1">
                  <KeyRound className="h-3 w-3" /> needs authentication
                </Badge>
              )}
              {rt && (
                <>
                  <Badge variant={rt.enabled ? "success" : "muted"}>
                    {rt.enabled ? "loaded by codex" : "not loaded"}
                  </Badge>
                  {typeof rt.authStatus === "string" && <Badge variant="outline">{rt.authStatus}</Badge>}
                  {typeof rt.disabledReason === "string" && (
                    <span className="text-xs text-muted-foreground">{rt.disabledReason}</span>
                  )}
                </>
              )}
            </div>
          </InfoRow>
        )}
      </div>

      {pluginScoped && server.scope.type === "plugin" && (
        <PluginScopeGate
          pluginId={server.scope.pluginId}
          unlocked={unlocked}
          onUnlock={() => setUnlocked(true)}
        />
      )}

      <McpServerForm
        agent={server.agent}
        server={server}
        disabled={pluginScoped}
        onSave={(input) => requestPlan(() => rpc("preview_upsert_mcp_server", { target, input }))}
        onDelete={() => void requestPlan(() => rpc("preview_delete_mcp_server", { target }))}
      />

      {extraEntries.length > 0 && (
        <div>
          <p className="mb-1 text-xs font-medium text-muted-foreground">Other fields (kept as-is)</p>
          <pre className="overflow-auto rounded border bg-muted/30 p-2 font-mono text-xs">
            {extraEntries.map(([k, v]) => `${k} = ${JSON.stringify(v)}`).join("\n")}
          </pre>
        </div>
      )}
    </div>
  );
}
