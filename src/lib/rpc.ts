import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type {
  Agent,
  ApplyResult,
  AppSettings,
  BackupInfo,
  DirEntryInfo,
  FileText,
  FrontmatterChange,
  JsonValue,
  Scope,
  Snapshot,
  WritePlan,
} from "./types";

export interface McpTarget {
  agent: Agent;
  scope: Scope;
  file: string;
  name: string;
}

export interface McpServerInput {
  transport:
    | { type: "stdio"; command: string; args: string[]; env: Record<string, string>; cwd: string | null }
    | { type: "http"; url: string; headers: Record<string, string>; bearerTokenEnvVar: string | null }
    | { type: "sse"; url: string; headers: Record<string, string> };
  enabled: boolean | null;
  startupTimeoutSec: number | null;
  toolTimeoutSec: number | null;
  extra: Record<string, JsonValue>;
}

export interface AutomationPatch {
  name?: string;
  prompt?: string;
  status?: string;
  rrule?: string;
  model?: string | null;
  reasoningEffort?: string | null;
  executionEnvironment?: string | null;
  cwds?: string[];
}

export interface AutomationCreateInput {
  name: string;
  prompt: string;
  rrule: string;
  model: string | null;
  reasoningEffort: string | null;
  executionEnvironment: string;
  cwds: string[];
}

export type NewEntityKind = "skill" | "subAgent" | "slashCommand";

/**
 * One entry per Tauri command: the args object it takes and the result it returns.
 * Keep in sync with `src-tauri/src/commands.rs`.
 */
export type AceRpc = {
  scan_all: { args: Record<string, never>; result: Snapshot };
  get_file_text: { args: { path: string }; result: FileText };
  list_dir: { args: { path: string }; result: DirEntryInfo[] };
  codex_mcp_status: { args: { force?: boolean }; result: Record<string, JsonValue>[] };

  get_app_settings: { args: Record<string, never>; result: AppSettings };
  set_app_settings: { args: { settings: AppSettings }; result: AppSettings };

  reveal_in_file_manager: { args: { path: string }; result: null };
  open_in_editor: { args: { path: string; line?: number }; result: null };
  open_url: { args: { url: string }; result: null };

  preview_save_file_text: {
    args: { path: string; text: string; baseHash: string | null };
    result: WritePlan;
  };
  preview_upsert_mcp_server: { args: { target: McpTarget; input: McpServerInput }; result: WritePlan };
  preview_delete_mcp_server: { args: { target: McpTarget }; result: WritePlan };
  preview_set_mcp_enabled: {
    args: { target: McpTarget; enabled: boolean; projectRoot?: string | null };
    result: WritePlan;
  };
  preview_set_plugin_enabled: {
    args: { agent: Agent; pluginId: string; scope: Scope; enabled: boolean };
    result: WritePlan;
  };
  preview_set_skill_enabled: { args: { skillId: string; enabled: boolean }; result: WritePlan };
  preview_save_frontmatter: {
    args: { path: string; changes: FrontmatterChange[]; body?: string | null; baseHash: string | null };
    result: WritePlan;
  };
  preview_save_automation: { args: { id: string; patch: AutomationPatch }; result: WritePlan };
  preview_set_automation_status: { args: { id: string; status: string }; result: WritePlan };
  preview_create_automation: { args: { input: AutomationCreateInput }; result: WritePlan };
  preview_create_entity: {
    args: { kind: NewEntityKind; agent: Agent; scope: Scope; name: string };
    result: WritePlan;
  };
  preview_delete_path: { args: { path: string }; result: WritePlan };
  apply_write_plan: { args: { plan: WritePlan }; result: ApplyResult };

  list_backups: { args: { path: string }; result: BackupInfo[] };
  read_backup: { args: { id: string }; result: string };
  preview_restore_backup: { args: { id: string }; result: WritePlan };
};

export type RpcOp = keyof AceRpc;

export async function rpc<K extends RpcOp>(op: K, args: AceRpc[K]["args"]): Promise<AceRpc[K]["result"]> {
  return tauriInvoke<AceRpc[K]["result"]>(op, args);
}
