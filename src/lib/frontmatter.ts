import type { Frontmatter, FrontmatterChange, JsonValue } from "./types";

export type FieldControl = "text" | "textarea" | "bool" | "list";

export interface FieldSpec {
  key: string;
  label: string;
  control: FieldControl;
  help?: string;
  placeholder?: string;
}

export type FrontmatterKind = "skill" | "subAgent" | "slashCommand";

export const FIELD_SPECS: Record<FrontmatterKind, FieldSpec[]> = {
  skill: [
    { key: "name", label: "Name", control: "text" },
    {
      key: "description",
      label: "Description",
      control: "textarea",
      help: "When the agent should use this skill.",
    },
    { key: "allowed-tools", label: "Allowed tools", control: "list", placeholder: "Bash(git:*)" },
    { key: "argument-hint", label: "Argument hint", control: "text", placeholder: "[issue-number]" },
    {
      key: "disable-model-invocation",
      label: "Disable model invocation",
      control: "bool",
      help: "Only the user can invoke it with /name.",
    },
    { key: "user-invocable", label: "User invocable", control: "bool" },
    { key: "model", label: "Model", control: "text", placeholder: "sonnet" },
  ],
  subAgent: [
    { key: "name", label: "Name", control: "text" },
    {
      key: "description",
      label: "Description",
      control: "textarea",
      help: "When Claude should delegate to this agent.",
    },
    { key: "model", label: "Model", control: "text", placeholder: "sonnet | opus | haiku | inherit" },
    { key: "tools", label: "Tools", control: "list", placeholder: "Read" },
    { key: "color", label: "Color", control: "text", placeholder: "green" },
    { key: "effort", label: "Effort", control: "text", placeholder: "high" },
  ],
  slashCommand: [
    { key: "description", label: "Description", control: "textarea" },
    { key: "argument-hint", label: "Argument hint", control: "text", placeholder: "[pr-number]" },
    { key: "allowed-tools", label: "Allowed tools", control: "list", placeholder: "Bash(gh:*)" },
    { key: "model", label: "Model", control: "text" },
    { key: "disable-model-invocation", label: "Disable model invocation", control: "bool" },
    { key: "hide-from-slash-command-tool", label: "Hide from slash-command tool", control: "bool" },
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

/** Changes between the original frontmatter and the draft; empty strings/lists delete the key. */
export function diffDraft(fm: Frontmatter, specs: FieldSpec[], draft: Draft): FrontmatterChange[] {
  const changes: FrontmatterChange[] = [];
  for (const spec of specs) {
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
