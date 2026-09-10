import { useEffect, useState } from "react";
import { FieldRow } from "@/components/FieldRow";
import { ListEditor } from "@/components/ListEditor";
import { TopBar } from "@/components/TopBar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useAppSettings, useSaveAppSettings, useSnapshot } from "@/lib/queries";
import type { AppSettings } from "@/lib/types";

export function SettingsScreen() {
  const settings = useAppSettings();
  const save = useSaveAppSettings();
  const snapshot = useSnapshot();
  const [draft, setDraft] = useState<AppSettings | null>(null);
  useEffect(() => {
    if (settings.data) setDraft(settings.data);
  }, [settings.data]);
  if (!draft) return null;
  const set = <K extends keyof AppSettings>(k: K, v: AppSettings[K]) =>
    setDraft((d) => (d ? { ...d, [k]: v } : d));
  const dirty = JSON.stringify(draft) !== JSON.stringify(settings.data);
  const homes = snapshot.data?.homes;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <TopBar />
      <div className="min-h-0 flex-1 space-y-4 overflow-auto p-4">
        <FieldRow label="Theme">
          <Select
            value={draft.theme}
            onChange={(e) => set("theme", e.target.value as AppSettings["theme"])}
            className="h-8 w-40 text-sm"
          >
            <option value="system">System</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </Select>
        </FieldRow>
        <FieldRow
          label="External editor"
          help="Command template. {path} and optional {line} are substituted."
        >
          <Input
            value={draft.editorCommand ?? ""}
            onChange={(e) => set("editorCommand", e.target.value || null)}
            className="h-8 font-mono text-sm"
            placeholder="code --goto {path}:{line}"
          />
        </FieldRow>
        <FieldRow label="Codex binary" help="Used for `codex mcp list --json` status. Avoid cmux shims.">
          <Input
            value={draft.codexBinary ?? ""}
            onChange={(e) => set("codexBinary", e.target.value || null)}
            className="h-8 font-mono text-sm"
            placeholder="/opt/homebrew/bin/codex"
          />
        </FieldRow>
        <FieldRow label="Backups per file" help="Copies kept before each write.">
          <Input
            type="number"
            min={1}
            max={500}
            value={draft.backupLimit}
            onChange={(e) => set("backupLimit", Math.max(1, Number(e.target.value) || 1))}
            className="h-8 w-24 text-sm"
          />
        </FieldRow>
        <FieldRow label="Show Codex built-in skills">
          <div className="pt-1.5">
            <Switch checked={draft.showSystemSkills} onCheckedChange={(v) => set("showSystemSkills", v)} />
          </div>
        </FieldRow>
        <FieldRow
          label="Show ~/.agents skills"
          help="Cross-agent skills dir read by Codex, not by Claude Code."
        >
          <div className="pt-1.5">
            <Switch
              checked={draft.showAgentsDirSkills}
              onCheckedChange={(v) => set("showAgentsDirSkills", v)}
            />
          </div>
        </FieldRow>
        <FieldRow label="Show missing projects">
          <div className="pt-1.5">
            <Switch
              checked={draft.showMissingProjects}
              onCheckedChange={(v) => set("showMissingProjects", v)}
            />
          </div>
        </FieldRow>
        <FieldRow label="Extra skill roots" help="Reserved for a future release; not scanned yet.">
          <ListEditor
            items={draft.extraSkillRoots}
            onChange={(v) => set("extraSkillRoots", v)}
            placeholder="/path/to/skills"
          />
        </FieldRow>
        <div className="flex justify-end gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={!dirty}
            onClick={() => settings.data && setDraft(settings.data)}
          >
            Reset
          </Button>
          <Button size="sm" disabled={!dirty || save.isPending} onClick={() => save.mutate(draft)}>
            Save settings
          </Button>
        </div>
        {homes && (
          <div className="rounded border bg-muted/30 p-3 text-xs text-muted-foreground">
            <div className="mb-1 font-medium text-foreground">Resolved locations</div>
            <div className="grid grid-cols-[120px_1fr] gap-x-3 gap-y-0.5 font-mono">
              <span>home</span>
              <span>{homes.home}</span>
              <span>claude dir</span>
              <span>{homes.claudeDir}</span>
              <span>claude state</span>
              <span>{homes.claudeState}</span>
              <span>codex home</span>
              <span>{homes.codexHome}</span>
              <span>agents dir</span>
              <span>{homes.agentsDir}</span>
            </div>
            {homes.sandbox && <p className="mt-2 text-destructive">Sandbox mode (ACE_HOME is set).</p>}
          </div>
        )}
      </div>
    </div>
  );
}
