import {
  Bot,
  Brain,
  Clock,
  FileCode2,
  FlaskConical,
  FolderGit2,
  LockKeyhole,
  Package,
  Plug,
  Settings,
  Sparkles,
  Terminal,
  TriangleAlert,
  Webhook,
} from "lucide-react";
import type { ComponentType } from "react";
import { LocationPicker } from "@/components/LocationPicker";
import {
  KIND_LABELS,
  KIND_ORDER,
  MEMORY_SOURCE_LABELS,
  MEMORY_SOURCE_ORDER,
  memorySourceDescription,
} from "@/lib/entities";
import { useSnapshot } from "@/lib/queries";
import type { EntityKind } from "@/lib/types";
import { useKindCounts, useMemorySourceCounts } from "@/lib/useEntities";
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
  memory: Brain,
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
  collapsed,
  expanded,
  controls,
}: {
  active: boolean;
  onClick: () => void;
  icon: ComponentType<{ className?: string }>;
  label: string;
  count?: number;
  collapsed: boolean;
  expanded?: boolean;
  controls?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      aria-current={active ? "page" : undefined}
      aria-expanded={expanded}
      aria-controls={controls}
      title={collapsed ? `${label}${count !== undefined ? ` (${count})` : ""}` : undefined}
      className={cn(
        "flex w-full items-center gap-2 rounded-md px-2 text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
        collapsed ? "justify-center py-2" : "py-1.5",
        active
          ? "bg-accent text-accent-foreground"
          : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
      )}
    >
      <Icon className="h-4 w-4 shrink-0" />
      <span className={collapsed ? "sr-only" : "flex-1 truncate"}>{label}</span>
      {!collapsed && count !== undefined && <span className="text-xs tabular-nums opacity-70">{count}</span>}
    </button>
  );
}

export function Sidebar() {
  const nav = useUiStore((s) => s.nav);
  const setNav = useUiStore((s) => s.setNav);
  const agentFilter = useUiStore((s) => s.agentFilter);
  const setAgentFilter = useUiStore((s) => s.setAgentFilter);
  const collapsed = useUiStore((s) => s.sidebarCollapsed);
  const counts = useKindCounts();
  const memoryCounts = useMemorySourceCounts();
  const memorySource = useUiStore((s) => s.memorySource);
  const navigateToMemorySource = useUiStore((s) => s.navigateToMemorySource);
  const snapshot = useSnapshot();
  const projectCount = snapshot.data?.projects.filter((p) => p.exists).length ?? 0;
  const warningCount = snapshot.data?.warnings.length ?? 0;
  const warningLabel = `${warningCount} scan warning${warningCount === 1 ? "" : "s"}`;

  const go = (k: NavKind) => setNav(k);

  return (
    <aside
      id="navigation-sidebar"
      aria-label="Navigation"
      className={cn(
        "flex h-full min-h-0 shrink-0 flex-col border-r bg-muted/20",
        collapsed ? "w-14" : "w-56",
      )}
    >
      <div hidden={collapsed} className="p-2">
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
        <LocationPicker />
      </div>
      <nav className={cn("flex-1 space-y-0.5 overflow-y-auto px-2", collapsed && "py-2")}>
        {KIND_ORDER.map((kind) => (
          <div key={kind}>
            <NavButton
              active={nav === kind && (kind !== "memory" || memorySource === "all" || collapsed)}
              onClick={() => go(kind)}
              icon={ICONS[kind]}
              label={KIND_LABELS[kind].plural}
              count={counts[kind]}
              collapsed={collapsed}
              expanded={kind === "memory" ? nav === "memory" && !collapsed : undefined}
              controls={kind === "memory" ? "memory-sources" : undefined}
            />
            {kind === "memory" && (
              <ul
                id="memory-sources"
                aria-label="Memory sources"
                hidden={nav !== "memory" || collapsed}
                className="ml-4 mt-1 space-y-0.5 border-l pl-1"
              >
                {MEMORY_SOURCE_ORDER.map((source) => (
                  <li key={source}>
                    <button
                      type="button"
                      aria-label={MEMORY_SOURCE_LABELS[source]}
                      aria-current={memorySource === source ? "page" : undefined}
                      title={memorySourceDescription(source, snapshot.data?.homes)}
                      onClick={() => void navigateToMemorySource(source)}
                      className={cn(
                        "flex w-full items-center gap-1 rounded-md px-2 py-1.5 text-left text-xs transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                        memorySource === source
                          ? "bg-accent text-accent-foreground"
                          : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
                      )}
                    >
                      <span className="min-w-0 flex-1 truncate">{MEMORY_SOURCE_LABELS[source]}</span>
                      <span className="tabular-nums opacity-70">{memoryCounts[source]}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        ))}
        <div className="my-2 border-t" />
        <NavButton
          active={nav === "projects"}
          onClick={() => go("projects")}
          icon={FolderGit2}
          label="Projects"
          count={projectCount}
          collapsed={collapsed}
        />
        <NavButton
          active={nav === "codexConfig"}
          onClick={() => go("codexConfig")}
          icon={FileCode2}
          label="Codex Config"
          collapsed={collapsed}
        />
        <NavButton
          active={nav === "settings"}
          onClick={() => go("settings")}
          icon={Settings}
          label="Settings"
          collapsed={collapsed}
        />
      </nav>
      <div
        className={cn(
          "border-t p-2 text-[11px] text-muted-foreground",
          collapsed ? "flex flex-col items-center gap-2" : "space-y-1",
        )}
      >
        {snapshot.data?.homes?.sandbox && (
          <div
            className={cn(
              "rounded bg-destructive/15 text-destructive",
              collapsed ? "p-1.5" : "px-1.5 py-0.5",
            )}
            title={`Sandbox home: ${snapshot.data.homes.home}`}
          >
            {collapsed && <FlaskConical className="h-4 w-4" aria-hidden="true" />}
            <span className={collapsed ? "sr-only" : undefined}>Sandbox home</span>
          </div>
        )}
        {snapshot.data?.homes?.readOnly && (
          <div
            className={cn("rounded bg-amber-500/15 text-amber-700", collapsed ? "p-1.5" : "px-1.5 py-0.5")}
            title="Read-only mode"
          >
            {collapsed && <LockKeyhole className="h-4 w-4" aria-hidden="true" />}
            <span className={collapsed ? "sr-only" : undefined}>Read-only mode</span>
          </div>
        )}
        {warningCount > 0 && (
          <button
            type="button"
            className={cn(
              "rounded hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
              collapsed ? "p-1.5" : "text-left",
            )}
            onClick={() => go("projects")}
            aria-label={warningLabel}
            title={warningLabel}
          >
            {collapsed && <TriangleAlert className="h-4 w-4" aria-hidden="true" />}
            <span className={collapsed ? "sr-only" : undefined}>{warningLabel}</span>
          </button>
        )}
        {snapshot.data && <div hidden={collapsed}>Scanned in {snapshot.data.scanMillis} ms</div>}
      </div>
    </aside>
  );
}
