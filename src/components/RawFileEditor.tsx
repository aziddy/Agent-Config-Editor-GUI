import { useQuery } from "@tanstack/react-query";
import { History, RotateCcw, Save } from "lucide-react";
import { useEffect, useState } from "react";
import { CodeEditor } from "@/components/CodeEditor";
import { PathActions } from "@/components/PathActions";
import { Button } from "@/components/ui/button";
import { describeError } from "@/lib/errors";
import { useRequestPlan } from "@/lib/mutations";
import { queryKeys, useFileText } from "@/lib/queries";
import { rpc } from "@/lib/rpc";
import type { BackupInfo } from "@/lib/types";
import { useUiStore } from "@/state/uiStore";

export function RawFileEditor({ path, readOnlyHint }: { path: string; readOnlyHint?: string }) {
  const file = useFileText(path);
  const [draft, setDraft] = useState<string | null>(null);
  const [loadedHash, setLoadedHash] = useState<string | null>(null);
  const externallyChanged = useUiStore((s) => s.externallyChanged[path]);
  const clearExternallyChanged = useUiStore((s) => s.clearExternallyChanged);
  const requestPlan = useRequestPlan();
  const [showBackups, setShowBackups] = useState(false);

  // Adopt fresh content when nothing is being edited.
  useEffect(() => {
    if (file.data && (draft === null || loadedHash === file.data.hash)) {
      setDraft(file.data.text);
      setLoadedHash(file.data.hash);
    }
  }, [file.data, draft, loadedHash]);

  const dirty = file.data !== undefined && draft !== null && draft !== file.data.text;
  const conflict = dirty && file.data !== undefined && loadedHash !== null && loadedHash !== file.data.hash;
  const stale = externallyChanged !== undefined && !dirty;

  const save = async () => {
    if (!file.data || draft === null) return;
    const result = await requestPlan(() =>
      rpc("preview_save_file_text", { path, text: draft, baseHash: loadedHash }),
    );
    if (result) {
      setLoadedHash(result.newHash);
      clearExternallyChanged(path);
    }
  };

  const reload = () => {
    if (file.data) {
      setDraft(file.data.text);
      setLoadedHash(file.data.hash);
      clearExternallyChanged(path);
    }
  };

  if (file.error) return <p className="p-3 text-sm text-destructive">{describeError(file.error)}</p>;
  if (!file.data || draft === null) return <p className="p-3 text-sm text-muted-foreground">Loading…</p>;

  return (
    <div className="flex h-full min-h-0 flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground" title={path}>
          {path}
        </span>
        <PathActions path={path} size="icon" />
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1 text-xs"
          title="Backups made by this app"
          onClick={() => setShowBackups((v) => !v)}
        >
          <History className="h-3.5 w-3.5" /> Backups
        </Button>
        <Button variant="outline" size="sm" className="h-7 gap-1 text-xs" disabled={!dirty} onClick={reload}>
          <RotateCcw className="h-3.5 w-3.5" /> Discard
        </Button>
        <Button size="sm" className="h-7 gap-1 text-xs" disabled={!dirty} onClick={() => void save()}>
          <Save className="h-3.5 w-3.5" /> Save…
        </Button>
      </div>
      {readOnlyHint && (
        <p className="rounded border border-amber-500/40 bg-amber-500/10 px-2 py-1 text-xs">{readOnlyHint}</p>
      )}
      {(conflict || stale) && (
        <div className="flex items-center justify-between rounded border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs">
          <span>
            {conflict
              ? "This file changed on disk while you were editing. Saving is blocked until you reload."
              : "This file changed on disk."}
          </span>
          <Button variant="outline" size="sm" className="h-6 text-xs" onClick={reload}>
            Reload
          </Button>
        </div>
      )}
      {showBackups && <BackupList path={path} />}
      <CodeEditor
        value={draft}
        language={file.data.language}
        onChange={setDraft}
        className="min-h-0 flex-1"
      />
      <div className="text-[11px] text-muted-foreground">
        mode {file.data.mode.toString(8)} · {file.data.text.length.toLocaleString()} chars
        {dirty && " · unsaved changes"}
      </div>
    </div>
  );
}

function BackupList({ path }: { path: string }) {
  const backups = useQuery({
    queryKey: queryKeys.backups(path),
    queryFn: () => rpc("list_backups", { path }),
  });
  const requestPlan = useRequestPlan();
  const restore = (b: BackupInfo) => void requestPlan(() => rpc("preview_restore_backup", { id: b.id }));
  if (!backups.data) return null;
  if (backups.data.length === 0) {
    return <p className="rounded border p-2 text-xs text-muted-foreground">No backups yet for this file.</p>;
  }
  return (
    <ul className="max-h-40 overflow-auto rounded border text-xs">
      {backups.data.map((b) => (
        <li key={b.id} className="flex items-center justify-between gap-2 border-b px-2 py-1 last:border-b-0">
          <span className="font-mono">{new Date(b.createdAt).toLocaleString()}</span>
          <span className="text-muted-foreground">{b.size.toLocaleString()} B</span>
          <Button variant="outline" size="sm" className="h-6 text-xs" onClick={() => restore(b)}>
            Restore…
          </Button>
        </li>
      ))}
    </ul>
  );
}
