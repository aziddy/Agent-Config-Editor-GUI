import { Plus, RotateCcw, Trash2 } from "lucide-react";
import { useId } from "react";
import { FieldRow } from "@/components/FieldRow";
import { ListEditor } from "@/components/ListEditor";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import {
  environmentFilters,
  type CodexSettingField as Field,
  settingError,
  settingValue,
} from "@/lib/codexSettings";
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
  const custom = unsupported.some((path) => path.every((segment, index) => field.path[index] === segment));
  const set = (next: JsonValue) => onChange(field.path, next);
  const text = typeof value === "string" ? value : "";
  const options = field.options ?? [];
  const error = settingError(field, value);
  const filters = environmentFilters(value);
  const updateFilter = (index: number, patch: { pattern?: string; policy?: string }) =>
    set(filters.map((row, i) => ({ ...row, ...(i === index ? patch : {}) })));
  const list = (
    <fieldset aria-label={`${field.label} items`}>
      <ListEditor
        items={Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : []}
        onChange={set}
        placeholder="Add an item"
        ariaLabel={field.label}
      />
      {value === undefined && <p className="mt-1 text-[11px] text-muted-foreground">Using Codex default</p>}
    </fieldset>
  );
  const reset = (
    <Button
      size="icon"
      variant="ghost"
      className="h-8 w-8 shrink-0"
      title="Remove this override and use Codex defaults"
      aria-label={`Reset ${field.label} to default`}
      disabled={value === undefined || custom || field.readOnly}
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
        <fieldset disabled={field.readOnly} className="min-w-0 space-y-1" title={field.path.join(".")}>
          <div className="flex items-start gap-1">
            <div className="min-w-0 flex-1">
              {field.kind === "boolean" ? (
                <Select
                  className="h-8 text-sm"
                  aria-label={field.label}
                  value={value === undefined ? "" : String(value)}
                  onChange={(event) => set(event.target.value === "" ? null : event.target.value === "true")}
                >
                  <option value="">Use Codex default</option>
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
                  <option value="">Use Codex default</option>
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
                  placeholder="Use Codex default"
                />
              ) : field.kind === "integer" ? (
                <Input
                  aria-label={field.label}
                  aria-invalid={!!error}
                  aria-describedby={error ? `${id}-error` : undefined}
                  inputMode="numeric"
                  className="h-8 font-mono text-xs"
                  placeholder="Use Codex default"
                  value={typeof value === "number" || typeof value === "string" ? value : ""}
                  onChange={(event) => {
                    const raw = event.target.value;
                    set(
                      raw === ""
                        ? null
                        : /^\d+$/.test(raw) && Number.isSafeInteger(Number(raw))
                          ? Number(raw)
                          : raw,
                    );
                  }}
                />
              ) : field.kind === "filters" ? (
                <fieldset aria-label={field.label} className="space-y-1">
                  {filters.map((row, index) => (
                    <div key={index} className="flex items-center gap-1">
                      <Input
                        aria-label={`Environment pattern ${index + 1}`}
                        className="h-8 min-w-0 flex-1 font-mono text-xs"
                        placeholder="e.g. PATH or *_TOKEN"
                        value={row.pattern}
                        onChange={(event) => updateFilter(index, { pattern: event.target.value })}
                      />
                      <Select
                        aria-label={`Environment policy ${index + 1}`}
                        className="h-8 w-24 text-xs"
                        value={row.policy}
                        onChange={(event) => updateFilter(index, { policy: event.target.value })}
                      >
                        <option value="include">include</option>
                        <option value="exclude">exclude</option>
                      </Select>
                      <Button
                        size="icon"
                        variant="ghost"
                        className="h-8 w-8"
                        aria-label={`Remove environment filter ${index + 1}`}
                        onClick={() =>
                          set(filters.filter((_, i) => i !== index).map((entry) => ({ ...entry })))
                        }
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  ))}
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 gap-1 text-xs"
                    onClick={() =>
                      set([...filters.map((row) => ({ ...row })), { pattern: "", policy: "include" }])
                    }
                  >
                    <Plus className="h-3 w-3" /> Add filter
                  </Button>
                  {value === undefined && (
                    <p className="text-[11px] text-muted-foreground">Using Codex default</p>
                  )}
                </fieldset>
              ) : field.kind === "notifications" ? (
                <div className="space-y-2">
                  <Select
                    className="h-8 text-sm"
                    aria-label={field.label}
                    value={value === undefined ? "" : Array.isArray(value) ? "events" : String(value)}
                    onChange={(event) =>
                      set(
                        event.target.value === ""
                          ? null
                          : event.target.value === "events"
                            ? []
                            : event.target.value === "true",
                      )
                    }
                  >
                    <option value="">Use Codex default</option>
                    <option value="true">Enabled for all events</option>
                    <option value="false">Disabled</option>
                    <option value="events">Selected events</option>
                  </Select>
                  {Array.isArray(value) && list}
                </div>
              ) : field.kind === "list" ? (
                list
              ) : (
                <>
                  <Input
                    aria-label={field.label}
                    className="h-8 font-mono text-xs"
                    list={options.length ? id : undefined}
                    placeholder="Use Codex default"
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
          {error && (
            <p id={`${id}-error`} role="alert" className="text-xs text-destructive">
              {error}
            </p>
          )}
        </fieldset>
      )}
    </FieldRow>
  );
}
