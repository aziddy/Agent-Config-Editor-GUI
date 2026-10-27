import type { CodexSettingChange, JsonMap, JsonValue } from "./types";

export interface CodexSettingField {
  path: string[];
  label: string;
  kind: "text" | "choice" | "boolean" | "list" | "textarea" | "integer" | "filters" | "notifications";
  options?: string[];
  help?: string;
  min?: number;
  max?: number;
  readOnly?: boolean;
}

const EFFORTS = ["low", "medium", "high", "xhigh", "max", "ultra"];

export const CONTEXT_FIELDS: CodexSettingField[] = [
  { path: ["review_model"], label: "Review model", kind: "text", help: "Model override for /review." },
  { path: ["model_context_window"], label: "Context window (tokens)", kind: "integer", min: 1 },
  {
    path: ["model_auto_compact_token_limit"],
    label: "Auto-compaction threshold (tokens)",
    kind: "integer",
    min: 1,
    help: "Use default to let the model choose its compaction threshold.",
  },
  {
    path: ["model_auto_compact_token_limit_scope"],
    label: "Compaction token scope",
    kind: "choice",
    options: ["total", "body_after_prefix"],
    help: "Count the full context or growth after the carried compaction prefix.",
  },
  { path: ["tool_output_token_limit"], label: "Tool-output budget (tokens)", kind: "integer", min: 0 },
];

export const AGENT_FIELDS: CodexSettingField[] = [
  { path: ["agents", "enabled"], label: "Subagent tools", kind: "boolean" },
  {
    path: ["agents", "max_concurrent_threads_per_session"],
    label: "Concurrent subagents",
    kind: "integer",
    min: 1,
    help: "Maximum open subagent threads, excluding the primary agent.",
  },
  { path: ["agents", "default_subagent_model"], label: "Default subagent model", kind: "text" },
  {
    path: ["agents", "default_subagent_reasoning_effort"],
    label: "Default subagent effort",
    kind: "text",
    options: EFFORTS,
  },
  { path: ["agents", "interrupt_message"], label: "Record interruptions", kind: "boolean" },
];

export const HISTORY_FIELDS: CodexSettingField[] = [
  {
    path: ["history", "persistence"],
    label: "Command history",
    kind: "choice",
    options: ["save-all", "none"],
    help: "Controls history.jsonl. This does not disable other session storage.",
  },
  { path: ["history", "max_bytes"], label: "History size limit (bytes)", kind: "integer", min: 0 },
  { path: ["analytics", "enabled"], label: "Analytics", kind: "boolean" },
  { path: ["feedback", "enabled"], label: "Feedback submission", kind: "boolean" },
];

export const FILTERS_FIELD: CodexSettingField = {
  path: ["shell_environment_policy", "filters"],
  label: "Environment filters",
  kind: "filters",
  help: "Case-insensitive variable patterns. Include rules create an allowlist; exclusions win. Cannot be combined with legacy exclude/include_only arrays.",
};

export const SHELL_FIELDS: CodexSettingField[] = [
  { path: ["allow_login_shell"], label: "Allow login shells", kind: "boolean" },
  {
    path: ["shell_environment_policy", "inherit"],
    label: "Inherit environment",
    kind: "choice",
    options: ["all", "core", "none"],
  },
  {
    path: ["shell_environment_policy", "ignore_default_excludes"],
    label: "Skip automatic secret filtering",
    kind: "boolean",
    help: "Disabled applies automatic exclusions for variable names containing KEY, SECRET, or TOKEN. Codex defaults to enabled.",
  },
  FILTERS_FIELD,
];

export const PROJECT_FIELDS: CodexSettingField[] = [
  { path: ["project_doc_max_bytes"], label: "AGENTS.md budget (bytes)", kind: "integer", min: 0 },
  { path: ["project_doc_fallback_filenames"], label: "Fallback instruction filenames", kind: "list" },
  { path: ["project_root_markers"], label: "Project-root markers", kind: "list" },
  {
    path: ["skills", "max_context_tokens"],
    label: "Skills catalog budget (tokens)",
    kind: "integer",
    min: 1,
    max: 10000,
    help: "Default is 2% of the model context window. Explicit limits are capped at 10,000 tokens.",
  },
];

export const MODEL_FIELDS: CodexSettingField[] = [
  {
    path: ["model"],
    label: "Model",
    kind: "text",
    help: "Model ID. Custom and provider-specific IDs are supported.",
  },
  { path: ["model_provider"], label: "Provider", kind: "text" },
  {
    path: ["service_tier"],
    label: "Service tier",
    kind: "text",
    options: ["fast"],
    help: "Use a tier supported by your model and account.",
  },
  {
    path: ["model_reasoning_effort"],
    label: "Reasoning effort",
    kind: "text",
    options: EFFORTS,
    help: "Supported effort levels depend on the model and client.",
  },
  {
    path: ["plan_mode_reasoning_effort"],
    label: "Plan-mode effort",
    kind: "text",
    options: EFFORTS,
  },
  {
    path: ["model_reasoning_summary"],
    label: "Reasoning summary",
    kind: "choice",
    options: ["auto", "concise", "detailed", "none"],
  },
  { path: ["model_verbosity"], label: "Verbosity", kind: "choice", options: ["low", "medium", "high"] },
];

