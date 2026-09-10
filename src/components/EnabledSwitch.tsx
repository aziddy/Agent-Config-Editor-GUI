import { Switch } from "@/components/ui/switch";
import type { EnabledState } from "@/lib/types";

interface Props {
  state: EnabledState;
  onChange?: (enabled: boolean) => void;
  busy?: boolean;
}

/** Enabled toggle that explains itself when read-only. */
export function EnabledSwitch({ state, onChange, busy }: Props) {
  if (state.enabled === null) {
    return (
      <span
        className="text-[10px] uppercase tracking-wide text-muted-foreground"
        title={state.reason ?? undefined}
      >
        n/a
      </span>
    );
  }
  return (
    <Switch
      checked={state.enabled}
      disabled={!state.editable || busy || !onChange}
      title={state.reason ?? (state.enabled ? "Enabled" : "Disabled")}
      aria-label={state.enabled ? "Enabled" : "Disabled"}
      onCheckedChange={(v) => onChange?.(v)}
    />
  );
}
