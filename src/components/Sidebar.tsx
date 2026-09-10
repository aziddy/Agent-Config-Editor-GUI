import { Bot, Clock, FolderGit2, Package, Plug, Settings, Sparkles, Terminal, Webhook } from "lucide-react";
import type { ComponentType } from "react";
import { KIND_LABELS, KIND_ORDER } from "@/lib/entities";
import { useSnapshot } from "@/lib/queries";
import type { EntityKind } from "@/lib/types";
import { useKindCounts } from "@/lib/useEntities";
import { cn } from "@/lib/utils";
import { type AgentFilter, type NavKind, useUiStore } from "@/state/uiStore";

const ICONS: Record<EntityKind, ComponentType<{ className?: string }>> = {
  skill: Sparkles,
  mcpServer: Plug,
  plugin: Package,
  subAgent: Bot,
  slashCommand: Terminal,
  hook: Webhook,
  automation: Clock,
};

const AGENT_FILTERS: { value: AgentFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "claude", label: "Claude" },
  { value: "codex", label: "Codex" },
];

function NavButton({
  active,
  onClick,
  icon: Icon,
  label,
  count,
}: {
  active: boolean;
  onClick: () => void;
  icon: ComponentType<{ className?: string }>;
  label: string;
  count?: number;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors",
        active
          ? "bg-accent text-accent-foreground"
          : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
      )}
    >
      <Icon className="h-4 w-4 shrink-0" />
      <span className="flex-1 truncate">{label}</span>
      {count !== undefined && <span className="text-xs tabular-nums opacity-70">{count}</span>}
    </button>
  );
}

export function Sidebar() {
  const nav = useUiStore((s) => s.nav);
  const setNav = useUiStore((s) => s.setNav);
  const agentFilter = useUiStore((s) => s.agentFilter);
  const setAgentFilter = useUiStore((s) => s.setAgentFilter);
  const counts = useKindCounts();
  const snapshot = useSnapshot();
  const projectCount = snapshot.data?.projects.filter((p) => p.exists).length ?? 0;
  const warningCount = snapshot.data?.warnings.length ?? 0;

  const go = (k: NavKind) => setNav(k);

  return (
    <aside className="flex h-full w-56 shrink-0 flex-col border-r bg-muted/20">
      <div className="p-2">
        <div className="grid grid-cols-3 rounded-md bg-muted p-0.5 text-xs">
          {AGENT_FILTERS.map((f) => (
            <button
              key={f.value}
              type="button"
              onClick={() => setAgentFilter(f.value)}
              className={cn(
                "rounded-sm py-1 font-medium transition-colors",
                agentFilter === f.value
                  ? "bg-background shadow-sm"
                  : "text-muted-foreground hover:text-foreground",
                f.value === "claude" && agentFilter === f.value && "text-claude",
                f.value === "codex" && agentFilter === f.value && "text-codex",
              )}
            >
              {f.label}
            </button>
          ))}
        </div>
      </div>
      <nav className="flex-1 space-y-0.5 overflow-y-auto px-2">
        {KIND_ORDER.map((kind) => (
          <NavButton
            key={kind}
            active={nav === kind}
            onClick={() => go(kind)}
            icon={ICONS[kind]}
            label={KIND_LABELS[kind].plural}
            count={counts[kind]}
          />
        ))}
        <div className="my-2 border-t" />
        <NavButton
          active={nav === "projects"}
          onClick={() => go("projects")}
          icon={FolderGit2}
          label="Projects"
          count={projectCount}
        />
        <NavButton
          active={nav === "settings"}
          onClick={() => go("settings")}
          icon={Settings}
          label="Settings"
        />
      </nav>
      <div className="space-y-1 border-t p-2 text-[11px] text-muted-foreground">
        {snapshot.data?.homes?.sandbox && (
          <div
            className="rounded bg-destructive/15 px-1.5 py-0.5 text-destructive"
            title={snapshot.data.homes.home}
          >
            Sandbox home
          </div>
        )}
        {snapshot.data?.homes?.readOnly && (
          <div className="rounded bg-amber-500/15 px-1.5 py-0.5 text-amber-700">Read-only mode</div>
        )}
        {warningCount > 0 && (
          <button type="button" className="text-left hover:text-foreground" onClick={() => go("projects")}>
            {warningCount} scan warning{warningCount === 1 ? "" : "s"}
          </button>
        )}
        {snapshot.data && <div>Scanned in {snapshot.data.scanMillis} ms</div>}
      </div>
    </aside>
  );
}
