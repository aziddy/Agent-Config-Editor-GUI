import { InfoRow } from "@/components/FieldRow";
import { Badge } from "@/components/ui/badge";
import type { HookEvent } from "@/lib/types";

export function HookDetail({ hook }: { hook: HookEvent }) {
  return (
    <div className="space-y-3">
      <InfoRow label="Event">
        <span className="font-mono text-sm">{hook.event}</span>
      </InfoRow>
      <InfoRow label="Defined in">
        <span className="font-mono text-xs">{hook.entries[0]?.origin.file}</span>
      </InfoRow>
      <p className="text-xs text-muted-foreground">Hooks are read-only here; edit them in the Raw tab.</p>
      <ul className="divide-y rounded border">
        {hook.entries.map((h, i) => (
          <li
            key={h.origin.locator.type === "jsonPointer" ? h.origin.locator.pointer : `${h.command}-${i}`}
            className="space-y-1 p-2 text-sm"
          >
            <div className="flex flex-wrap items-center gap-1.5">
              <Badge variant="outline">{h.hookType}</Badge>
              {h.matcher !== null && <Badge variant="muted">matcher: {h.matcher || "*"}</Badge>}
              {h.timeout !== null && <Badge variant="muted">timeout {h.timeout}s</Badge>}
              {h.trustEntryPresent === true && <Badge variant="success">trusted</Badge>}
              {h.trustEntryPresent === false && <Badge variant="warning">no trust entry</Badge>}
            </div>
            <pre className="whitespace-pre-wrap break-all font-mono text-xs">{h.command}</pre>
          </li>
        ))}
      </ul>
    </div>
  );
}
