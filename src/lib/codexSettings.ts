import type { CodexSettingChange, JsonMap, JsonValue } from "./types";

export interface CodexSettingField {
  path: string[];
  label: string;
  kind: "text" | "choice" | "boolean" | "list" | "textarea";
  options?: string[];
  help?: string;
}

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
    options: ["low", "medium", "high", "xhigh", "max", "ultra"],
    help: "Supported effort levels depend on the model and client.",
  },
  {
    path: ["plan_mode_reasoning_effort"],
    label: "Plan-mode effort",
    kind: "text",
    options: ["low", "medium", "high", "xhigh", "max", "ultra"],
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
    path: ["notify"],
    label: "Notification command",
    kind: "list",
    help: "Executable followed by one argument per row. Editing this does not run the command.",
  },
];

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

/** A patch contains only changed leaves; absent values remove explicit overrides. */
export function codexSettingChanges(before: JsonMap, after: JsonMap): CodexSettingChange[] {
  const changes: CodexSettingChange[] = [];
  const visit = (old: JsonMap, next: JsonMap, parent: string[]) => {
    for (const key of new Set([...Object.keys(old), ...Object.keys(next)])) {
      const previous = Object.hasOwn(old, key) ? old[key] : undefined;
      const value = Object.hasOwn(next, key) ? next[key] : undefined;
      const path = [...parent, key];
      if (isJsonMap(previous) || isJsonMap(value)) {
        visit(isJsonMap(previous) ? previous : {}, isJsonMap(value) ? value : {}, path);
      } else if (JSON.stringify(previous) !== JSON.stringify(value)) {
        changes.push({ path, value: value ?? null });
      }
    }
  };
  visit(before, after, []);
  return changes;
}

export function settingsListsValid(before: JsonMap, values: JsonMap): boolean {
  return [...WORKSPACE_FIELDS, ...TERMINAL_FIELDS].every((field) => {
    const value = settingValue(values, field.path);
    if (JSON.stringify(value) === JSON.stringify(settingValue(before, field.path))) return true;
    return (
      field.kind !== "list" ||
      !Array.isArray(value) ||
      value.every((item) => typeof item === "string" && item.trim() !== "")
    );
  });
}
