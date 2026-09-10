import { ExternalLink, FolderOpen } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { describeError } from "@/lib/errors";
import { rpc } from "@/lib/rpc";

export async function revealPath(path: string) {
  try {
    await rpc("reveal_in_file_manager", { path });
  } catch (err) {
    toast.error(describeError(err));
  }
}

export async function openInEditor(path: string, line?: number) {
  try {
    await rpc("open_in_editor", line === undefined ? { path } : { path, line });
  } catch (err) {
    toast.error(describeError(err));
  }
}

export function PathActions({ path, size = "sm" }: { path: string | null; size?: "sm" | "icon" }) {
  if (!path) return null;
  const compact = size === "icon";
  return (
    <div className="flex items-center gap-1">
      <Button
        variant="outline"
        size={compact ? "icon" : "sm"}
        className={compact ? "h-7 w-7" : "h-7 gap-1 text-xs"}
        title={`Reveal in ${navigator.platform.includes("Mac") ? "Finder" : "file manager"}`}
        onClick={() => void revealPath(path)}
      >
        <FolderOpen className="h-3.5 w-3.5" />
        {!compact && "Reveal"}
      </Button>
      <Button
        variant="outline"
        size={compact ? "icon" : "sm"}
        className={compact ? "h-7 w-7" : "h-7 gap-1 text-xs"}
        title="Open in external editor"
        onClick={() => void openInEditor(path)}
      >
        <ExternalLink className="h-3.5 w-3.5" />
        {!compact && "Open"}
      </Button>
    </div>
  );
}
