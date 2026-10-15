import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { FileTree } from "@/components/FileTree";
import { Button } from "@/components/ui/button";
import {
  clampEditorTreeWidth,
  EDITOR_TREE_MIN_WIDTH,
  editorTreeFits,
  editorTreeMaxWidth,
} from "@/lib/fileTree";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/state/uiStore";

interface Props {
  root: string | null;
  selectedPath: string | null;
  onOpenFile: (path: string) => void;
  children: ReactNode;
}

/** Retains the editor while the tree is folded or the detail pane shows Overview. */
export function FileEditor({ root, selectedPath, onOpenFile, children }: Props) {
  const width = useUiStore((state) => state.editorTreeWidth);
  const setWidth = useUiStore((state) => state.setEditorTreeWidth);
  const collapsed = useUiStore((state) => state.editorTreeCollapsed);
  const toggleTree = useUiStore((state) => state.toggleEditorTree);
  const [availableWidth, setAvailableWidth] = useState<number | undefined>();
  const containerRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ x: number; width: number } | null>(null);
  const treeId = useId();

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const measure = () => {
      // Hidden Overview panels report zero; retain the most recent visible measurement.
      if (element.clientWidth > 0) setAvailableWidth(element.clientWidth);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const compact = availableWidth !== undefined && !editorTreeFits(availableWidth);
  const folded = collapsed || compact;
  const treeWidth = clampEditorTreeWidth(width, availableWidth);
  const maximum = editorTreeMaxWidth(availableWidth);
  const resizeWithKeyboard = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? 50 : 10;
    let next: number;
    switch (event.key) {
      case "ArrowLeft":
        next = treeWidth - step;
        break;
      case "ArrowRight":
        next = treeWidth + step;
        break;
      case "Home":
        next = EDITOR_TREE_MIN_WIDTH;
        break;
      case "End":
        next = maximum;
        break;
      default:
        return;
    }
    event.preventDefault();
    setWidth(clampEditorTreeWidth(next, availableWidth));
  };

  return (
    <div ref={containerRef} className="flex h-full min-h-0 min-w-0 overflow-hidden">
      <aside
        id={treeId}
        aria-label="File browser"
        className={cn("flex min-h-0 shrink-0 flex-col border-r", folded && "items-center")}
        style={{ width: folded ? 36 : treeWidth }}
      >
        <div className={cn("flex h-9 shrink-0 items-center gap-1 border-b px-1", folded && "border-b-0")}>
          {!folded && <span className="min-w-0 flex-1 px-1 text-xs font-medium">Files</span>}
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            aria-label={folded ? "Restore file tree" : "Fold file tree"}
            aria-controls={treeId}
            aria-expanded={!folded}
            disabled={compact}
            title={
              compact
                ? "File tree folded to leave room for the editor. Widen the pane or fold a sidebar to restore it."
                : folded
                  ? "Restore file tree"
                  : "Fold file tree"
            }
            onClick={toggleTree}
          >
            {folded ? <PanelLeftOpen className="h-4 w-4" /> : <PanelLeftClose className="h-4 w-4" />}
          </Button>
        </div>
        <div className={cn("min-h-0 flex-1 overflow-auto", folded && "hidden")}>
          {root ? (
            <FileTree key={root} root={root} selectedPath={selectedPath} onOpenFile={onOpenFile} />
          ) : (
            <p className="p-3 text-xs text-muted-foreground">No directory is available for this item.</p>
          )}
        </div>
      </aside>
      {!folded && (
        // biome-ignore lint/a11y/useSemanticElements: this is an interactive pane divider, not a document rule
        <div
          role="separator"
          tabIndex={0}
          aria-label="Resize file tree"
          aria-orientation="vertical"
          aria-controls={treeId}
          aria-valuemin={EDITOR_TREE_MIN_WIDTH}
          aria-valuemax={maximum}
          aria-valuenow={treeWidth}
          aria-valuetext={`${Math.round(treeWidth)} pixels`}
          className="w-2 shrink-0 cursor-col-resize touch-none bg-border/40 hover:bg-primary/20 focus-visible:bg-primary/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
          onKeyDown={resizeWithKeyboard}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            dragRef.current = { x: event.clientX, width: treeWidth };
            event.currentTarget.setPointerCapture(event.pointerId);
          }}
          onPointerMove={(event) => {
            if (!dragRef.current) return;
            const next = dragRef.current.width + event.clientX - dragRef.current.x;
            setWidth(clampEditorTreeWidth(next, availableWidth));
          }}
          onPointerUp={(event) => {
            dragRef.current = null;
            event.currentTarget.releasePointerCapture(event.pointerId);
          }}
          onLostPointerCapture={() => {
            dragRef.current = null;
          }}
        />
      )}
      <div className="min-h-0 min-w-0 flex-1 overflow-hidden p-3">{children}</div>
    </div>
  );
}
