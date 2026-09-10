//! Unified data model shared with the webview. Mirrored by hand in `src/lib/types.ts`.
//! Every type serializes as camelCase JSON; enums that carry data are tagged with `type`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub fn key(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntityKind {
    Skill,
    McpServer,
    Plugin,
    SubAgent,
    SlashCommand,
    Hook,
    Automation,
}

impl EntityKind {
    pub fn key(self) -> &'static str {
        match self {
            EntityKind::Skill => "skill",
            EntityKind::McpServer => "mcp",
            EntityKind::Plugin => "plugin",
            EntityKind::SubAgent => "agent",
            EntityKind::SlashCommand => "command",
            EntityKind::Hook => "hook",
            EntityKind::Automation => "automation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Scope {
    User,
    Project {
        root: String,
    },
    Plugin {
        plugin_id: String,
    },
    /// Built-in / vendor-managed (e.g. Codex `~/.codex/skills/.system`).
    System,
    /// Enterprise managed settings. Not present on this machine, reserved.
    Managed,
}

impl Scope {
    /// Short stable key used inside entity ids.
    pub fn key(&self) -> String {
        match self {
            Scope::User => "user".to_string(),
            Scope::Project { root } => format!("project:{root}"),
            Scope::Plugin { plugin_id } => format!("plugin:{plugin_id}"),
            Scope::System => "system".to_string(),
            Scope::Managed => "managed".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Locator {
    JsonPointer { pointer: String },
    TomlPath { path: Vec<String> },
    TomlArrayItem { path: Vec<String>, index: usize },
    MarkdownFile,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    pub agent: Agent,
    pub scope: Scope,
    pub file: String,
    pub locator: Locator,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnabledState {
    /// `None` when the agent has no notion of enabled/disabled for this entity.
    pub enabled: Option<bool>,
    pub editable: bool,
    /// Where the toggle would be written, when editable.
    pub toggle: Option<Origin>,
    /// Human explanation (why disabled, or why read-only).
    pub reason: Option<String>,
}

impl EnabledState {
    pub fn read_only(reason: impl Into<String>) -> Self {
        Self {
            enabled: None,
            editable: false,
            toggle: None,
            reason: Some(reason.into()),
        }
    }

    pub fn fixed(enabled: bool, reason: Option<String>) -> Self {
        Self {
            enabled: Some(enabled),
            editable: false,
            toggle: None,
            reason,
        }
    }

    pub fn toggleable(enabled: bool, toggle: Origin, reason: Option<String>) -> Self {
        Self {
            enabled: Some(enabled),
            editable: true,
            toggle: Some(toggle),
            reason,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frontmatter {
    /// Parsed YAML fields in original order. Empty when `present` is false.
    pub fields: Map<String, Value>,
    /// Raw text between the `---` fences, without the fences.
    pub raw: String,
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub agent: Agent,
    pub scope: Scope,
    pub origin: Origin,
    /// Directory containing SKILL.md.
    pub dir: String,
    pub enabled: EnabledState,
    pub frontmatter: Frontmatter,
    pub is_system: bool,
    /// Present in `~/.agents/.skill-lock.json` (installer managed).
    pub lock_managed: bool,
    pub content_hash: String,
    pub dup_group: Option<String>,
    pub body_preview: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum McpTransport {
    Stdio {
        command: String,
        args: Vec<String>,
        env: Map<String, Value>,
        cwd: Option<String>,
    },
    Http {
        url: String,
        headers: Map<String, Value>,
        bearer_token_env_var: Option<String>,
    },
    Sse {
        url: String,
        headers: Map<String, Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexMcpRuntime {
    pub enabled: bool,
    pub disabled_reason: Option<String>,
    pub auth_status: Option<String>,
    pub transport_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub agent: Agent,
    pub scope: Scope,
    pub origin: Origin,
    pub transport: McpTransport,
    pub enabled: EnabledState,
    pub startup_timeout_sec: Option<f64>,
    pub tool_timeout_sec: Option<f64>,
    /// Plugin id (`name@marketplace`) when provided by a plugin.
    pub from_plugin: Option<String>,
    /// Keys we do not model, passed through untouched on save.
    pub extra: Map<String, Value>,
    pub runtime: Option<CodexMcpRuntime>,
    pub needs_auth: bool,
    pub dup_group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributions {
    pub skills: Vec<String>,
    pub agents: Vec<String>,
    pub commands: Vec<String>,
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    /// `name@marketplace`.
    pub id: String,
    pub name: String,
    pub marketplace: String,
    pub agent: Agent,
    pub scope: Scope,
    pub enabled: EnabledState,
    pub version: Option<String>,
    pub install_path: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub homepage: Option<String>,
    pub unresolved_marketplace: bool,
    pub installed: bool,
    pub in_use: bool,
    pub stale_dirs: Vec<String>,
    pub contributions: PluginContributions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubAgent {
    pub id: String,
    pub name: String,
    pub agent: Agent,
    pub scope: Scope,
    pub origin: Origin,
    pub description: Option<String>,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub color: Option<String>,
    pub effort: Option<String>,
    pub frontmatter: Frontmatter,
    pub body_preview: String,
    pub content_hash: String,
    pub dup_group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommand {
    pub id: String,
    /// File stem.
    pub name: String,
    pub agent: Agent,
    pub scope: Scope,
    pub origin: Origin,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub allowed_tools: Vec<String>,
    pub disable_model_invocation: bool,
    pub frontmatter: Frontmatter,
    pub body_preview: String,
    pub content_hash: String,
    pub dup_group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookEntry {
    pub matcher: Option<String>,
    pub hook_type: String,
    pub command: String,
    pub timeout: Option<f64>,
    pub origin: Origin,
    /// Codex only: a `[hooks.state."…"]` trust entry exists for this hook.
    pub trust_entry_present: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookEvent {
    pub id: String,
    pub agent: Agent,
    pub scope: Scope,
    pub event: String,
    pub entries: Vec<HookEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AutomationTarget {
    Project {
        project_id: String,
        /// `local-<sha256(path)[..32]>` ids are derived from a local path.
        local: bool,
    },
    Projectless,
    Other {
        raw: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Automation {
    pub id: String,
    pub name: String,
    pub agent: Agent,
    pub scope: Scope,
    pub kind: String,
    /// `ACTIVE` | `PAUSED` | anything else passes through.
    pub status: String,
    pub prompt: String,
    pub rrule: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub execution_environment: Option<String>,
    pub target: AutomationTarget,
    pub cwds: Vec<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub dir: String,
    pub origin: Origin,
    pub memory_path: Option<String>,
    pub report_paths: Vec<String>,
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub key: String,
    pub kind: EntityKind,
    pub name: String,
    pub content_hash: String,
    pub member_ids: Vec<String>,
    pub project_roots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRef {
    pub root: String,
    pub tracked_by: Vec<Agent>,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    pub file: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomesInfo {
    pub home: String,
    pub claude_dir: String,
    pub claude_state: String,
    pub codex_home: String,
    pub agents_dir: String,
    pub sandbox: bool,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub generated_at: String,
    pub homes: Option<HomesInfo>,
    pub skills: Vec<Skill>,
    pub mcp_servers: Vec<McpServer>,
    pub plugins: Vec<Plugin>,
    pub subagents: Vec<SubAgent>,
    pub commands: Vec<SlashCommand>,
    pub hooks: Vec<HookEvent>,
    pub automations: Vec<Automation>,
    pub dup_groups: Vec<DuplicateGroup>,
    pub projects: Vec<ProjectRef>,
    pub warnings: Vec<Warning>,
    /// sha256 of every file the snapshot was built from, keyed by path.
    pub file_hashes: Map<String, Value>,
    pub scan_millis: u64,
}

// ---------------------------------------------------------------------------
// Files, writes, backups
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileText {
    pub path: String,
    pub text: String,
    pub hash: String,
    pub mtime_ms: i64,
    pub mode: u32,
    /// `json` | `toml` | `markdown` | `yaml` | `text`
    pub language: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontmatterChange {
    pub key: String,
    /// `None` deletes the key.
    pub value: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Mutation {
    JsonSet {
        pointer: String,
        value: Value,
    },
    JsonDelete {
        pointer: String,
    },
    TomlSet {
        path: Vec<String>,
        value: Value,
    },
    TomlDelete {
        path: Vec<String>,
    },
    TomlSkillsConfig {
        skill_path: String,
        skill_name: Option<String>,
        enabled: bool,
    },
    TomlAutomation {
        patch: Value,
    },
    Frontmatter {
        changes: Vec<FrontmatterChange>,
        body: Option<String>,
    },
    RawText,
    CreateFile,
    DeleteFile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraFile {
    pub path: String,
    pub text: String,
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WritePlan {
    pub path: String,
    /// Hash of the file when the plan was built. `None` for a new file.
    pub base_hash: Option<String>,
    /// `None` means delete.
    pub new_text: Option<String>,
    pub unified_diff: String,
    pub mode: Option<u32>,
    pub warnings: Vec<String>,
    pub extra_files: Vec<ExtraFile>,
    pub mutation: Mutation,
    /// One-line human description for the confirm dialog.
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub path: String,
    pub new_hash: Option<String>,
    pub backup_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub id: String,
    pub path: String,
    pub original_path: String,
    pub created_at: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// Command template with `{path}` and optional `{line}` placeholders.
    pub editor_command: Option<String>,
    pub codex_binary: Option<String>,
    pub backup_limit: u32,
    pub extra_skill_roots: Vec<String>,
    /// `system` | `light` | `dark`
    pub theme: String,
    pub show_system_skills: bool,
    pub show_agents_dir_skills: bool,
    pub show_missing_projects: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            editor_command: None,
            codex_binary: None,
            backup_limit: 50,
            extra_skill_roots: Vec::new(),
            theme: "system".to_string(),
            show_system_skills: false,
            show_agents_dir_skills: true,
            show_missing_projects: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryInfo {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime_ms: i64,
}
