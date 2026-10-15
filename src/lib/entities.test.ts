import { describe, expect, it } from "vitest";
import {
  type AnyEntity,
  countEntitiesByKind,
  type FilterOptions,
  filterEntities,
  type LocationFilter,
  projectLocations,
  scopeLabel,
} from "./entities";
import type { Scope, Skill, Snapshot } from "./types";

function skill(
  id: string,
  dupGroup: string | null,
  location: string | Scope,
  agent: "claude" | "codex" = "claude",
): Extract<AnyEntity, { kind: "skill" }> {
  const scope: Scope = typeof location === "string" ? { type: "project", root: location } : location;
  const root = scope.type === "project" ? scope.root : "/home/test";
  const data: Skill = {
    id,
    name: "shared",
    description: "d",
    agent,
    scope,
    origin: {
      agent,
      scope,
      file: `${root}/.claude/skills/shared/SKILL.md`,
      locator: { type: "markdownFile" },
    },
    dir: `${root}/.claude/skills/shared`,
    enabled: { enabled: null, editable: false, toggle: null, reason: null },
    frontmatter: { fields: {}, raw: "", present: true },
    isSystem: scope.type === "system",
    lockManaged: false,
    contentHash: "abc",
    dupGroup,
    bodyPreview: "",
  };
  return { kind: "skill", data };
}

function snapshot(overrides: Partial<Snapshot> = {}): Snapshot {
  return {
    generatedAt: "2026-09-22T00:00:00Z",
    homes: null,
    skills: [],
    mcpServers: [],
    plugins: [],
    subagents: [],
    commands: [],
    hooks: [],
    automations: [],
    dupGroups: [],
    projects: [],
    warnings: [],
    fileHashes: {},
    scanMillis: 0,
    ...overrides,
  };
}

const base: FilterOptions = {
  agent: "all",
  location: { type: "all" },
  search: "",
  collapseDuplicates: true,
  showSystemSkills: true,
  showAgentsDirSkills: true,
  agentsDir: null,
};

describe("filterEntities", () => {
  it("collapses duplicate groups into one row with a count", () => {
    const rows = filterEntities([skill("a", "g1", "/r1"), skill("b", "g1", "/r2"), skill("c", null, "/r3")], {
      ...base,
      collapseDuplicates: true,
    });
    expect(rows.map((r) => [r.entity.data.id, r.duplicateCount])).toEqual([
      ["a", 1],
      ["c", 0],
    ]);
  });
  it("filters by agent and search", () => {
    const rows = filterEntities([skill("a", null, "/r1", "claude"), skill("b", null, "/r2", "codex")], {
      ...base,
      collapseDuplicates: false,
      agent: "codex",
      search: "R2",
    });
    expect(rows.map((r) => r.entity.data.id)).toEqual(["b"]);
  });
  it("labels scopes by project basename", () => {
    expect(scopeLabel({ type: "project", root: "/a/b/my-repo" })).toBe("my-repo");
    expect(scopeLabel({ type: "plugin", pluginId: "github@claude-plugins-official" })).toBe("github");
  });

  const scopeCases: { location: LocationFilter; ids: string[] }[] = [
    { location: { type: "all" }, ids: ["user", "project", "plugin", "system", "managed"] },
    ...(["user", "project", "plugin", "system", "managed"] as const).map((scope) => ({
      location: { type: "scope", scope } as const,
      ids: [scope],
    })),
  ];
  it.each(scopeCases)("separates configuration scopes: $location", ({ location, ids }) => {
    const entities = [
      skill("user", null, { type: "user" }),
      skill("project", null, "/work/repo"),
      skill("plugin", null, { type: "plugin", pluginId: "example@market" }),
      skill("system", null, { type: "system" }),
      skill("managed", null, { type: "managed" }),
    ];
    expect(filterEntities(entities, { ...base, location }).map((r) => r.entity.data.id)).toEqual(ids);
  });

  it("matches a full project root, excluding same-named, nested, and prefix-sharing repos", () => {
    const entities = [
      skill("selected", null, "/work/a/repo"),
      skill("same-name", null, "/work/b/repo"),
      skill("nested", null, "/work/a/repo/nested"),
      skill("prefix", null, "/work/a/repository"),
    ];
    const rows = filterEntities(entities, { ...base, location: { type: "project", root: "/work/a/repo" } });
    expect(rows.map((r) => r.entity.data.id)).toEqual(["selected"]);
  });

  it("filters locations before collapsing duplicates and counts only matching copies", () => {
    const entities = [
      skill("user", "shared", { type: "user" }),
      skill("first-project", "shared", "/work/first"),
      skill("second-project", "shared", "/work/second"),
    ];
    const selected = filterEntities(entities, {
      ...base,
      location: { type: "project", root: "/work/second" },
    });
    expect(selected.map((r) => [r.entity.data.id, r.duplicateCount])).toEqual([["second-project", 0]]);
    const projects = filterEntities(entities, { ...base, location: { type: "scope", scope: "project" } });
    expect(projects.map((r) => [r.entity.data.id, r.duplicateCount])).toEqual([["first-project", 1]]);
    const copies = filterEntities(entities, {
      ...base,
      location: { type: "scope", scope: "project" },
      collapseDuplicates: false,
    });
    expect(copies.map((r) => r.entity.data.id)).toEqual(["first-project", "second-project"]);
  });

  it("combines location with agent, search, and visibility preferences", () => {
    const entities = [
      skill("claude", null, "/work/selected"),
      skill("codex", null, "/work/selected", "codex"),
      skill("elsewhere", null, "/work/elsewhere", "codex"),
      skill("system", null, { type: "system" }, "codex"),
    ];
    const rows = filterEntities(entities, {
      ...base,
      agent: "codex",
      search: "SHARED",
      location: { type: "project", root: "/work/selected" },
    });
    expect(rows.map((r) => r.entity.data.id)).toEqual(["codex"]);
    expect(filterEntities(entities, { ...base, search: "no match" })).toEqual([]);
    expect(
      filterEntities(entities, {
        ...base,
        location: { type: "scope", scope: "system" },
        showSystemSkills: false,
      }),
    ).toEqual([]);
    const agentsSkill = skill("agents", null, { type: "user" }, "codex");
    agentsSkill.data.origin.file = "/home/test/.agents/skills/shared/SKILL.md";
    expect(
      filterEntities([agentsSkill], {
        ...base,
        location: { type: "scope", scope: "user" },
        showAgentsDirSkills: false,
        agentsDir: "/home/test/.agents",
      }),
    ).toEqual([]);
  });

  it("keeps automations in User even when their working directory is a project", () => {
    const automation: AnyEntity = {
      kind: "automation",
      data: {
        id: "automation",
        name: "Review repo",
        agent: "codex",
        scope: { type: "user" },
        kind: "scheduled",
        status: "ACTIVE",
        prompt: "Review changes",
        rrule: null,
        model: null,
        reasoningEffort: null,
        executionEnvironment: null,
        target: { type: "project", projectId: "repo", local: true },
        cwds: ["/work/repo"],
        createdAt: null,
        updatedAt: null,
        dir: "/home/test/.codex/automations/review",
        origin: {
          agent: "codex",
          scope: { type: "user" },
          file: "/home/test/.codex/automations/review/automation.toml",
          locator: { type: "tomlPath", path: [] },
        },
        memoryPath: null,
        reportPaths: [],
        extra: {},
      },
    };
    expect(
      filterEntities([automation], { ...base, location: { type: "project", root: "/work/repo" } }),
    ).toEqual([]);
    expect(
      filterEntities([automation], { ...base, location: { type: "scope", scope: "user" } }),
    ).toHaveLength(1);
  });
});

