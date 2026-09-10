import { Save, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { FieldRow } from "@/components/FieldRow";
import { KeyValueEditor, type KV, kvFromRecord, kvToRecord } from "@/components/KeyValueEditor";
import { ListEditor } from "@/components/ListEditor";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import type { McpServerInput } from "@/lib/rpc";
import type { Agent, McpServer } from "@/lib/types";

type TransportKind = "stdio" | "http" | "sse";

interface DraftState {
  transport: TransportKind;
  command: string;
  args: string[];
  env: KV[];
  cwd: string;
  url: string;
  headers: KV[];
  bearerTokenEnvVar: string;
  startupTimeoutSec: string;
  toolTimeoutSec: string;
  enabled: boolean;
}

function fromServer(server: McpServer | null): DraftState {
  const t = server?.transport;
  return {
    transport: t?.type ?? "stdio",
    command: t?.type === "stdio" ? t.command : "",
    args: t?.type === "stdio" ? t.args : [],
    env: t?.type === "stdio" ? kvFromRecord(t.env) : [],
    cwd: t?.type === "stdio" ? (t.cwd ?? "") : "",
    url: t && t.type !== "stdio" ? t.url : "",
    headers: t && t.type !== "stdio" ? kvFromRecord(t.headers) : [],
    bearerTokenEnvVar: t?.type === "http" ? (t.bearerTokenEnvVar ?? "") : "",
    startupTimeoutSec: server?.startupTimeoutSec?.toString() ?? "",
    toolTimeoutSec: server?.toolTimeoutSec?.toString() ?? "",
    enabled: server?.enabled.enabled ?? true,
  };
}

function toInput(d: DraftState, agent: Agent, extra: McpServer["extra"]): McpServerInput {
  const num = (s: string) => (s.trim() === "" ? null : Number(s));
  const transport: McpServerInput["transport"] =
    d.transport === "stdio"
      ? {
          type: "stdio",
          command: d.command.trim(),
          args: d.args.filter((a) => a !== ""),
          env: kvToRecord(d.env),
          cwd: d.cwd.trim() || null,
        }
      : d.transport === "http"
        ? {
            type: "http",
            url: d.url.trim(),
            headers: kvToRecord(d.headers),
            bearerTokenEnvVar: d.bearerTokenEnvVar.trim() || null,
          }
        : { type: "sse", url: d.url.trim(), headers: kvToRecord(d.headers) };
  const passthrough: McpServerInput["extra"] = {};
  for (const [k, v] of Object.entries(extra)) {
    if (k !== "disabledInProjects") passthrough[k] = v;
  }
  return {
    transport,
    enabled: agent === "codex" ? d.enabled : null,
    startupTimeoutSec: agent === "codex" ? num(d.startupTimeoutSec) : null,
    toolTimeoutSec: agent === "codex" ? num(d.toolTimeoutSec) : null,
    extra: passthrough,
  };
}

interface Props {
  agent: Agent;
  server: McpServer | null;
  disabled?: boolean;
  onSave: (input: McpServerInput) => Promise<unknown>;
  onDelete?: () => void;
  saveLabel?: string;
}

export function McpServerForm({ agent, server, disabled, onSave, onDelete, saveLabel }: Props) {
  const [draft, setDraft] = useState<DraftState>(() => fromServer(server));
  const [saving, setSaving] = useState(false);
  useEffect(() => setDraft(fromServer(server)), [server]);
  const set = <K extends keyof DraftState>(k: K, v: DraftState[K]) => setDraft((d) => ({ ...d, [k]: v }));
  const dirty = JSON.stringify(draft) !== JSON.stringify(fromServer(server));
  const valid = draft.transport === "stdio" ? draft.command.trim() !== "" : draft.url.trim() !== "";

  return (
    <div className="space-y-3">
      <FieldRow label="Transport">
        <Select
          value={draft.transport}
          disabled={disabled}
          onChange={(e) => set("transport", e.target.value as TransportKind)}
          className="h-8 text-sm"
        >
          <option value="stdio">stdio (local command)</option>
          <option value="http">http (streamable)</option>
          {agent === "claude" && <option value="sse">sse</option>}
        </Select>
      </FieldRow>
      {draft.transport === "stdio" ? (
        <>
          <FieldRow label="Command">
            <Input
              value={draft.command}
              disabled={disabled}
              placeholder="npx"
              onChange={(e) => set("command", e.target.value)}
              className="h-8 font-mono text-sm"
            />
          </FieldRow>
          <FieldRow label="Arguments">
            <ListEditor
              items={draft.args}
              readOnly={disabled}
              placeholder="-y some-mcp@latest"
              onChange={(a) => set("args", a)}
            />
          </FieldRow>
          <FieldRow label="Environment" help="Values that look like secrets are masked.">
            <KeyValueEditor entries={draft.env} readOnly={disabled} onChange={(env) => set("env", env)} />
          </FieldRow>
          <FieldRow label="Working directory">
            <Input
              value={draft.cwd}
              disabled={disabled}
              placeholder="(inherit)"
              onChange={(e) => set("cwd", e.target.value)}
              className="h-8 font-mono text-sm"
            />
          </FieldRow>
        </>
      ) : (
        <>
          <FieldRow label="URL">
            <Input
              value={draft.url}
              disabled={disabled}
              placeholder="https://example.com/mcp"
              onChange={(e) => set("url", e.target.value)}
              className="h-8 font-mono text-sm"
            />
          </FieldRow>
          {agent === "claude" && (
            <FieldRow label="Headers" help="Use ${VAR} to reference settings env values.">
              <KeyValueEditor
                entries={draft.headers}
                readOnly={disabled}
                keyPlaceholder="Authorization"
                onChange={(h) => set("headers", h)}
              />
            </FieldRow>
          )}
          {agent === "codex" && draft.transport === "http" && (
            <FieldRow
              label="Bearer token env var"
              help="Codex reads the token from this environment variable."
            >
              <Input
                value={draft.bearerTokenEnvVar}
                disabled={disabled}
                placeholder="MY_API_TOKEN"
                onChange={(e) => set("bearerTokenEnvVar", e.target.value)}
                className="h-8 font-mono text-sm"
              />
            </FieldRow>
          )}
        </>
      )}
      {agent === "codex" && (
        <>
          <FieldRow label="Timeouts (s)" help="Startup / per tool call. Blank keeps Codex defaults.">
            <div className="flex gap-2">
              <Input
                value={draft.startupTimeoutSec}
                disabled={disabled}
                placeholder="startup"
                inputMode="numeric"
                onChange={(e) => set("startupTimeoutSec", e.target.value)}
                className="h-8 w-28 text-sm"
              />
              <Input
                value={draft.toolTimeoutSec}
                disabled={disabled}
                placeholder="tool"
                inputMode="numeric"
                onChange={(e) => set("toolTimeoutSec", e.target.value)}
                className="h-8 w-28 text-sm"
              />
            </div>
          </FieldRow>
          <FieldRow label="Enabled">
            <div className="pt-1.5">
              <Switch
                checked={draft.enabled}
                disabled={disabled}
                onCheckedChange={(v) => set("enabled", v)}
              />
            </div>
          </FieldRow>
        </>
      )}
      <div className="flex items-center justify-end gap-2">
        {onDelete && (
          <Button
            variant="ghost"
            size="sm"
            className="mr-auto h-7 gap-1 text-xs text-destructive"
            disabled={disabled}
            onClick={onDelete}
          >
            <Trash2 className="h-3.5 w-3.5" /> Remove server
          </Button>
        )}
        <Button
          variant="outline"
          size="sm"
          className="h-7 text-xs"
          disabled={!dirty || saving}
          onClick={() => setDraft(fromServer(server))}
        >
          Reset
        </Button>
        <Button
          size="sm"
          className="h-7 gap-1 text-xs"
          disabled={disabled || saving || !valid || (!dirty && server !== null)}
          onClick={async () => {
            setSaving(true);
            try {
              await onSave(toInput(draft, agent, server?.extra ?? {}));
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
