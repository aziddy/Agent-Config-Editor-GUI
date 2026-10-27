import { InfoRow } from "@/components/FieldRow";
import { MarkdownContent } from "@/components/MarkdownContent";
import { PathActions } from "@/components/PathActions";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { MEMORY_SOURCE_LABELS, scopeKey, scopeLabel } from "@/lib/entities";
import { describeError } from "@/lib/errors";
import { useFileText } from "@/lib/queries";
import type { MemoryFile } from "@/lib/types";

export function MemoryDetail({
  memory,
  onOpenFile,
}: {
  memory: MemoryFile;
  onOpenFile: (path: string) => void;
}) {
  const path = memory.paths[0] ?? memory.file;
  const file = useFileText(path);
  const managed = memory.scopes.some((scope) => scope.type === "managed");

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <InfoRow label="Source">
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant="secondary">{memory.category === "memory" ? "Memory" : "Instructions"}</Badge>
            <span>{MEMORY_SOURCE_LABELS[memory.source]}</span>
          </div>
        </InfoRow>
        <InfoRow label="File">
          <div className="space-y-1 font-mono text-xs">
            {(memory.paths.length ? memory.paths : [memory.file]).map((alias) => (
              <p key={alias}>{alias}</p>
            ))}
          </div>
        </InfoRow>
        {memory.paths.some((alias) => alias !== memory.file) && (
          <InfoRow label="Resolved target">
            <span className="font-mono text-xs">{memory.file}</span>
          </InfoRow>
        )}
        <details className="text-xs">
          <summary className="cursor-pointer text-muted-foreground">
            Locations ({memory.scopes.length})
          </summary>
          <ul className="mt-2 space-y-1 break-all font-mono">
            {memory.scopes.map((scope) => (
              <li key={scopeKey(scope)}>{scope.type === "project" ? scope.root : scopeLabel(scope)}</li>
            ))}
          </ul>
        </details>
        {memory.generated && memory.agents.includes("codex") && (
          <p className="rounded border border-amber-500/40 bg-amber-500/10 px-2 py-1 text-xs">
            Codex may regenerate this file. Keep required rules in instruction files.
          </p>
        )}
        {managed && <p className="text-xs text-muted-foreground">Managed instructions are read-only.</p>}
        <p className="text-xs text-muted-foreground">
          Stored files are shown even when memory is disabled. Instruction loading depends on the agent
          settings and working directory.
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" className="h-7 text-xs" onClick={() => onOpenFile(path)}>
            {managed ? "View source" : memory.category === "memory" ? "Edit memory" : "Edit instructions"}
          </Button>
          <PathActions path={path} />
        </div>
      </div>
      <div className="border-t pt-4">
        {file.error ? (
          <div role="alert" className="space-y-2 text-sm text-destructive">
            <p>{describeError(file.error)}</p>
            <Button
              variant="outline"
              size="sm"
              disabled={file.isFetching}
              onClick={() => void file.refetch()}
            >
              Retry
            </Button>
          </div>
        ) : !file.data ? (
          <p className="text-sm text-muted-foreground">Loading file…</p>
        ) : file.data.text.length === 0 ? (
          <p className="text-sm text-muted-foreground">This file is empty.</p>
        ) : (
          <MarkdownContent text={file.data.text} />
        )}
      </div>
    </div>
  );
}
