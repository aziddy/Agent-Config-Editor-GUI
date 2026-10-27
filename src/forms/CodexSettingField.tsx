import { RotateCcw } from "lucide-react";
import { useId } from "react";
import { FieldRow } from "@/components/FieldRow";
import { ListEditor } from "@/components/ListEditor";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { type CodexSettingField as Field, settingValue } from "@/lib/codexSettings";
import type { JsonMap, JsonValue } from "@/lib/types";

export function CodexSettingField({
  field,
  values,
  unsupported,
  onChange,
}: {
  field: Field;
  values: JsonMap;
  unsupported: string[][];
  onChange: (path: string[], value: JsonValue) => void;
}) {
  const id = useId();
  const value = settingValue(values, field.path);
  const custom = unsupported.some((path) => JSON.stringify(path) === JSON.stringify(field.path));
  const set = (next: JsonValue) => onChange(field.path, next);
  const text = typeof value === "string" ? value : "";
  const options = field.options ?? [];
  const reset = (
    <Button
      size="icon"
      variant="ghost"
      className="h-8 w-8 shrink-0"
      title="Remove this override and use Codex defaults"
      aria-label={`Reset ${field.label} to default`}
      disabled={value === undefined || custom}
      onClick={() => set(null)}
    >
      <RotateCcw className="h-3.5 w-3.5" />
    </Button>
  );
  return (
    <FieldRow label={field.label} help={field.help}>
      {custom ? (
        <p className="py-1.5 text-xs text-muted-foreground">Custom TOML value. Edit it in Codex Config.</p>
      ) : (
        <div className="space-y-1">
          <div className="flex items-start gap-1">
            <div className="min-w-0 flex-1">
              {field.kind === "boolean" ? (
                <Select
                  className="h-8 text-sm"
                  aria-label={field.label}
                  value={value === undefined ? "" : String(value)}
                  onChange={(event) => set(event.target.value === "" ? null : event.target.value === "true")}
                >
                  <option value="">Use default</option>
                  <option value="true">Enabled</option>
                  <option value="false">Disabled</option>
                </Select>
              ) : field.kind === "choice" ? (
                <Select
                  className="h-8 text-sm"
                  aria-label={field.label}
                  value={text}
                  onChange={(event) => set(event.target.value || null)}
                >
                  <option value="">Use default</option>
                  {text && !options.includes(text) && <option value={text}>{text} (current)</option>}
                  {options.map((option) => (
                    <option key={option} value={option}>
                      {option}
                    </option>
                  ))}
                </Select>
              ) : field.kind === "textarea" ? (
                <Textarea
                  aria-label={field.label}
                  rows={8}
                  value={text}
                  onChange={(event) => set(event.target.value || null)}
                  className="min-h-40 font-mono text-xs"
                  placeholder="Use default"
                />
              ) : field.kind === "list" ? (
                <fieldset aria-label={field.label}>
                  <ListEditor
                    items={
                      Array.isArray(value)
                        ? value.filter((item): item is string => typeof item === "string")
                        : []
                    }
                    onChange={set}
                    placeholder="Add an item"
                  />
                  {value === undefined && (
                    <p className="mt-1 text-[11px] text-muted-foreground">Using default</p>
                  )}
                </fieldset>
              ) : (
                <>
                  <Input
                    aria-label={field.label}
                    className="h-8 font-mono text-xs"
                    list={options.length ? id : undefined}
                    placeholder="Use default"
                    value={text}
                    onChange={(event) => set(event.target.value || null)}
                  />
                  {options.length > 0 && (
                    <datalist id={id}>
                      {options.map((option) => (
                        <option key={option} value={option} />
                      ))}
                    </datalist>
                  )}
                </>
              )}
            </div>
            {reset}
          </div>
          {field.kind === "list" &&
            Array.isArray(value) &&
            value.some((item) => typeof item === "string" && !item.trim()) && (
              <p role="alert" className="text-xs text-destructive">
                Fill in or remove empty rows before saving.
              </p>
            )}
        </div>
      )}
    </FieldRow>
  );
}
