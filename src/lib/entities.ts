import type {
  Agent,
  Automation,
  EntityKind,
  HookEvent,
  McpServer,
  Plugin,
  Scope,
  Skill,
  SlashCommand,
  Snapshot,
  SubAgent,
} from "./types";

export type AnyEntity =
  | { kind: "skill"; data: Skill }
  | { kind: "mcpServer"; data: McpServer }
  | { kind: "plugin"; data: Plugin }
  | { kind: "subAgent"; data: SubAgent }
  | { kind: "slashCommand"; data: SlashCommand }
  | { kind: "hook"; data: HookEvent }
  | { kind: "automation"; data: Automation };

export const KIND_LABELS: Record<EntityKind, { singular: string; plural: string }> = {
  skill: { singular: "Skill", plural: "Skills" },
  mcpServer: { singular: "MCP server", plural: "MCP Servers" },
  plugin: { singular: "Plugin", plural: "Plugins" },
  subAgent: { singular: "Subagent", plural: "Subagents" },
  slashCommand: { singular: "Command", plural: "Commands" },
  hook: { singular: "Hook", plural: "Hooks" },
  automation: { singular: "Automation", plural: "Automations" },
};

export const KIND_ORDER: EntityKind[] = [
  "skill",
  "mcpServer",
  "plugin",
  "subAgent",
  "slashCommand",
  "hook",
  "automation",
];

/** Frontend browsing state, independent of an entity's on-disk scope. */
export type LocationFilter =
  | { type: "all" }
  | { type: "scope"; scope: Scope["type"] }
  | { type: "project"; root: string };

export function locationFilterLabel(filter: LocationFilter): string {
  if (filter.type === "all") return "All locations";
  if (filter.type === "project") return `Project: ${basename(filter.root) || filter.root}`;
  switch (filter.scope) {
    case "user":
      return "User";
    case "project":
      return "All projects";
    case "plugin":
      return "Plugin-provided";
    case "system":
      return "System";
    case "managed":
      return "Managed";
  }
}

export function matchesLocation(scope: Scope, filter: LocationFilter): boolean {
  switch (filter.type) {
    case "all":
      return true;
    case "scope":
      return scope.type === filter.scope;
    case "project":
      return scope.type === "project" && scope.root === filter.root;
  }
}

export interface ProjectLocation {
  root: string;
  missing: boolean;
}

/** Include entity-only roots and keep missing roots available when they still own config. */
export function projectLocations(snapshot: Snapshot, showMissing: boolean): ProjectLocation[] {
  const projects = new Map(snapshot.projects.map((p) => [p.root, { root: p.root, missing: !p.exists }]));
  const entityRoots = new Set<string>();
  for (const kind of KIND_ORDER) {
    for (const entity of entitiesOfKind(snapshot, kind)) {
      if (entity.data.scope.type !== "project") continue;
      const root = entity.data.scope.root;
      entityRoots.add(root);
      if (!projects.has(root)) projects.set(root, { root, missing: false });
    }
  }
  return [...projects.values()]
    .filter((p) => showMissing || !p.missing || entityRoots.has(p.root))
    .sort((a, b) => basename(a.root).localeCompare(basename(b.root)) || a.root.localeCompare(b.root));
}

export function basename(path: string): string {
  const parts = path.replace(/\/+$/, "").split("/");
  return parts[parts.length - 1] ?? path;
}

export function dirname(path: string): string {
  const i = path.lastIndexOf("/");
  return i > 0 ? path.slice(0, i) : path;
}

export function scopeLabel(scope: Scope): string {
  switch (scope.type) {
    case "user":
      return "User";
    case "project":
      return basename(scope.root);
    case "plugin":
      return scope.pluginId.split("@")[0] ?? scope.pluginId;
    case "system":
      return "System";
    case "managed":
      return "Managed";
  }
}

export function scopeKey(scope: Scope): string {
  switch (scope.type) {
    case "user":
      return "user";
    case "project":
      return `project:${scope.root}`;
    case "plugin":
      return `plugin:${scope.pluginId}`;
    case "system":
      return "system";
    case "managed":
      return "managed";
  }
}

export function scopeRoot(scope: Scope): string | null {
  return scope.type === "project" ? scope.root : null;
}

export function entitiesOfKind(snapshot: Snapshot, kind: EntityKind): AnyEntity[] {
  switch (kind) {
    case "skill":
      return snapshot.skills.map((data) => ({ kind, data }));
    case "mcpServer":
      return snapshot.mcpServers.map((data) => ({ kind, data }));
    case "plugin":
      return snapshot.plugins.map((data) => ({ kind, data }));
    case "subAgent":
      return snapshot.subagents.map((data) => ({ kind, data }));
    case "slashCommand":
      return snapshot.commands.map((data) => ({ kind, data }));
    case "hook":
      return snapshot.hooks.map((data) => ({ kind, data }));
    case "automation":
      return snapshot.automations.map((data) => ({ kind, data }));
  }
}

