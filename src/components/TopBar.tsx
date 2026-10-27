import { useQueryClient } from "@tanstack/react-query";
import { Copy, List, ListCollapse, PanelLeftClose, PanelLeftOpen, RefreshCw, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { KIND_LABELS } from "@/lib/entities";
import { queryKeys, useSnapshot } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/state/uiStore";

const PAGE_TITLES = {
  projects: "Projects",
  codexConfig: "Codex Config",
  codexSettings: "Codex Settings",
  settings: "Settings",
};

export function TopBar({ actions }: { actions?: React.ReactNode }) {
  const nav = useUiStore((s) => s.nav);
  const search = useUiStore((s) => s.search);
  const setSearch = useUiStore((s) => s.setSearch);
  const showDuplicates = useUiStore((s) => s.showDuplicates);
  const toggleDuplicates = useUiStore((s) => s.toggleDuplicates);
  const sidebarCollapsed = useUiStore((s) => s.sidebarCollapsed);
  const entityListCollapsed = useUiStore((s) => s.entityListCollapsed);
  const toggleSidebar = useUiStore((s) => s.toggleSidebar);
  const toggleEntityList = useUiStore((s) => s.toggleEntityList);
  const qc = useQueryClient();
  const snapshot = useSnapshot();
  const isEntityKind =
    nav !== "projects" && nav !== "settings" && nav !== "codexConfig" && nav !== "codexSettings";
  const title = isEntityKind ? KIND_LABELS[nav].plural : PAGE_TITLES[nav];
  const sidebarLabel = sidebarCollapsed ? "Expand navigation" : "Collapse navigation";
  const listLabel = entityListCollapsed ? "Expand item list" : "Collapse item list";
  const SidebarIcon = sidebarCollapsed ? PanelLeftOpen : PanelLeftClose;
  const ListIcon = entityListCollapsed ? List : ListCollapse;

  return (
    <div className="flex shrink-0 items-center gap-2 border-b px-3 py-2">
      <div className="flex shrink-0 items-center gap-1">
        <Button
          variant="ghost"
          size="icon"
          className={cn("h-8 w-8", sidebarCollapsed && "bg-accent")}
          aria-label={sidebarLabel}
          title={sidebarLabel}
          aria-expanded={!sidebarCollapsed}
          aria-controls="navigation-sidebar"
          onClick={toggleSidebar}
        >
          <SidebarIcon className="h-4 w-4" aria-hidden="true" />
        </Button>
        {isEntityKind && (
          <Button
            variant="ghost"
            size="icon"
            className={cn("h-8 w-8", entityListCollapsed && "bg-accent")}
            aria-label={listLabel}
            title={listLabel}
            aria-expanded={!entityListCollapsed}
            aria-controls="entity-list"
            onClick={toggleEntityList}
          >
            <ListIcon className="h-4 w-4" aria-hidden="true" />
          </Button>
        )}
      </div>
      <h1 className="min-w-0 max-w-40 truncate text-sm font-semibold">{title}</h1>
      {isEntityKind && (
        <div className="relative min-w-0 max-w-md flex-1">
          <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={`Search ${title.toLowerCase()}…`}
            className="h-8 pl-7 text-sm"
          />
        </div>
      )}
      <div className="ml-auto flex shrink-0 items-center gap-2">
        {isEntityKind && nav !== "memory" && (
          <Button
            variant="ghost"
            size="sm"
            className={cn("h-8 gap-1 px-2 text-xs xl:px-3", showDuplicates && "bg-accent")}
            aria-label={showDuplicates ? "Collapse duplicates" : "Show duplicates"}
            aria-pressed={showDuplicates}
            title="Show every copy of duplicated entities instead of collapsing them"
            onClick={toggleDuplicates}
          >
            <Copy className="h-3.5 w-3.5" />
            <span className="hidden xl:inline">
              {showDuplicates ? "Duplicates shown" : "Duplicates collapsed"}
            </span>
          </Button>
        )}
        {actions}
        <Button
          variant="ghost"
          size="sm"
          className="h-8 gap-1 px-2 text-xs xl:px-3"
          onClick={() => void qc.invalidateQueries({ queryKey: queryKeys.snapshot })}
          disabled={snapshot.isFetching}
          title="Rescan configuration"
          aria-label="Rescan configuration"
        >
          <RefreshCw className={cn("h-3.5 w-3.5", snapshot.isFetching && "animate-spin")} />
          <span className="hidden xl:inline">Rescan</span>
        </Button>
      </div>
    </div>
  );
}
