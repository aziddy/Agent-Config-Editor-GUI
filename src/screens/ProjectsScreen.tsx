import { AlertTriangle } from "lucide-react";
import { AgentBadge } from "@/components/AgentBadge";
import { PathActions } from "@/components/PathActions";
import { TopBar } from "@/components/TopBar";
import { Badge } from "@/components/ui/badge";
import { useAppSettings, useSnapshot } from "@/lib/queries";
import { useUiStore } from "@/state/uiStore";

export function ProjectsScreen() {
  const snapshot = useSnapshot();
  const settings = useAppSettings();
  const agentFilter = useUiStore((s) => s.agentFilter);
  const showMissing = settings.data?.showMissingProjects ?? false;
  const projects = (snapshot.data?.projects ?? []).filter(
    (p) => (showMissing || p.exists) && (agentFilter === "all" || p.trackedBy.includes(agentFilter)),
  );
  const warnings = snapshot.data?.warnings ?? [];
  return (
    <div className="flex h-full min-h-0 flex-col">
      <TopBar />
      <div className="min-h-0 flex-1 overflow-auto p-4">
        <p className="mb-3 text-xs text-muted-foreground">
          Projects each agent already tracks. Their <span className="font-mono">.claude/</span>,{" "}
          <span className="font-mono">.mcp.json</span>, <span className="font-mono">.agents/skills</span> and{" "}
          <span className="font-mono">.codex/</span> are scanned; nothing else on disk is.
        </p>
        <ul className="divide-y rounded border">
          {projects.map((p) => (
            <li key={p.root} className="flex items-center gap-2 px-3 py-2 text-sm">
              <span
                className={
                  p.exists
                    ? "min-w-0 flex-1 truncate font-mono text-xs"
                    : "min-w-0 flex-1 truncate font-mono text-xs line-through opacity-60"
                }
                title={p.root}
              >
                {p.root}
              </span>
              {!p.exists && <Badge variant="destructive">missing</Badge>}
              {p.trackedBy.map((a) => (
                <AgentBadge key={a} agent={a} />
              ))}
              {p.exists && <PathActions path={p.root} size="icon" />}
            </li>
          ))}
          {projects.length === 0 && <li className="px-3 py-2 text-xs text-muted-foreground">No projects.</li>}
        </ul>
        {warnings.length > 0 && (
          <div className="mt-6">
            <h3 className="mb-2 flex items-center gap-1.5 text-sm font-semibold">
              <AlertTriangle className="h-4 w-4 text-amber-600" /> Scan warnings ({warnings.length})
            </h3>
            <ul className="divide-y rounded border text-xs">
              {warnings.map((w, i) => (
                <li key={`${w.file ?? ""}:${i}`} className="px-3 py-1.5">
                  <div>{w.message}</div>
                  {w.file && <div className="font-mono text-muted-foreground">{w.file}</div>}
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
