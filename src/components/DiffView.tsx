import { cn } from "@/lib/utils";

/** Renders a unified diff with simple add/remove colouring. */
export function DiffView({ diff, className }: { diff: string; className?: string }) {
  const lines = diff.split("\n");
  if (diff.trim() === "") {
    return <p className={cn("text-sm text-muted-foreground", className)}>No textual changes.</p>;
  }
  return (
    <pre
      className={cn("overflow-auto rounded-md border bg-muted/30 p-2 font-mono text-xs leading-5", className)}
    >
      {lines.map((line, i) => {
        const cls =
          line.startsWith("+++") || line.startsWith("---")
            ? "font-semibold text-muted-foreground"
            : line.startsWith("@@")
              ? "diff-line-hunk"
              : line.startsWith("+")
                ? "diff-line-add"
                : line.startsWith("-")
                  ? "diff-line-del"
                  : "";
        return (
          <div key={`${i}:${line.slice(0, 40)}`} className={cn("whitespace-pre-wrap break-all px-1", cls)}>
            {line || " "}
          </div>
        );
      })}
    </pre>
  );
}
