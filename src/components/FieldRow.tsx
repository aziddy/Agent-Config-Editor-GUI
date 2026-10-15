import type { ReactNode } from "react";
import { AgentBadge } from "@/components/AgentBadge";
import { Label } from "@/components/ui/label";
import type { Agent } from "@/lib/types";

interface RowProps {
  label: string;
  agents?: readonly Agent[];
  children: ReactNode;
}

function SupportBadges({ agents }: { agents?: readonly Agent[] }) {
  if (!agents?.length) return null;
  const names = agents.map((agent) => (agent === "claude" ? "Claude Code" : "Codex"));
  return (
    <div className="mt-1 flex flex-wrap gap-1" title={`Supported by ${names.join(" and ")}`}>
      {agents.map((agent) => (
        <AgentBadge key={agent} agent={agent} />
      ))}
    </div>
  );
}

export function FieldRow({ label, help, agents, children }: RowProps & { help?: string }) {
  return (
    <div className="grid grid-cols-[140px_1fr] items-start gap-3">
      <div className="pt-1.5">
        <Label>{label}</Label>
        <SupportBadges agents={agents} />
        {help && <p className="mt-0.5 text-[11px] leading-snug text-muted-foreground/80">{help}</p>}
      </div>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

export function InfoRow({ label, agents, children }: RowProps) {
  return (
    <div className="grid grid-cols-[140px_1fr] items-baseline gap-3 text-sm">
      {agents?.length ? (
        <div>
          <Label>{label}</Label>
          <SupportBadges agents={agents} />
        </div>
      ) : (
        <Label>{label}</Label>
      )}
      <div className="min-w-0 break-words">{children}</div>
    </div>
  );
}