describe("countEntitiesByKind", () => {
  it("shares location, agent, duplicate, and visibility rules but ignores text search", () => {
    const data = snapshot({
      skills: [
        skill("first", "shared", "/work/first", "codex").data,
        skill("second", "shared", "/work/second", "codex").data,
        skill("claude", null, "/work/first").data,
        skill("system", null, { type: "system" }, "codex").data,
      ],
      hooks: [{ id: "hook", event: "Stop", agent: "claude", scope: { type: "user" }, entries: [] }],
    });
    const options: FilterOptions = {
      ...base,
      agent: "codex",
      location: { type: "scope", scope: "project" },
      search: "no match",
      showSystemSkills: false,
    };
    expect(countEntitiesByKind(data, options)).toEqual({
      skill: 1,
      mcpServer: 0,
      plugin: 0,
      subAgent: 0,
      slashCommand: 0,
      hook: 0,
      automation: 0,
    });
    expect(countEntitiesByKind(data, { ...options, collapseDuplicates: false }).skill).toBe(2);
    expect(
      countEntitiesByKind(data, { ...options, location: { type: "scope", scope: "system" } }).skill,
    ).toBe(0);
    expect(countEntitiesByKind(data, { ...base, location: { type: "scope", scope: "user" } }).hook).toBe(1);
  });
});

describe("projectLocations", () => {
  const data = snapshot({
    projects: [
      { root: "/work/b/repo", exists: true, trackedBy: ["claude", "codex"] },
      { root: "/work/a/repo", exists: true, trackedBy: ["claude"] },
      { root: "/work/missing-empty", exists: false, trackedBy: ["claude"] },
      { root: "/work/missing-with-config", exists: false, trackedBy: ["codex"] },
    ],
    skills: [
      skill("a", null, "/work/a/repo").data,
      skill("b", null, "/work/a/repo").data,
      skill("missing", null, "/work/missing-with-config").data,
    ],
    hooks: [
      {
        id: "hook",
        event: "Stop",
        agent: "claude",
        scope: { type: "project", root: "/work/entity-only" },
        entries: [],
      },
    ],
  });

  it("merges tracked and entity-only roots, deduplicates full paths, and retains missing roots with config", () => {
    expect(projectLocations(data, false)).toEqual([
      { root: "/work/entity-only", missing: false },
      { root: "/work/missing-with-config", missing: true },
      { root: "/work/a/repo", missing: false },
      { root: "/work/b/repo", missing: false },
    ]);
  });

  it("includes empty missing projects when that preference is enabled", () => {
    expect(projectLocations(data, true)).toContainEqual({ root: "/work/missing-empty", missing: true });
  });

  it("returns no stale project choices after a rescan removes the roots", () => {
    expect(projectLocations(snapshot(), false)).toEqual([]);
  });
});
