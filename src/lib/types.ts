// Hand-mirrored from src-tauri/src/model.rs. Keep the two in sync; the
// snapshot fixture test (`src/lib/snapshot.test.ts`) catches drift.

export type Agent = "claude" | "codex";

export type EntityKind =
  | "skill"
  | "mcpServer"
  | "plugin"
  | "subAgent"
  | "slashCommand"
  | "hook"
  | "automation";

export type Scope =
  | { type: "user" }
  | { type: "project"; root: string }
  | { type: "plugin"; pluginId: string }
  | { type: "system" }
  | { type: "managed" };

export type Locator =
  | { type: "jsonPointer"; pointer: string }
  | { type: "tomlPath"; path: string[] }
  | { type: "tomlArrayItem"; path: string[]; index: number }
  | { type: "markdownFile" }
  | { type: "directory" };

export interface Origin {
  agent: Agent;
  scope: Scope;
  file: string;
  locator: Locator;
}

export interface EnabledState {
  enabled: boolean | null;
  editable: boolean;
  toggle: Origin | null;
  reason: string | null;
}

export type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };
export type JsonMap = Record<string, JsonValue>;

export interface Frontmatter {
  fields: JsonMap;
  raw: string;
  present: boolean;
}

export interface Skill {
  id: string;
  name: string;
  description: string | null;
  agent: Agent;
  scope: Scope;
  origin: Origin;
  dir: string;
  enabled: EnabledState;
  frontmatter: Frontmatter;
  isSystem: boolean;
  lockManaged: boolean;
  contentHash: string;
  dupGroup: string | null;
  bodyPreview: string;
}

export type McpTransport =
  | { type: "stdio"; command: string; args: string[]; env: JsonMap; cwd: string | null }
  | { type: "http"; url: string; headers: JsonMap; bearerTokenEnvVar: string | null }
  | { type: "sse"; url: string; headers: JsonMap };

export interface CodexMcpRuntime {
  enabled: boolean;
  disabledReason: string | null;
  authStatus: string | null;
  transportType: string | null;
}

export interface McpServer {
  id: string;
  name: string;
  agent: Agent;
  scope: Scope;
  origin: Origin;
  transport: McpTransport;
  enabled: EnabledState;
  startupTimeoutSec: number | null;
  toolTimeoutSec: number | null;
  fromPlugin: string | null;
  extra: JsonMap;
  runtime: CodexMcpRuntime | null;
  needsAuth: boolean;
  dupGroup: string | null;
}

export interface PluginContributions {
  skills: string[];
  agents: string[];
  commands: string[];
  mcpServers: string[];
}

export interface Plugin {
  id: string;
  name: string;
  marketplace: string;
  agent: Agent;
  scope: Scope;
  enabled: EnabledState;
  version: string | null;
  installPath: string | null;
  description: string | null;
  author: string | null;
  homepage: string | null;
  unresolvedMarketplace: boolean;
  installed: boolean;
  inUse: boolean;
  staleDirs: string[];
  contributions: PluginContributions;
}

export interface SubAgent {
  id: string;
  name: string;
  agent: Agent;
  scope: Scope;
  origin: Origin;
  description: string | null;
  model: string | null;
  tools: string[];
  color: string | null;
  effort: string | null;
  frontmatter: Frontmatter;
  bodyPreview: string;
  contentHash: string;
  dupGroup: string | null;
}

export interface SlashCommand {
  id: string;
  name: string;
  agent: Agent;
  scope: Scope;
  origin: Origin;
  description: string | null;
  argumentHint: string | null;
  allowedTools: string[];
  disableModelInvocation: boolean;
  frontmatter: Frontmatter;
  bodyPreview: string;
  contentHash: string;
  dupGroup: string | null;
}

export interface HookEntry {
  matcher: string | null;
  hookType: string;
  command: string;
  timeout: number | null;
  origin: Origin;
  trustEntryPresent: boolean | null;
}

export interface HookEvent {
  id: string;
  agent: Agent;
  scope: Scope;
  event: string;
  entries: HookEntry[];
}

