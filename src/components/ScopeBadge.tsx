import { Badge } from "@/components/ui/badge";
import { scopeKey, scopeLabel } from "@/lib/entities";
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

/** Keep shared ancestor instructions from filling the list or detail header with badges. */
export function ScopeBadges({ scopes }: { scopes: Scope[] }) {
  const priority = (scope: Scope) => (scope.type === "user" ? 0 : scope.type === "managed" ? 1 : 2);
  const sorted = [...scopes].sort((a, b) => priority(a) - priority(b));
  const visible = sorted.slice(0, 2);
  const hidden = sorted.slice(2);
  return (
    <>
      {visible.map((scope) => (
        <ScopeBadge key={scopeKey(scope)} scope={scope} />
      ))}
      {hidden.length > 0 && (
        <Badge
          variant="secondary"
          className="px-1.5 py-0 text-[10px] font-normal"
          title={hidden
            .map((scope) => (scope.type === "project" ? scope.root : scopeLabel(scope)))
            .join("\n")}
        >
          +{hidden.length} locations
        </Badge>
      )}
    </>
  );
}
