import { Button } from "@/components/ui/button";

/** Plugin-provided files are managed by the plugin; require an explicit opt-in to edit. */
export function PluginScopeGate({
  pluginId,
  unlocked,
  onUnlock,
}: {
  pluginId: string;
  unlocked: boolean;
  onUnlock: () => void;
}) {
  if (unlocked) return null;
  return (
    <div className="flex items-center justify-between gap-2 rounded border border-amber-500/40 bg-amber-500/10 px-2 py-1.5 text-xs">
      <span>
        Provided by plugin <span className="font-mono">{pluginId}</span>. Changes here are lost when the
        plugin updates.
      </span>
      <Button variant="outline" size="sm" className="h-6 shrink-0 text-xs" onClick={onUnlock}>
        Edit anyway
      </Button>
    </div>
  );
}
