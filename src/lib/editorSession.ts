import type { FileText } from "./types";

export interface EditorSession {
  base: FileText | null;
  draft: string | null;
}

export const emptyEditorSession: EditorSession = { base: null, draft: null };

export type EditorAction =
  | { type: "loaded"; file: FileText }
  | { type: "edit"; text: string }
  | { type: "reload"; file?: FileText }
  | { type: "saved"; text: string; hash: string | null };

export function editorDirty(session: EditorSession): boolean {
  return session.base !== null && session.draft !== null && session.draft !== session.base.text;
}

/** The draft is anchored to the last loaded/saved version, even if a refetch fails. */
export function editorSessionReducer(session: EditorSession, action: EditorAction): EditorSession {
  switch (action.type) {
    case "loaded":
      // Keep both clean and dirty buffers on disk changes until an explicit reload.
      if (session.base && session.draft !== action.file.text) return session;
      return { base: action.file, draft: action.file.text };
    case "edit":
      return { ...session, draft: action.text };
    case "reload": {
      const base = action.file ?? session.base;
      return { base, draft: base?.text ?? null };
    }
    case "saved":
      if (!session.base || !action.hash) return session;
      return {
        base: { ...session.base, text: action.text, hash: action.hash },
        draft: action.text,
      };
  }
}

export type UnsavedChoice = "save" | "discard" | "cancel";

export async function resolveEditorNavigation(
  choice: UnsavedChoice,
  save: () => Promise<boolean>,
  discard: () => void,
): Promise<boolean> {
  if (choice === "discard") {
    discard();
    return true;
  }
  if (choice === "save") return save();
  return false;
}
