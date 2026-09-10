import { Save } from "lucide-react";
import { useEffect, useState } from "react";
import { CodeEditor } from "@/components/CodeEditor";
import { FieldRow } from "@/components/FieldRow";
import { ListEditor } from "@/components/ListEditor";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { ScheduleBuilder } from "@/forms/ScheduleBuilder";
import type { AutomationCreateInput, AutomationPatch } from "@/lib/rpc";
import type { Automation } from "@/lib/types";

export interface AutomationDraft {
  name: string;
  prompt: string;
  rrule: string;
  model: string;
  reasoningEffort: string;
  executionEnvironment: string;
  cwds: string[];
}

export function draftFromAutomation(a: Automation | null): AutomationDraft {
  return {
    name: a?.name ?? "",
    prompt: a?.prompt ?? "",
    rrule: a?.rrule ?? "RRULE:FREQ=DAILY;BYHOUR=9;BYMINUTE=0",
    model: a?.model ?? "",
    reasoningEffort: a?.reasoningEffort ?? "",
    executionEnvironment: a?.executionEnvironment ?? "worktree",
    cwds: a?.cwds ?? [],
  };
}

export function patchFromDrafts(before: AutomationDraft, after: AutomationDraft): AutomationPatch {
  const patch: AutomationPatch = {};
  if (before.name !== after.name) patch.name = after.name;
  if (before.prompt !== after.prompt) patch.prompt = after.prompt;
  if (before.rrule !== after.rrule) patch.rrule = after.rrule;
  if (before.model !== after.model) patch.model = after.model.trim() || null;
  if (before.reasoningEffort !== after.reasoningEffort)
    patch.reasoningEffort = after.reasoningEffort.trim() || null;
  if (before.executionEnvironment !== after.executionEnvironment)
    patch.executionEnvironment = after.executionEnvironment;
  if (JSON.stringify(before.cwds) !== JSON.stringify(after.cwds))
    patch.cwds = after.cwds.filter((c) => c.trim() !== "");
  return patch;
}

export function createInputFromDraft(d: AutomationDraft): AutomationCreateInput {
  return {
    name: d.name.trim(),
    prompt: d.prompt,
    rrule: d.rrule.trim(),
    model: d.model.trim() || null,
    reasoningEffort: d.reasoningEffort.trim() || null,
    executionEnvironment: d.executionEnvironment,
    cwds: d.cwds.map((c) => c.trim()).filter(Boolean),
  };
}

interface Props {
  automation: Automation | null;
  onSave: (draft: AutomationDraft) => Promise<unknown>;
  saveLabel?: string;
}

export function AutomationForm({ automation, onSave, saveLabel }: Props) {
  const [draft, setDraft] = useState<AutomationDraft>(() => draftFromAutomation(automation));
  const [saving, setSaving] = useState(false);
  useEffect(() => setDraft(draftFromAutomation(automation)), [automation]);
  const set = <K extends keyof AutomationDraft>(k: K, v: AutomationDraft[K]) =>
    setDraft((d) => ({ ...d, [k]: v }));
  const dirty = JSON.stringify(draft) !== JSON.stringify(draftFromAutomation(automation));
  const valid =
    draft.name.trim() !== "" && draft.rrule.trim().startsWith("RRULE:") && draft.prompt.trim() !== "";

  return (
    <div className="space-y-3">
      <FieldRow label="Name">
        <Input
          value={draft.name}
          onChange={(e) => set("name", e.target.value)}
          className="h-8 text-sm"
          placeholder="Weekly stale branches"
        />
      </FieldRow>
      <FieldRow label="Schedule">
        <ScheduleBuilder value={draft.rrule} onChange={(v) => set("rrule", v)} />
      </FieldRow>
      <FieldRow label="Prompt" help="What Codex does on each run.">
        <CodeEditor
          value={draft.prompt}
          language="markdown"
          onChange={(v) => set("prompt", v)}
          minHeight="180px"
        />
      </FieldRow>
      <FieldRow label="Model">
        <Input
          value={draft.model}
          onChange={(e) => set("model", e.target.value)}
          className="h-8 font-mono text-sm"
          placeholder="(default)"
        />
      </FieldRow>
      <FieldRow label="Reasoning effort">
        <Select
          value={draft.reasoningEffort}
          onChange={(e) => set("reasoningEffort", e.target.value)}
          className="h-8 w-40 text-sm"
        >
          <option value="">(default)</option>
          <option value="low">low</option>
          <option value="medium">medium</option>
          <option value="high">high</option>
          <option value="xhigh">xhigh</option>
          <option value="max">max</option>
        </Select>
      </FieldRow>
      <FieldRow
        label="Runs in"
        help="worktree = isolated git worktree; local = the project directory itself."
      >
        <Select
          value={draft.executionEnvironment}
          onChange={(e) => set("executionEnvironment", e.target.value)}
          className="h-8 w-40 text-sm"
        >
          <option value="worktree">worktree</option>
          <option value="local">local</option>
        </Select>
      </FieldRow>
      <FieldRow label="Working directories" help="Absolute paths. Empty means projectless (~).">
        <ListEditor
          items={draft.cwds}
          placeholder="/Users/you/Documents/GitHub/repo"
          onChange={(c) => set("cwds", c)}
        />
      </FieldRow>
      <div className="flex justify-end gap-2">
        <Button
          variant="outline"
          size="sm"
          className="h-7 text-xs"
          disabled={!dirty || saving}
          onClick={() => setDraft(draftFromAutomation(automation))}
        >
          Reset
        </Button>
        <Button
          size="sm"
          className="h-7 gap-1 text-xs"
          disabled={saving || !valid || (!dirty && automation !== null)}
          onClick={async () => {
            setSaving(true);
            try {
              await onSave(draft);
            } finally {
              setSaving(false);
            }
          }}
        >
          <Save className="h-3.5 w-3.5" /> {saveLabel ?? "Save"}
        </Button>
      </div>
    </div>
  );
}
