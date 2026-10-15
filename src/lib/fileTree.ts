export const EDITOR_TREE_MIN_WIDTH = 140;
export const EDITOR_TREE_MAX_WIDTH = 360;
export const EDITOR_MIN_WIDTH = 200;
export const EDITOR_DIVIDER_WIDTH = 8;
export const EDITOR_PADDING_WIDTH = 24;

/** Directories to expand to expose a file, without following paths outside the root. */
export function fileTreeAncestors(root: string, file: string | null): string[] {
  if (!file) return [];
  const normalizedRoot = root.replace(/\/+$/, "") || "/";
  const prefix = normalizedRoot === "/" ? "/" : `${normalizedRoot}/`;
  if (!file.startsWith(prefix)) return [];
  const relative = file.slice(prefix.length);
  const parts = relative.split("/").slice(0, -1);
  const ancestors = [normalizedRoot];
  let directory = normalizedRoot;
  for (const part of parts) {
    directory = `${directory === "/" ? "" : directory}/${part}`;
    ancestors.push(directory);
  }
  return ancestors;
}

export function editorTreeMaxWidth(availableWidth?: number): number {
  return availableWidth === undefined
    ? EDITOR_TREE_MAX_WIDTH
    : Math.min(
        EDITOR_TREE_MAX_WIDTH,
        availableWidth - EDITOR_MIN_WIDTH - EDITOR_DIVIDER_WIDTH - EDITOR_PADDING_WIDTH,
      );
}

/** The caller folds the tree when the pane cannot fit its minimum width. */
export function clampEditorTreeWidth(width: number, availableWidth?: number): number {
  return Math.max(EDITOR_TREE_MIN_WIDTH, Math.min(width, editorTreeMaxWidth(availableWidth)));
}

export function editorTreeFits(availableWidth: number): boolean {
  return editorTreeMaxWidth(availableWidth) >= EDITOR_TREE_MIN_WIDTH;
}