export const APPROVAL_FIELDS: CodexSettingField[] = [
  { path: ["approval_policy"], label: "Approval policy", kind: "choice", options: ["on-request", "never"] },
  {
    path: ["approvals_reviewer"],
    label: "Approval reviewer",
    kind: "choice",
    options: ["user", "auto_review"],
  },
];

export const SANDBOX_FIELDS: CodexSettingField[] = [
  {
    path: ["sandbox_mode"],
    label: "Sandbox mode",
    kind: "choice",
    options: ["read-only", "workspace-write", "danger-full-access"],
  },
];

export const WORKSPACE_FIELDS: CodexSettingField[] = [
  {
    path: ["sandbox_workspace_write", "network_access"],
    label: "Network access",
    kind: "boolean",
    help: "Outbound network access in the workspace-write sandbox.",
  },
  { path: ["sandbox_workspace_write", "writable_roots"], label: "Writable roots", kind: "list" },
];

export const WEB_FIELD: CodexSettingField = {
  path: ["web_search"],
  label: "Web search",
  kind: "choice",
  options: ["disabled", "cached", "indexed", "live"],
};
export const INSTRUCTIONS_FIELD: CodexSettingField = {
  path: ["developer_instructions"],
  label: "Developer instructions",
  kind: "textarea",
  help: "Additional instructions added before AGENTS.md.",
};
export const MEMORY_FIELDS: CodexSettingField[] = [
  { path: ["memories", "generate_memories"], label: "Generate memories", kind: "boolean" },
  { path: ["memories", "use_memories"], label: "Use memories", kind: "boolean" },
  {
    path: ["memories", "disable_on_external_context"],
    label: "Exclude external-context threads",
    kind: "boolean",
    help: "Skip memory generation for threads using MCP, web search, or tool search.",
  },
  {
    path: ["memories", "max_unused_days"],
    label: "Maximum unused age (days)",
    kind: "integer",
    min: 0,
    max: 365,
  },
  {
    path: ["memories", "max_rollout_age_days"],
    label: "Maximum thread age (days)",
    kind: "integer",
    min: 0,
    max: 90,
  },
  {
    path: ["memories", "max_raw_memories_for_consolidation"],
    label: "Raw memory limit",
    kind: "integer",
    min: 0,
    max: 4096,
  },
  {
    path: ["memories", "max_rollouts_per_startup"],
    label: "Threads per startup",
    kind: "integer",
    min: 0,
    max: 128,
  },
  {
    path: ["memories", "min_rollout_idle_hours"],
    label: "Minimum thread idle time (hours)",
    kind: "integer",
    min: 1,
    max: 48,
  },
  {
    path: ["memories", "min_rate_limit_remaining_percent"],
    label: "Minimum remaining rate limit (%)",
    kind: "integer",
    min: 0,
    max: 100,
  },
  { path: ["memories", "extract_model"], label: "Memory extraction model", kind: "text" },
  { path: ["memories", "consolidation_model"], label: "Memory consolidation model", kind: "text" },
];
export const TERMINAL_FIELDS: CodexSettingField[] = [
  {
    path: ["tui", "status_line"],
    label: "Status-line items",
    kind: "list",
    help: "Ordered item IDs. An explicit empty list hides the footer.",
  },
  { path: ["tui", "theme"], label: "Terminal theme", kind: "text" },
  { path: ["tui", "animations"], label: "Animations", kind: "boolean" },
  {
    path: ["tui", "notifications"],
    label: "Terminal notifications",
    kind: "notifications",
    help: "Enable all notifications or choose event IDs such as agent-turn-complete and approval-requested.",
  },
  {
    path: ["tui", "notification_method"],
    label: "Notification method",
    kind: "choice",
    options: ["auto", "osc9", "bel"],
  },
  {
    path: ["tui", "notification_condition"],
    label: "Notify when",
    kind: "choice",
    options: ["unfocused", "always"],
  },
  { path: ["tui", "vim_mode_default"], label: "Start in Vim normal mode", kind: "boolean" },
  { path: ["tui", "resume_cwd"], label: "Resume directory", kind: "choice", options: ["current", "session"] },
  {
    path: ["tui", "alternate_screen"],
    label: "Alternate screen",
    kind: "choice",
    options: ["auto", "always", "never"],
  },
  { path: ["tui", "auto_recap"], label: "Automatic recaps", kind: "boolean" },
  { path: ["tui", "show_tooltips"], label: "Welcome tooltips", kind: "boolean" },
  {
    path: ["file_opener"],
    label: "Citation file opener",
    kind: "choice",
    options: ["vscode", "vscode-insiders", "windsurf", "cursor", "none"],
  },
  {
    path: ["notify"],
    label: "Notification command",
    kind: "list",
    help: "Executable followed by one argument per row. Editing this does not run the command.",
  },
];