export type AutomationTarget =
  | { type: "project"; projectId: string; local: boolean }
  | { type: "projectless" }
  | { type: "other"; raw: JsonValue };

export interface Automation {
  id: string;
  name: string;
  agent: Agent;
  scope: Scope;
  kind: string;
  status: string;
  prompt: string;
  rrule: string | null;
  model: string | null;
  reasoningEffort: string | null;
  executionEnvironment: string | null;
  target: AutomationTarget;
  cwds: string[];
  createdAt: number | null;
  updatedAt: number | null;
  dir: string;
  origin: Origin;
  memoryPath: string | null;
  reportPaths: string[];
  extra: JsonMap;
}

export interface DuplicateGroup {
  key: string;
  kind: EntityKind;
  name: string;
  contentHash: string;
  memberIds: string[];
  projectRoots: string[];
}

export interface ProjectRef {
  root: string;
  trackedBy: Agent[];
  exists: boolean;
}

export interface Warning {
  file: string | null;
  message: string;
}

export interface HomesInfo {
  home: string;
  claudeDir: string;
  claudeState: string;
  codexHome: string;
  agentsDir: string;
  sandbox: boolean;
  readOnly: boolean;
}

export interface Snapshot {
  generatedAt: string;
  homes: HomesInfo | null;
  skills: Skill[];
  mcpServers: McpServer[];
  plugins: Plugin[];
  subagents: SubAgent[];
  commands: SlashCommand[];
  hooks: HookEvent[];
  automations: Automation[];
  dupGroups: DuplicateGroup[];
  projects: ProjectRef[];
  warnings: Warning[];
  fileHashes: Record<string, string>;
  scanMillis: number;
}

export type Language = "json" | "toml" | "markdown" | "yaml" | "text";

export interface FileText {
  path: string;
  text: string;
  hash: string;
  mtimeMs: number;
  mode: number;
  language: Language;
}

export interface FrontmatterChange {
  key: string;
  value: JsonValue | null;
}

export type Mutation =
  | { type: "jsonSet"; pointer: string; value: JsonValue }
  | { type: "jsonDelete"; pointer: string }
  | { type: "tomlSet"; path: string[]; value: JsonValue }
  | { type: "tomlDelete"; path: string[] }
  | { type: "tomlSkillsConfig"; skillPath: string; skillName: string | null; enabled: boolean }
  | { type: "tomlAutomation"; patch: JsonValue }
  | { type: "frontmatter"; changes: FrontmatterChange[]; body: string | null }
  | { type: "rawText" }
  | { type: "createFile" }
  | { type: "deleteFile" };

export interface ExtraFile {
  path: string;
  text: string;
  mode: number | null;
}

export interface WritePlan {
  path: string;
  baseHash: string | null;
  newText: string | null;
  unifiedDiff: string;
  mode: number | null;
  warnings: string[];
  extraFiles: ExtraFile[];
  mutation: Mutation;
  description: string;
}

export interface ApplyResult {
  path: string;
  newHash: string | null;
  backupPath: string | null;
}

export interface BackupInfo {
  id: string;
  path: string;
  originalPath: string;
  createdAt: string;
  size: number;
}

export interface AppSettings {
  editorCommand: string | null;
  codexBinary: string | null;
  backupLimit: number;
  extraSkillRoots: string[];
  theme: "system" | "light" | "dark";
  showSystemSkills: boolean;
  showAgentsDirSkills: boolean;
  showMissingProjects: boolean;
}

export interface DirEntryInfo {
  path: string;
  name: string;
  isDir: boolean;
  size: number;
  mtimeMs: number;
}

export type ErrorCode =
  | "io"
  | "parse"
  | "externalChange"
  | "readOnly"
  | "notFound"
  | "invalidInput"
  | "conflict";

export interface AppError {
  code: ErrorCode;
  message: string;
  path?: string;
}

export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    typeof (value as { message: unknown }).message === "string"
  );
}
