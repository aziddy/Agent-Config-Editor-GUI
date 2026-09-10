import { Badge } from "@/components/ui/badge";
import { scopeLabel } from "@/lib/entities";
import type { Scope } from "@/lib/types";

export function ScopeBadge({ scope }: { scope: Scope }) {
  const title =
    scope.type === "project" ? scope.root : scope.type === "plugin" ? `Plugin ${scope.pluginId}` : scope.type;
  return (
    <Badge
      variant="secondary"
      className="max-w-[160px] truncate px-1.5 py-0 text-[10px] font-normal"
      title={title}
    >
      {scope.type === "project" && <span className="mr-1 opacity-60">proj</span>}
      {scope.type === "plugin" && <span className="mr-1 opacity-60">plugin</span>}
      {scopeLabel(scope)}
    </Badge>
  );
}
