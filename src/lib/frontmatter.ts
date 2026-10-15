import type { Agent, Frontmatter, FrontmatterChange, JsonValue } from "./types";

export type FieldControl = "text" | "textarea" | "bool" | "list";

export interface FieldSpec {
  key: string;
  label: string;
  control: FieldControl;
  agents: readonly Agent[];
  frontmatterAgents?: readonly Agent[];
  showBadges?: boolean;
  help?: string;
  placeholder?: string;
}

export type FrontmatterKind = "skill" | "subAgent" | "slashCommand";

export const FIELD_SPECS: Record<FrontmatterKind, FieldSpec[]> = {
  skill: [
    { key: "name", label: "Name", control: "text", agents: ["claude", "codex"], showBadges: false },
    {
      key: "description",
      label: "Description",
      control: "textarea",
      agents: ["claude", "codex"],
      showBadges: false,
      help: "When the agent should use this skill.",
    },
    {
      key: "allowed-tools",
      label: "Allowed tools",
      control: "list",
      agents: ["claude"],
      placeholder: "Bash(git:*)",
    },
    {
      key: "argument-hint",
      label: "Argument hint",
      control: "text",
      agents: ["claude"],
      placeholder: "[issue-number]",
    },
    {
      key: "disable-model-invocation",
      label: "Disable model invocation",
      control: "bool",
      agents: ["claude", "codex"],
      frontmatterAgents: ["claude"],
      help: "Only the user can invoke it with /name.",
    },
    { key: "user-invocable", label: "User invocable", control: "bool", agents: ["claude"] },
    { key: "model", label: "Model", control: "text", agents: ["claude"], placeholder: "sonnet" },
  ],
  subAgent: [
    { key: "name", label: "Name", control: "text", agents: ["claude", "codex"], showBadges: false },
    {
      key: "description",
      label: "Description",
      control: "textarea",
      agents: ["claude", "codex"],
      help: "When Claude should delegate to this agent.",
      showBadges: false,
    },
    {
      key: "model",
      label: "Model",
      control: "text",
      agents: ["claude", "codex"],
      placeholder: "sonnet | opus | haiku | inherit",
    },
    { key: "tools", label: "Tools", control: "list", agents: ["claude"], placeholder: "Read" },
    { key: "color", label: "Color", control: "text", agents: ["claude"], placeholder: "green" },
    { key: "effort", label: "Effort", control: "text", agents: ["claude"], placeholder: "high" },
  ],
  slashCommand: [
    {
      key: "description",
      label: "Description",
      control: "textarea",
      agents: ["claude", "codex"],
      showBadges: false,
    },
    {
      key: "argument-hint",
      label: "Argument hint",
      control: "text",
      agents: ["claude", "codex"],
      placeholder: "[pr-number]",
    },
    {
      key: "allowed-tools",
      label: "Allowed tools",
      control: "list",
      agents: ["claude"],
      placeholder: "Bash(gh:*)",
    },
    { key: "model", label: "Model", control: "text", agents: ["claude"] },
    {
      key: "disable-model-invocation",
      label: "Disable model invocation",
      control: "bool",
      agents: ["claude"],
    },
    {
      key: "hide-from-slash-command-tool",
      label: "Hide from slash-command tool",
      control: "bool",
      agents: ["claude"],
    },
  ],
};

export type Draft = Record<string, JsonValue | undefined>;

/** Normalise a frontmatter value for a control (list fields accept string or array). */
export function draftValue(fm: Frontmatter, spec: FieldSpec): JsonValue | undefined {
  const v = fm.fields[spec.key];
  if (v === undefined) return undefined;
  if (spec.control === "list") return listOf(v);
  if (spec.control === "bool") return typeof v === "boolean" ? v : String(v).toLowerCase() === "true";
  if (v === null) return undefined;
  return typeof v === "string" ? v : typeof v === "number" ? String(v) : v;
}

export function listOf(v: JsonValue | undefined): string[] {
  if (v === undefined || v === null) return [];
  if (Array.isArray(v)) return v.map((i) => (typeof i === "string" ? i : JSON.stringify(i))).filter(Boolean);
  if (typeof v === "string")
    return v
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
  return [String(v)];
}

export function initialDraft(fm: Frontmatter, specs: FieldSpec[]): Draft {
  const d: Draft = {};
  for (const spec of specs) d[spec.key] = draftValue(fm, spec);
  return d;
}

function same(a: JsonValue | undefined, b: JsonValue | undefined): boolean {
  return JSON.stringify(a ?? null) === JSON.stringify(b ?? null);
}

/** Only supported fields can change; empty strings/lists delete the key. */
export function diffDraft(
  fm: Frontmatter,
  specs: FieldSpec[],
  draft: Draft,
  agent: Agent,
): FrontmatterChange[] {
  const changes: FrontmatterChange[] = [];
  for (const spec of specs) {
    if (!(spec.frontmatterAgents ?? spec.agents).includes(agent)) continue;
    const before = draftValue(fm, spec);
    let after = draft[spec.key];
    if (typeof after === "string" && after.trim() === "") after = undefined;
    if (Array.isArray(after) && after.length === 0) after = undefined;
    if (spec.control === "bool" && after === false && before === undefined) after = undefined;
    if (same(before, after)) continue;
    changes.push({ key: spec.key, value: after === undefined ? null : after });
  }
  return changes;
}

/** Keys present in the frontmatter that no control covers. */
export function otherFields(fm: Frontmatter, specs: FieldSpec[]): [string, JsonValue][] {
  const known = new Set(specs.map((s) => s.key));
  return Object.entries(fm.fields).filter(([k]) => !known.has(k));
}
