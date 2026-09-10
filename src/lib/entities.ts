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

/** Directory to show in the Files tab. */
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

export function findEntity(snapshot: Snapshot, id: string): AnyEntity | null {
  for (const kind of KIND_ORDER) {
    const hit = entitiesOfKind(snapshot, kind).find((e) => e.data.id === id);
    if (hit) return hit;
  }
  return null;
}
