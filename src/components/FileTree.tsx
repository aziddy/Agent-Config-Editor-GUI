import { ChevronDown, ChevronRight, File, Folder, FolderOpen } from "lucide-react";
import { type KeyboardEvent, useEffect, useId, useRef, useState } from "react";
import { PathActions } from "@/components/PathActions";
import { Button } from "@/components/ui/button";
import { basename } from "@/lib/entities";
import { describeError } from "@/lib/errors";
import { fileTreeAncestors } from "@/lib/fileTree";
import { useDirListing } from "@/lib/queries";
import { cn } from "@/lib/utils";

interface Props {
  root: string;
  selectedPath: string | null;
  onOpenFile: (path: string) => void;
}

interface TreeState {
  expanded: Set<string>;
  focusedPath: string;
  selectedPath: string | null;
  toggle: (path: string) => void;
  focus: (path: string) => void;
  open: (path: string) => void;
  onKeyDown: (event: KeyboardEvent<HTMLButtonElement>, path: string, directory: boolean) => void;
}

/** Lazy directory branches keep browsing independent from the single selected editor file. */
export function FileTree({ root, selectedPath, onOpenFile }: Props) {
  const normalizedRoot = root.replace(/\/+$/, "") || "/";
  const [expanded, setExpanded] = useState(
    () => new Set([normalizedRoot, ...fileTreeAncestors(normalizedRoot, selectedPath)]),
  );
  const [focusedPath, setFocusedPath] = useState(normalizedRoot);
  const treeRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const ancestors = fileTreeAncestors(normalizedRoot, selectedPath);
    setExpanded((current) => {
      if (ancestors.every((path) => current.has(path))) return current;
      return new Set([...current, ...ancestors]);
    });
  }, [normalizedRoot, selectedPath]);

  const toggle = (path: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, path: string, directory: boolean) => {
    const nodes = Array.from(treeRef.current?.querySelectorAll<HTMLButtonElement>("[role=treeitem]") ?? []);
    const index = nodes.indexOf(event.currentTarget);
    const parent = event.currentTarget.dataset.parentPath;
    let destination: HTMLButtonElement | undefined;
    switch (event.key) {
      case "ArrowDown":
        destination = nodes[index + 1];
        break;
      case "ArrowUp":
        destination = nodes[index - 1];
        break;
      case "Home":
        destination = nodes[0];
        break;
      case "End":
        destination = nodes[nodes.length - 1];
        break;
      case "ArrowRight":
        if (directory && !expanded.has(path)) toggle(path);
        else if (directory && nodes[index + 1]?.dataset.parentPath === path) destination = nodes[index + 1];
        break;
      case "ArrowLeft":
        if (directory && expanded.has(path)) toggle(path);
        else destination = nodes.find((node) => node.dataset.path === parent);
        break;
      default:
        return;
    }
    event.preventDefault();
    destination?.focus();
  };

  const tree: TreeState = {
    expanded,
    focusedPath,
    selectedPath,
    toggle,
    focus: setFocusedPath,
    open: onOpenFile,
    onKeyDown,
  };

  return (
    <div ref={treeRef} role="tree" aria-label="Entity files" className="py-1 text-xs">
      <ul role="none">
        <DirectoryNode
          path={normalizedRoot}
          name={basename(normalizedRoot) || normalizedRoot}
          parentPath={null}
          level={1}
          tree={tree}
        />
      </ul>
    </div>
  );
}

function TreeRow({
  path,
  name,
  parentPath,
  level,
  directory,
  groupId,
  tree,
}: {
  path: string;
  name: string;
  parentPath: string | null;
  level: number;
  directory: boolean;
  groupId?: string;
  tree: TreeState;
}) {
  const selected = path === tree.selectedPath;
  const expanded = tree.expanded.has(path);
  const rowRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (selected) rowRef.current?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [selected]);

  return (
    <div
      className={cn("flex items-center gap-0.5 pr-1", selected && "bg-accent text-accent-foreground")}
      style={{ minWidth: `${180 + (level - 1) * 12}px` }}
    >
      <button
        ref={rowRef}
        type="button"
        role="treeitem"
        aria-level={level}
        aria-expanded={directory ? expanded : undefined}
        aria-owns={directory && expanded ? groupId : undefined}
        aria-selected={selected}
        data-path={path}
        data-parent-path={parentPath ?? undefined}
        tabIndex={tree.focusedPath === path ? 0 : -1}
        className="flex min-w-0 flex-1 items-center gap-1 rounded py-2 pr-1 text-left hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
        style={{ paddingLeft: `${8 + (level - 1) * 12}px` }}
        title={path}
        onFocus={() => tree.focus(path)}
        onKeyDown={(event) => tree.onKeyDown(event, path, directory)}
        onClick={() => (directory ? tree.toggle(path) : tree.open(path))}
      >
        {directory ? (
          <>
            {expanded ? (
              <ChevronDown className="h-3 w-3 shrink-0" />
            ) : (
              <ChevronRight className="h-3 w-3 shrink-0" />
            )}
            {expanded ? (
              <FolderOpen className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            ) : (
              <Folder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            )}
          </>
        ) : (
          <>
            <span className="w-3 shrink-0" />
            <File className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          </>
        )}
        <span className="truncate">{name}</span>
      </button>
      <PathActions path={path} size="icon" />
    </div>
  );
}

function DirectoryNode({
  path,
  name,
  parentPath,
  level,
  tree,
}: {
  path: string;
  name: string;
  parentPath: string | null;
  level: number;
  tree: TreeState;
}) {
  const expanded = tree.expanded.has(path);
  const listing = useDirListing(expanded ? path : null);
  const groupId = useId();
  return (
    <li role="none">
      <TreeRow
        path={path}
        name={name}
        parentPath={parentPath}
        level={level}
        directory
        groupId={groupId}
        tree={tree}
      />
      {expanded && (
        // biome-ignore lint/a11y/useSemanticElements: ARIA tree branches require groups, not form fieldsets
        <ul id={groupId} role="group" aria-label={name}>
          {listing.isPending && (
            <li role="none" className="px-3 py-2 text-muted-foreground">
              Loading files…
            </li>
          )}
          {listing.error && (
            <li role="none" className="space-y-1 px-3 py-2">
              <p className="break-words text-destructive">{describeError(listing.error)}</p>
              <Button
                variant="outline"
                size="sm"
                className="h-6 text-xs"
                onClick={() => void listing.refetch()}
              >
                Retry
              </Button>
            </li>
          )}
          {listing.data?.map((entry) =>
            entry.isDir ? (
              <DirectoryNode
                key={entry.path}
                path={entry.path}
                name={entry.name}
                parentPath={path}
                level={level + 1}
                tree={tree}
              />
            ) : (
              <li key={entry.path} role="none">
                <TreeRow
                  path={entry.path}
                  name={entry.name}
                  parentPath={path}
                  level={level + 1}
                  directory={false}
                  tree={tree}
                />
              </li>
            ),
          )}
          {listing.data?.length === 0 && (
            <li role="none" className="px-3 py-2 text-muted-foreground">
              Empty directory
            </li>
          )}
        </ul>
      )}
    </li>
  );
}
