import { useQuery } from "@tanstack/react-query";
import { History, RotateCcw, Save } from "lucide-react";
import { useCallback, useEffect, useReducer, useState } from "react";
import { toast } from "sonner";
import { CodeEditor } from "@/components/CodeEditor";
import { PathActions } from "@/components/PathActions";
import { Button } from "@/components/ui/button";
import {
  editorDirty,
  editorSessionReducer,
  emptyEditorSession,
  resolveEditorNavigation,
} from "@/lib/editorSession";
import { describeError } from "@/lib/errors";
import { useRequestPlan } from "@/lib/mutations";
import { queryKeys, useFileText } from "@/lib/queries";
import { rpc } from "@/lib/rpc";
import type { BackupInfo } from "@/lib/types";
import { useUiStore } from "@/state/uiStore";
import { useUnsavedChangesStore } from "@/state/unsavedChangesStore";

export function RawFileEditor({
  path,
  readOnly = false,
  readOnlyHint,
}: {
  path: string;
  readOnly?: boolean;
  readOnlyHint?: string;
}) {
  const file = useFileText(path);
  const [session, dispatch] = useReducer(editorSessionReducer, emptyEditorSession);
  const { draft, base } = session;
  const externallyChanged = useUiStore((s) => s.externallyChanged[path]);
  const clearExternallyChanged = useUiStore((s) => s.clearExternallyChanged);
  const requestPlan = useRequestPlan();
  const [showBackups, setShowBackups] = useState(false);
  const [saving, setSaving] = useState(false);
  const confirm = useUnsavedChangesStore((s) => s.confirm);
  const setNavigationGuard = useUiStore((s) => s.setNavigationGuard);

  useEffect(() => {
    if (file.data) dispatch({ type: "loaded", file: file.data });
  }, [file.data]);

  const dirty = editorDirty(session);
  const diskChanged = base !== null && file.data !== undefined && base.hash !== file.data.hash;
  const conflict = dirty && diskChanged;
  const stale = !dirty && (diskChanged || externallyChanged !== undefined);

  const save = useCallback(async () => {
    if (readOnly || !base || draft === null || saving) return false;
    if (conflict || file.error) {
      toast.error(conflict ? "Reload the file before saving your changes." : describeError(file.error));
      return false;
    }
    setSaving(true);
    try {
      const result = await requestPlan(() =>
        rpc("preview_save_file_text", { path, text: draft, baseHash: base.hash }),
      );
      if (!result?.newHash) return false;
      dispatch({ type: "saved", text: draft, hash: result.newHash });
      clearExternallyChanged(path);
      return true;
    } finally {
      setSaving(false);
    }
  }, [base, draft, saving, conflict, file.error, requestPlan, path, clearExternallyChanged, readOnly]);

  const reload = useCallback(() => {
    dispatch({ type: "reload", file: file.error ? undefined : file.data });
    clearExternallyChanged(path);
  }, [file.data, file.error, clearExternallyChanged, path]);

  useEffect(() => {
    const guard = async () => {
      if (saving) return false;
      if (!dirty) return true;
      return resolveEditorNavigation(await confirm(path), save, reload);
    };
    setNavigationGuard(guard);
    return () => setNavigationGuard(null);
  }, [confirm, dirty, path, saving, save, reload, setNavigationGuard]);

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-col gap-2">
      <div className="shrink-0 space-y-1">
        <div className="flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground" title={path}>
            {path}
          </span>
          <PathActions path={path} size="icon" />
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            className="h-7 gap-1 text-xs"
            title="Backups made by this app"
            onClick={() => setShowBackups((v) => !v)}
          >
            <History className="h-3.5 w-3.5" /> Backups
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="ml-auto h-7 gap-1 text-xs"
            disabled={!dirty || saving}
            onClick={reload}
          >
            <RotateCcw className="h-3.5 w-3.5" /> Discard
          </Button>
          <Button
            size="sm"
            className="h-7 gap-1 text-xs"
            disabled={readOnly || !dirty || saving || conflict || !!file.error}
            onClick={() => void save()}
          >
            <Save className="h-3.5 w-3.5" /> {saving ? "Saving…" : "Save…"}
          </Button>
        </div>
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
          <Button
            variant="outline"
            size="sm"
            className="h-6 text-xs"
            disabled={saving || !!file.error}
            onClick={reload}
          >
            Reload
          </Button>
        </div>
      )}
      {file.error && (
        <div
          role="alert"
          className="shrink-0 rounded border border-destructive/40 p-2 text-xs text-destructive"
        >
          <p>
            {describeError(file.error)}
            {base && " Your editor buffer has been kept."}
          </p>
          <Button
            variant="outline"
            size="sm"
            className="mt-1 h-6 text-xs"
            disabled={file.isFetching}
            onClick={() => void file.refetch()}
          >
            Retry
          </Button>
        </div>
      )}
      {showBackups && <BackupList path={path} readOnly={readOnly} />}
      {base && draft !== null ? (
        <>
          <CodeEditor
            value={draft}
            language={base.language}
            onChange={(text) => dispatch({ type: "edit", text })}
            readOnly={readOnly || saving}
            minHeight="0px"
            className="min-h-0 flex-1"
          />
          <div className="shrink-0 text-[11px] text-muted-foreground">
            mode {base.mode.toString(8)} · {draft.length.toLocaleString()} chars
            {dirty && " · unsaved changes"}
          </div>
        </>
      ) : !file.error ? (
        <p className="p-3 text-sm text-muted-foreground">Loading…</p>
      ) : null}
    </div>
  );
}

function BackupList({ path, readOnly }: { path: string; readOnly: boolean }) {
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
          <Button
            variant="outline"
            size="sm"
            className="h-6 text-xs"
            disabled={readOnly}
            onClick={() => restore(b)}
          >
            Restore…
          </Button>
        </li>
      ))}
    </ul>
  );
}
