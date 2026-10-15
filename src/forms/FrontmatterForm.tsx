import { Save } from "lucide-react";
import { Fragment, type ReactNode, useEffect, useState } from "react";
import { FieldRow } from "@/components/FieldRow";
import { ListEditor } from "@/components/ListEditor";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { type Draft, diffDraft, type FieldSpec, initialDraft, listOf, otherFields } from "@/lib/frontmatter";
import type { Agent, Frontmatter, FrontmatterChange } from "@/lib/types";

interface Props {
  agent: Agent;
  frontmatter: Frontmatter;
  specs: FieldSpec[];
  disabled?: boolean;
  fieldOverrides?: Record<string, ReactNode>;
  onSave: (changes: FrontmatterChange[]) => Promise<unknown>;
}

export function FrontmatterForm({ agent, frontmatter, specs, disabled, fieldOverrides, onSave }: Props) {
  const [draft, setDraft] = useState<Draft>(() => initialDraft(frontmatter, specs));
  const [saving, setSaving] = useState(false);
  // Reset when the underlying file changes.
  useEffect(() => setDraft(initialDraft(frontmatter, specs)), [frontmatter, specs]);

  const changes = diffDraft(frontmatter, specs, draft, agent);
  const others = otherFields(frontmatter, specs);
  const set = (key: string, value: Draft[string]) => setDraft((d) => ({ ...d, [key]: value }));

  return (
    <div className="space-y-3">
      {specs.map((spec) => {
        if (fieldOverrides && spec.key in fieldOverrides) {
          return <Fragment key={spec.key}>{fieldOverrides[spec.key]}</Fragment>;
        }
        const v = draft[spec.key];
        const supported = (spec.frontmatterAgents ?? spec.agents).includes(agent);
        const fieldDisabled = disabled || !supported;
        const help = supported
          ? spec.help
          : `${spec.agents.map((a) => (a === "claude" ? "Claude Code" : "Codex")).join(" and ")} only; not used by ${agent === "claude" ? "Claude Code" : "Codex"}.`;
        return (
          <FieldRow
            key={spec.key}
            label={spec.label}
            help={help}
            agents={spec.showBadges === false ? undefined : spec.agents}
          >
            {spec.control === "text" && (
              <Input
                value={typeof v === "string" ? v : v === undefined ? "" : String(v)}
                placeholder={spec.placeholder}
                disabled={fieldDisabled}
                onChange={(e) => set(spec.key, e.target.value)}
                className="h-8 text-sm"
              />
            )}
            {spec.control === "textarea" && (
              <Textarea
                value={typeof v === "string" ? v : v === undefined ? "" : String(v)}
                placeholder={spec.placeholder}
                disabled={fieldDisabled}
                rows={Math.min(12, Math.max(3, String(v ?? "").split("\n").length + 1))}
                onChange={(e) => set(spec.key, e.target.value)}
                className="text-sm"
              />
            )}
            {spec.control === "bool" && (
              <div className="pt-1.5">
                <Switch
                  checked={v === true}
                  disabled={fieldDisabled}
                  onCheckedChange={(c) => set(spec.key, c)}
                />
              </div>
            )}
            {spec.control === "list" && (
              <ListEditor
                items={listOf(v)}
                placeholder={spec.placeholder}
                readOnly={fieldDisabled}
                onChange={(items) => set(spec.key, items)}
              />
            )}
          </FieldRow>
        );
      })}
      {others.length > 0 && (
        <FieldRow label="Other fields" help="Kept as-is; edit in the Raw tab.">
          <pre className="overflow-auto rounded border bg-muted/30 p-2 font-mono text-xs">
            {others.map(([k, v]) => `${k}: ${JSON.stringify(v)}`).join("\n")}
          </pre>
        </FieldRow>
      )}
      <div className="flex justify-end gap-2">
        <Button
          variant="outline"
          size="sm"
          className="h-7 text-xs"
          disabled={changes.length === 0 || saving}
          onClick={() => setDraft(initialDraft(frontmatter, specs))}
        >
          Reset
        </Button>
        <Button
          size="sm"
          className="h-7 gap-1 text-xs"
          disabled={changes.length === 0 || disabled || saving}
          onClick={async () => {
            setSaving(true);
            try {
              await onSave(changes);
            } finally {
              setSaving(false);
            }
          }}
        >
          <Save className="h-3.5 w-3.5" /> Save {changes.length > 0 && `(${changes.length})`}
        </Button>
      </div>
    </div>
  );
}
