import { Eye, EyeOff, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { isSecretKey, maskValue } from "@/lib/mask";

export type KV = { key: string; value: string };

interface Props {
  entries: KV[];
  onChange: (entries: KV[]) => void;
  readOnly?: boolean;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
}

/** Ordered key/value list with secret masking. */
export function KeyValueEditor({ entries, onChange, readOnly, keyPlaceholder, valuePlaceholder }: Props) {
  const [revealed, setRevealed] = useState<Record<number, boolean>>({});
  const update = (i: number, patch: Partial<KV>) =>
    onChange(entries.map((e, j) => (j === i ? { ...e, ...patch } : e)));
  return (
    <div className="space-y-1">
      {entries.map((e, i) => {
        const secret = isSecretKey(e.key);
        const shown = !secret || revealed[i];
        return (
          <div key={`${i}-${e.key}`} className="flex items-center gap-1">
            <Input
              value={e.key}
              readOnly={readOnly}
              placeholder={keyPlaceholder ?? "KEY"}
              onChange={(ev) => update(i, { key: ev.target.value })}
              className="h-7 w-2/5 font-mono text-xs"
            />
            <Input
              value={shown ? e.value : maskValue(e.value)}
              readOnly={readOnly || !shown}
              placeholder={valuePlaceholder ?? "value"}
              onChange={(ev) => update(i, { value: ev.target.value })}
              className="h-7 flex-1 font-mono text-xs"
            />
            {secret && (
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                title={shown ? "Hide value" : "Reveal value"}
                onClick={() => setRevealed((r) => ({ ...r, [i]: !r[i] }))}
              >
                {shown ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
              </Button>
            )}
            {!readOnly && (
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                title="Remove"
                onClick={() => onChange(entries.filter((_, j) => j !== i))}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            )}
          </div>
        );
      })}
      {!readOnly && (
        <Button
          variant="outline"
          size="sm"
          className="h-7 gap-1 text-xs"
          onClick={() => onChange([...entries, { key: "", value: "" }])}
        >
          <Plus className="h-3 w-3" /> Add
        </Button>
      )}
    </div>
  );
}

export function kvFromRecord(rec: Record<string, unknown> | null | undefined): KV[] {
  return Object.entries(rec ?? {}).map(([key, value]) => ({
    key,
    value: typeof value === "string" ? value : JSON.stringify(value),
  }));
}

export function kvToRecord(entries: KV[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const e of entries) {
    if (e.key.trim()) out[e.key.trim()] = e.value;
  }
  return out;
}
