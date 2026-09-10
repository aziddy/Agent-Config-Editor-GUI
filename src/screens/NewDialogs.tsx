import { useState } from "react";
import { FieldRow } from "@/components/FieldRow";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { AutomationForm, createInputFromDraft } from "@/forms/AutomationForm";
import { McpServerForm } from "@/forms/McpServerForm";
import { basename, KIND_LABELS } from "@/lib/entities";
import { useRequestPlan } from "@/lib/mutations";
import { useSnapshot } from "@/lib/queries";
import { type NewEntityKind, rpc } from "@/lib/rpc";
import type { Agent, Scope } from "@/lib/types";
import { useUiStore } from "@/state/uiStore";

function useScopeOptions(agent: Agent) {
  const snapshot = useSnapshot();
  const projects = (snapshot.data?.projects ?? []).filter((p) => p.exists && p.trackedBy.includes(agent));
  return projects;
}

function ScopePicker({
  agent,
  value,
  onChange,
}: {
  agent: Agent;
  value: string;
  onChange: (v: string) => void;
}) {
  const projects = useScopeOptions(agent);
  return (
    <Select value={value} onChange={(e) => onChange(e.target.value)} className="h-8 text-sm">
      <option value="user">User (global)</option>
      {projects.map((p) => (
        <option key={p.root} value={p.root}>
          Project: {basename(p.root)}
        </option>
      ))}
    </Select>
  );
}

function scopeFromValue(v: string): Scope {
  return v === "user" ? { type: "user" } : { type: "project", root: v };
}

export function NewEntityDialog({ kind, onClose }: { kind: NewEntityKind; onClose: () => void }) {
  const requestPlan = useRequestPlan();
  const agentFilter = useUiStore((s) => s.agentFilter);
  const [agent, setAgent] = useState<Agent>(agentFilter === "codex" ? "codex" : "claude");
  const [scope, setScope] = useState("user");
  const [name, setName] = useState("");
  const label = KIND_LABELS[kind].singular.toLowerCase();
  const codexUnsupported = agent === "codex" && kind === "slashCommand";
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title={`New ${label}`} className="max-w-lg">
      <div className="space-y-3 pb-3">
        <FieldRow label="Agent">
          <Select
            value={agent}
            onChange={(e) => {
              setAgent(e.target.value as Agent);
              setScope("user");
            }}
            className="h-8 text-sm"
          >
            <option value="claude">Claude Code</option>
            <option value="codex">Codex</option>
          </Select>
        </FieldRow>
        <FieldRow label="Scope">
          <ScopePicker agent={agent} value={scope} onChange={setScope} />
        </FieldRow>
        <FieldRow label="Name" help="Becomes the folder or file name (kebab-case).">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="h-8 text-sm"
            placeholder="my-skill"
            autoFocus
          />
        </FieldRow>
        {codexUnsupported && (
          <p className="text-xs text-destructive">
            Codex has no user-level slash commands; they ship inside plugins.
          </p>
        )}
        <div className="flex justify-end gap-2">
          <Button variant="outline" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button
            size="sm"
            disabled={name.trim() === "" || codexUnsupported}
            onClick={async () => {
              const r = await requestPlan(() =>
                rpc("preview_create_entity", {
                  kind,
                  agent,
                  scope: scopeFromValue(scope),
                  name: name.trim(),
                }),
              );
              if (r) onClose();
            }}
          >
            Create…
          </Button>
        </div>
      </div>
    </Dialog>
  );
}

export function NewMcpDialog({ onClose }: { onClose: () => void }) {
  const requestPlan = useRequestPlan();
  const snapshot = useSnapshot();
  const agentFilter = useUiStore((s) => s.agentFilter);
  const [agent, setAgent] = useState<Agent>(agentFilter === "codex" ? "codex" : "claude");
  const [scope, setScope] = useState("user");
  const [name, setName] = useState("");
  const homes = snapshot.data?.homes;
  const file =
    agent === "codex"
      ? `${homes?.codexHome ?? ""}/config.toml`
      : scope === "user"
        ? (homes?.claudeState ?? "")
        : `${scope}/.mcp.json`;
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title="New MCP server"
      description={file}
      className="max-w-2xl"
    >
      <div className="space-y-3 pb-3">
        <FieldRow label="Agent">
          <Select
            value={agent}
            onChange={(e) => {
              setAgent(e.target.value as Agent);
              setScope("user");
            }}
            className="h-8 text-sm"
          >
            <option value="claude">Claude Code</option>
            <option value="codex">Codex</option>
          </Select>
        </FieldRow>
        {agent === "claude" && (
          <FieldRow
            label="Scope"
            help="User servers live in ~/.claude.json; project servers in <repo>/.mcp.json."
          >
            <ScopePicker agent={agent} value={scope} onChange={setScope} />
          </FieldRow>
        )}
        <FieldRow label="Name">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="h-8 font-mono text-sm"
            placeholder="my-server"
            autoFocus
          />
        </FieldRow>
        <McpServerForm
          agent={agent}
          server={null}
          disabled={name.trim() === ""}
          saveLabel="Create…"
          onSave={async (input) => {
            const r = await requestPlan(() =>
              rpc("preview_upsert_mcp_server", {
                target: {
                  agent,
                  scope: agent === "codex" ? { type: "user" } : scopeFromValue(scope),
                  file,
                  name: name.trim(),
                },
                input,
              }),
            );
            if (r) onClose();
          }}
        />
      </div>
    </Dialog>
  );
}

export function NewAutomationDialog({ onClose }: { onClose: () => void }) {
  const requestPlan = useRequestPlan();
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="New Codex automation" className="max-w-3xl">
      <div className="max-h-[75vh] overflow-auto pb-3">
        <AutomationForm
          automation={null}
          saveLabel="Create…"
          onSave={async (draft) => {
            const r = await requestPlan(() =>
              rpc("preview_create_automation", { input: createInputFromDraft(draft) }),
            );
            if (r) onClose();
          }}
        />
      </div>
    </Dialog>
  );
}
