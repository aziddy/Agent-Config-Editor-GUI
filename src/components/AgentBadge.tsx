import { Badge } from "@/components/ui/badge";
import type { Agent } from "@/lib/types";
import { cn } from "@/lib/utils";

export function AgentBadge({ agent, className }: { agent: Agent; className?: string }) {
  return (
    <Badge
      className={cn(
        "border-transparent px-1.5 py-0 text-[10px] font-semibold uppercase tracking-wide",
        agent === "claude" ? "bg-claude/15 text-claude" : "bg-codex/15 text-codex",
        className,
      )}
    >
      {agent}
    </Badge>
  );
}