export function entityAgent(e: AnyEntity): Agent {
  return e.data.agent;
}

export function entityScope(e: AnyEntity): Scope {
  return e.data.scope;
}

export function entityName(e: AnyEntity): string {
  return e.kind === "hook" ? e.data.event : e.data.name;
}

export function entityFile(e: AnyEntity): string | null {
  switch (e.kind) {
    case "plugin":
      return e.data.installPath ? `${e.data.installPath}/.claude-plugin/plugin.json` : null;
    case "hook":
      return e.data.entries[0]?.origin.file ?? null;
    default:
      return e.data.origin.file;
  }
}

/** Root directory for the combined editor's file tree. */
export function entityDir(e: AnyEntity): string | null {
  switch (e.kind) {
    case "skill":
      return e.data.dir;
    case "automation":
      return e.data.dir;
    case "plugin":
      return e.data.installPath;
    case "hook":
      return e.data.entries[0] ? dirname(e.data.entries[0].origin.file) : null;
    default:
      return dirname(e.data.origin.file);
  }
}

export function entitySearchText(e: AnyEntity): string {
  const parts: string[] = [entityName(e), scopeLabel(entityScope(e)), e.data.agent];
  switch (e.kind) {
    case "skill":
    case "subAgent":
    case "slashCommand":
      parts.push(e.data.description ?? "", e.data.origin.file);
      break;
    case "mcpServer":
      parts.push(
        e.data.transport.type,
        e.data.transport.type === "stdio" ? e.data.transport.command : e.data.transport.url,
        e.data.fromPlugin ?? "",
      );
      break;
    case "plugin":
      parts.push(e.data.id, e.data.description ?? "");
      break;
    case "hook":
      parts.push(...e.data.entries.map((h) => h.command));
      break;
    case "automation":
      parts.push(e.data.status, e.data.prompt.slice(0, 200), e.data.rrule ?? "");
      break;
  }
  return parts.join(" ").toLowerCase();
}

export interface FilterOptions {
  agent: "all" | Agent;
  location: LocationFilter;
  search: string;
  collapseDuplicates: boolean;
  showSystemSkills: boolean;
  showAgentsDirSkills: boolean;
  agentsDir: string | null;
}

/** Duplicate members other than the first of their group are hidden; the survivor gets a count. */
export function filterEntities(
  entities: AnyEntity[],
  opts: FilterOptions,
): { entity: AnyEntity; duplicateCount: number }[] {
  const seenGroups = new Map<string, number>();
  const search = opts.search.trim().toLowerCase();
  const out: { entity: AnyEntity; duplicateCount: number }[] = [];
  for (const e of entities) {
    if (opts.agent !== "all" && e.data.agent !== opts.agent) continue;
    if (!matchesLocation(e.data.scope, opts.location)) continue;
    if (e.kind === "skill" && e.data.isSystem && !opts.showSystemSkills) continue;
    if (
      e.kind === "skill" &&
      !opts.showAgentsDirSkills &&
      opts.agentsDir &&
      e.data.origin.file.startsWith(`${opts.agentsDir}/`)
    ) {
      continue;
    }
    if (search && !entitySearchText(e).includes(search)) continue;
    const group = "dupGroup" in e.data ? e.data.dupGroup : null;
    if (group && opts.collapseDuplicates) {
      const count = seenGroups.get(group);
      if (count !== undefined) {
        seenGroups.set(group, count + 1);
        const survivor = out.find((r) => "dupGroup" in r.entity.data && r.entity.data.dupGroup === group);
        if (survivor) survivor.duplicateCount += 1;
        continue;
      }
      seenGroups.set(group, 1);
    }
    out.push({ entity: e, duplicateCount: 0 });
  }
  return out;
}

/** Sidebar totals share the list's filters, excluding its text search. */
export function countEntitiesByKind(snapshot: Snapshot, opts: FilterOptions): Record<EntityKind, number> {
  const counts: Record<EntityKind, number> = {
    skill: 0,
    mcpServer: 0,
    plugin: 0,
    subAgent: 0,
    slashCommand: 0,
    hook: 0,
    automation: 0,
  };
  for (const kind of KIND_ORDER) {
    counts[kind] = filterEntities(entitiesOfKind(snapshot, kind), { ...opts, search: "" }).length;
  }
  return counts;
}

export function findEntity(snapshot: Snapshot, id: string): AnyEntity | null {
  for (const kind of KIND_ORDER) {
    const hit = entitiesOfKind(snapshot, kind).find((e) => e.data.id === id);
    if (hit) return hit;
  }
  return null;
}