// Official user-config flags. Compatibility-only and ignored switches are not
// suggested as new overrides; existing custom flags remain visible below.
export const FEATURE_FIELDS: CodexSettingField[] = [
  {
    path: ["features", "apps"],
    label: "App integrations",
    kind: "boolean",
    help: "Stable · default enabled.",
  },
  {
    path: ["features", "goals"],
    label: "Persisted goals",
    kind: "boolean",
    help: "Stable · default enabled. Persist goals and continue automatically.",
  },
  {
    path: ["features", "hooks"],
    label: "Lifecycle hooks",
    kind: "boolean",
    help: "Stable · default enabled.",
  },
  {
    path: ["features", "fast_mode"],
    label: "Fast mode controls",
    kind: "boolean",
    help: "Stable · default enabled. Requires a supported model and account.",
  },
  {
    path: ["features", "ultrafast_mode"],
    label: "Ultrafast mode controls",
    kind: "boolean",
    help: "Stable · default enabled. Requires a supported model and account.",
  },
  {
    path: ["features", "multi_agent"],
    label: "Multi-agent collaboration",
    kind: "boolean",
    help: "Stable · default enabled.",
  },
  {
    path: ["features", "remote_plugin"],
    label: "Remote plugin catalog",
    kind: "boolean",
    help: "Stable · default enabled.",
  },
  {
    path: ["features", "shell_snapshot"],
    label: "Shell snapshots",
    kind: "boolean",
    help: "Stable · default enabled. Speed up repeated commands.",
  },
  {
    path: ["features", "shell_tool"],
    label: "Command execution tools",
    kind: "boolean",
    help: "Stable · default enabled.",
  },
  {
    path: ["features", "enable_request_compression"],
    label: "Request compression",
    kind: "boolean",
    help: "Stable · default enabled. Compress supported streaming requests.",
  },
  {
    path: ["features", "skill_mcp_dependency_install"],
    label: "Skill MCP dependency installation",
    kind: "boolean",
    help: "Stable · default enabled. Allow prompts to install missing MCP dependencies.",
  },
  {
    path: ["features", "memories"],
    label: "Memory feature",
    kind: "boolean",
    help: "Experimental · default disabled.",
  },
  {
    path: ["features", "prevent_idle_sleep"],
    label: "Prevent sleep during turns",
    kind: "boolean",
    help: "Experimental · default disabled.",
  },
  {
    path: ["features", "network_proxy"],
    label: "Sandbox command network proxy",
    kind: "boolean",
    help: "Experimental · default disabled. Applies to sandboxed commands; table policies remain in TOML.",
  },
  {
    path: ["features", "prefer_mxc"],
    label: "Prefer Windows MXC",
    kind: "boolean",
    help: "Windows only · standalone CLI defaults to disabled. Desktop rollout defaults can differ.",
  },
];

export const ALL_FIELDS = [
  ...MODEL_FIELDS,
  ...CONTEXT_FIELDS,
  ...AGENT_FIELDS,
  ...HISTORY_FIELDS,
  ...SHELL_FIELDS,
  ...PROJECT_FIELDS,
  ...APPROVAL_FIELDS,
  ...SANDBOX_FIELDS,
  ...WORKSPACE_FIELDS,
  WEB_FIELD,
  INSTRUCTIONS_FIELD,
  ...MEMORY_FIELDS,
  ...TERMINAL_FIELDS,
  ...FEATURE_FIELDS,
];

export function featureFields(values: JsonMap, draft: JsonMap, unsupported: string[][]): CodexSettingField[] {
  const names = new Set(FEATURE_FIELDS.map((field) => field.path[1]));
  const extra = new Set([
    ...Object.keys(isJsonMap(values.features) ? values.features : {}),
    ...Object.keys(isJsonMap(draft.features) ? draft.features : {}),
    ...unsupported.filter((path) => path.length === 2 && path[0] === "features").map((path) => path[1]!),
  ]);
  return [
    ...FEATURE_FIELDS,
    ...[...extra]
      .filter((name) => !names.has(name))
      .sort()
      .map(
        (name): CodexSettingField => ({
          path: ["features", name],
          label: name,
          kind: "boolean",
          ...(name === "unified_exec" || name === "personality"
            ? {
                readOnly: true,
                help: "Compatibility setting. Codex ignores user overrides; use TOML to remove it.",
              }
            : {
                help:
                  name.startsWith("web_search") || name === "codex_hooks"
                    ? "Deprecated alias. Prefer the current setting."
                    : "Existing custom flag. Its default and availability depend on your Codex version.",
              }),
        }),
      ),
  ];
}

