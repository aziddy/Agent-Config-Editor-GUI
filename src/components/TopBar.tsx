import { useQueryClient } from "@tanstack/react-query";
import { Copy, RefreshCw, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { KIND_LABELS } from "@/lib/entities";
import { queryKeys, useSnapshot } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/state/uiStore";

export function TopBar({ actions }: { actions?: React.ReactNode }) {
  const nav = useUiStore((s) => s.nav);
  const search = useUiStore((s) => s.search);
  const setSearch = useUiStore((s) => s.setSearch);
  const showDuplicates = useUiStore((s) => s.showDuplicates);
  const toggleDuplicates = useUiStore((s) => s.toggleDuplicates);
  const qc = useQueryClient();
  const snapshot = useSnapshot();
  const title = nav === "projects" ? "Projects" : nav === "settings" ? "Settings" : KIND_LABELS[nav].plural;
  const isEntityKind = nav !== "projects" && nav !== "settings";

  return (
    <div className="flex items-center gap-2 border-b px-3 py-2">
      <h1 className="w-40 shrink-0 truncate text-sm font-semibold">{title}</h1>
      {isEntityKind && (
        <div className="relative max-w-md flex-1">
          <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={`Search ${title.toLowerCase()}…`}
            className="h-8 pl-7 text-sm"
          />
        </div>
      )}
      <div className="flex-1" />
      {isEntityKind && (
        <Button
          variant="ghost"
          size="sm"
          className={cn("h-8 gap-1 text-xs", showDuplicates && "bg-accent")}
          title="Show every copy of duplicated entities instead of collapsing them"
          onClick={toggleDuplicates}
        >
          <Copy className="h-3.5 w-3.5" />
          {showDuplicates ? "Duplicates shown" : "Duplicates collapsed"}
        </Button>
      )}
      {actions}
      <Button
        variant="ghost"
        size="sm"
        className="h-8 gap-1 text-xs"
        onClick={() => void qc.invalidateQueries({ queryKey: queryKeys.snapshot })}
        disabled={snapshot.isFetching}
        title="Rescan configuration"
      >
        <RefreshCw className={cn("h-3.5 w-3.5", snapshot.isFetching && "animate-spin")} />
        Rescan
      </Button>
    </div>
  );
}