export interface EnvironmentFilter {
  pattern: string;
  policy: string;
}

export function environmentFilters(value: JsonValue | undefined): EnvironmentFilter[] {
  if (isJsonMap(value))
    return Object.entries(value).map(([pattern, policy]) => ({ pattern, policy: String(policy) }));
  if (Array.isArray(value))
    return value
      .filter(isJsonMap)
      .map((row) => ({ pattern: String(row.pattern ?? ""), policy: String(row.policy ?? "") }));
  return [];
}

export function settingError(field: CodexSettingField, value: JsonValue | undefined): string | undefined {
  if (value === undefined) return;
  if (field.kind === "integer") {
    if (typeof value !== "number" || !Number.isSafeInteger(value)) return "Enter a whole number.";
    if (value < (field.min ?? 0) || value > (field.max ?? Number.MAX_SAFE_INTEGER)) {
      return field.max === undefined
        ? `Enter ${field.min ?? 0} or more.`
        : `Enter a value from ${field.min ?? 0} to ${field.max}.`;
    }
  }
  if (
    (field.kind === "list" || field.kind === "notifications") &&
    Array.isArray(value) &&
    value.some((item) => typeof item !== "string" || !item.trim())
  ) {
    return "Fill in or remove empty rows before saving.";
  }
  if (field.kind === "filters") {
    const rows = environmentFilters(value);
    if (rows.some((row) => !row.pattern.trim() || !["include", "exclude"].includes(row.policy)))
      return "Enter a pattern and choose include or exclude for every filter.";
    if (new Set(rows.map((row) => row.pattern.toLowerCase())).size !== rows.length)
      return "Filter patterns must be unique (case-insensitive).";
  }
}

export function settingsValid(before: JsonMap, values: JsonMap): boolean {
  return ALL_FIELDS.every((field) => {
    const value = settingValue(values, field.path);
    return (
      JSON.stringify(value) === JSON.stringify(settingValue(before, field.path)) ||
      !settingError(field, value)
    );
  });
}

export function isJsonMap(value: JsonValue | undefined): value is JsonMap {
  return value !== undefined && value !== null && typeof value === "object" && !Array.isArray(value);
}

export function settingValue(values: JsonMap, path: string[]): JsonValue | undefined {
  let value: JsonValue | undefined = values;
  for (const key of path) {
    value = isJsonMap(value) && Object.hasOwn(value, key) ? value[key] : undefined;
  }
  return value;
}

export function setSetting(values: JsonMap, path: string[], value: JsonValue): JsonMap {
  const [key, ...rest] = path;
  if (key === undefined) return values;
  if (rest.length > 0) {
    const current = Object.hasOwn(values, key) ? values[key] : undefined;
    return { ...values, [key]: setSetting(isJsonMap(current) ? current : {}, rest, value) };
  }
  const next = { ...values };
  if (value === null) delete next[key];
  else Object.defineProperty(next, key, { value, enumerable: true, configurable: true, writable: true });
  return next;
}

/** Only changed fields are patched; filter maps are atomic so even an empty table can be reset. */
export function codexSettingChanges(before: JsonMap, after: JsonMap): CodexSettingChange[] {
  const changes: CodexSettingChange[] = [];
  const visit = (old: JsonMap, next: JsonMap, parent: string[]) => {
    for (const key of new Set([...Object.keys(old), ...Object.keys(next)])) {
      const previous = Object.hasOwn(old, key) ? old[key] : undefined;
      const value = Object.hasOwn(next, key) ? next[key] : undefined;
      const path = [...parent, key];
      if (JSON.stringify(path) === JSON.stringify(FILTERS_FIELD.path)) {
        const record = (v: JsonValue | undefined) =>
          v === undefined
            ? undefined
            : Object.fromEntries(environmentFilters(v).map((row) => [row.pattern, row.policy]));
        const oldRecord = record(previous);
        const nextRecord = record(value);
        const sorted = (v: Record<string, string> | undefined) =>
          v === undefined ? undefined : Object.entries(v).sort(([a], [b]) => a.localeCompare(b));
        if (JSON.stringify(sorted(oldRecord)) !== JSON.stringify(sorted(nextRecord)))
          changes.push({ path, value: nextRecord ?? null });
      } else if (isJsonMap(previous) || isJsonMap(value)) {
        visit(isJsonMap(previous) ? previous : {}, isJsonMap(value) ? value : {}, path);
      } else if (JSON.stringify(previous) !== JSON.stringify(value)) {
        changes.push({ path, value: value ?? null });
      }
    }
  };
  visit(before, after, []);
  return changes;
}
