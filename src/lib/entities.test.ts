import { describe, expect, it } from "vitest";
import {
  type AnyEntity,
  countEntitiesByKind,
  countMemorySources,
  DEFAULT_SKILL_FILTERS,
  entitiesOfKind,
  entityAgents,
  entityDir,
  entityFile,
  type FilterOptions,
  filterEntities,
  findEntity,
  type LocationFilter,
  memorySourceDescription,
  projectLocations,
  type SkillFilters,
  scopeLabel,
} from "./entities";
import type { MemoryFile, Scope, Skill, Snapshot } from "./types";

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
    allowImplicitInvocation: true,
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
    memories: [],
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

describe("skill filters", () => {
  function examples() {
    const claude = skill("claude", null, "/work/repo");
    const allowed = skill("allowed", null, "/work/repo", "codex");
    allowed.data.enabled.enabled = true;
    const blocked = skill("blocked", null, "/work/repo", "codex");
    blocked.data.allowImplicitInvocation = false;
    blocked.data.enabled.enabled = false;
    const unknown = skill("unknown", null, "/work/repo", "codex");
    unknown.data.allowImplicitInvocation = null;
    unknown.data.enabled.enabled = true;
    const builtIn = skill("built-in", null, { type: "system" }, "codex");
    builtIn.data.enabled.enabled = true;
    const managed = skill("managed", null, { type: "user" }, "codex");
    managed.data.lockManaged = true;
    managed.data.allowImplicitInvocation = false;
    managed.data.enabled.enabled = true;
    managed.data.origin.file = "/profile/.agents/skills/managed/SKILL.md";
    return [claude, allowed, blocked, unknown, builtIn, managed];
  }

  const cases: { filters: Partial<SkillFilters>; ids: string[] }[] = [
    { filters: {}, ids: ["claude", "allowed", "blocked", "unknown", "built-in", "managed"] },
    { filters: { invocation: "allowed" }, ids: ["claude", "allowed", "built-in"] },
    { filters: { invocation: "blocked" }, ids: ["blocked", "managed"] },
    { filters: { invocation: "unknown" }, ids: ["unknown"] },
    { filters: { enabled: "enabled" }, ids: ["allowed", "unknown", "built-in", "managed"] },
    { filters: { enabled: "disabled" }, ids: ["blocked"] },
    { filters: { enabled: "notApplicable" }, ids: ["claude"] },
    { filters: { builtIn: "yes" }, ids: ["built-in"] },
    { filters: { builtIn: "no" }, ids: ["claude", "allowed", "blocked", "unknown", "managed"] },
    { filters: { installerManaged: "yes" }, ids: ["managed"] },
    { filters: { installerManaged: "no" }, ids: ["claude", "allowed", "blocked", "unknown", "built-in"] },
    {
      filters: { invocation: "blocked", enabled: "enabled", builtIn: "no", installerManaged: "yes" },
      ids: ["managed"],
    },
  ];
  it.each(cases)("matches $filters without conflating invocation and enabled state", ({ filters, ids }) => {
    expect(
      filterEntities(examples(), { ...base, skillFilters: { ...DEFAULT_SKILL_FILTERS, ...filters } }).map(
        ({ entity }) => entity.data.id,
      ),
    ).toEqual(ids);
  });

  it("combines skill filters with agent, location, search, and visibility preferences", () => {
    const options: FilterOptions = {
      ...base,
      agent: "codex",
      location: { type: "project", root: "/work/repo" },
      search: "shared",
      skillFilters: { ...DEFAULT_SKILL_FILTERS, invocation: "allowed" },
    };
    expect(filterEntities(examples(), options).map(({ entity }) => entity.data.id)).toEqual(["allowed"]);
    expect(filterEntities(examples(), { ...options, search: "missing" })).toEqual([]);
    expect(
      filterEntities(examples(), {
        ...base,
        showSystemSkills: false,
        skillFilters: { ...DEFAULT_SKILL_FILTERS, builtIn: "yes" },
      }),
    ).toEqual([]);
    expect(
      filterEntities(examples(), {
        ...base,
        showAgentsDirSkills: false,
        agentsDir: "/profile/.agents",
        skillFilters: { ...DEFAULT_SKILL_FILTERS, installerManaged: "yes" },
      }),
    ).toEqual([]);
  });

  it("filters duplicate members before choosing the survivor and counting copies", () => {
    const first = skill("allowed", "identical-markdown", "/work/first", "codex");
    const second = skill("blocked", "identical-markdown", "/work/second", "codex");
    const third = skill("also-blocked", "identical-markdown", "/work/third");
    second.data.allowImplicitInvocation = false;
    third.data.allowImplicitInvocation = false;
    const options: FilterOptions = {
      ...base,
      skillFilters: { ...DEFAULT_SKILL_FILTERS, invocation: "blocked" },
    };
    expect(
      filterEntities([first, second, third], options).map(({ entity, duplicateCount }) => [
        entity.data.id,
        duplicateCount,
      ]),
    ).toEqual([["blocked", 1]]);
    expect(
      filterEntities([first, second, third], { ...options, collapseDuplicates: false }).map(
        ({ entity }) => entity.data.id,
      ),
    ).toEqual(["blocked", "also-blocked"]);
  });

  it("keeps sidebar totals and other entity kinds independent of skill filters", () => {
    const data = snapshot({
      skills: examples().map(({ data }) => data),
      hooks: [{ id: "hook", event: "Stop", agent: "claude", scope: { type: "user" }, entries: [] }],
    });
    const options: FilterOptions = {
      ...base,
      skillFilters: { ...DEFAULT_SKILL_FILTERS, invocation: "unknown", enabled: "disabled" },
    };
    expect(filterEntities(entitiesOfKind(data, "skill"), options)).toEqual([]);
    expect(filterEntities(entitiesOfKind(data, "hook"), options)).toHaveLength(1);
    expect(countEntitiesByKind(data, options)).toMatchObject({ skill: 6, hook: 1 });
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
      memory: 0,
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

function memory(overrides: Partial<MemoryFile> = {}): Extract<AnyEntity, { kind: "memory" }> {
  return {
    kind: "memory",
    data: {
      id: "memory:shared",
      name: "TEAM_GUIDE.md",
      category: "instruction",
      source: "instruction",
      agents: ["claude", "codex"],
      scopes: [
        { type: "project", root: "/work/first" },
        { type: "project", root: "/work/second" },
      ],
      paths: ["/work/first/TEAM_GUIDE.md", "/work/second/TEAM_GUIDE.md"],
      file: "/shared/TEAM_GUIDE.md",
      dir: "/shared",
      contentHash: "same-content",
      generated: false,
      ...overrides,
    },
  };
}

describe("memory and instruction inventory", () => {
  it("shows shared files once in All and matches both providers and project associations", () => {
    const shared = memory();
    for (const agent of ["all", "claude", "codex"] as const) {
      expect(filterEntities([shared], { ...base, agent })).toHaveLength(1);
    }
    for (const root of ["/work/first", "/work/second"]) {
      expect(filterEntities([shared], { ...base, location: { type: "project", root } })).toHaveLength(1);
    }
    expect(filterEntities([shared], { ...base, location: { type: "scope", scope: "user" } })).toEqual([]);
    expect(entityAgents(shared)).toEqual(["claude", "codex"]);
  });

  it("filters sources and metadata without collapsing separate files with identical contents", () => {
    const instruction = memory();
    const note = memory({
      id: "memory:note",
      name: "MEMORY.md",
      category: "memory",
      source: "codexMemory",
      file: "/other/MEMORY.md",
    });
    expect(filterEntities([instruction, note], base)).toHaveLength(2);
    expect(
      filterEntities([instruction, note], { ...base, memorySource: "codexMemory" }).map(
        (r) => r.entity.data.id,
      ),
    ).toEqual(["memory:note"]);
    expect(
      filterEntities([instruction, note], { ...base, memorySource: "instruction" }).map(
        (r) => r.entity.data.id,
      ),
    ).toEqual(["memory:shared"]);
    expect(filterEntities([note], { ...base, search: "core codex" })).toHaveLength(1);
    expect(filterEntities([instruction], { ...base, search: "/work/second" })).toHaveLength(1);
    expect(filterEntities([instruction], { ...base, search: "/shared/TEAM_GUIDE.md" })).toHaveLength(1);
  });

  it("finds inventory files, uses the logical path for editing, and includes every project location", () => {
    const shared = memory();
    const data = snapshot({ memories: [shared.data] });
    expect(entitiesOfKind(data, "memory")).toEqual([shared]);
    expect(findEntity(data, shared.data.id)).toEqual(shared);
    expect(entityFile(shared)).toBe("/work/first/TEAM_GUIDE.md");
    expect(entityDir(shared)).toBe("/shared");
    expect(projectLocations(data, false).map((p) => p.root)).toEqual(["/work/first", "/work/second"]);
    expect(countEntitiesByKind(data, { ...base, memorySource: "codexMemory" }).memory).toBe(1);
    expect(
      countEntitiesByKind(data, { ...base, memorySource: "instruction", search: "missing" }).memory,
    ).toBe(1);
  });

  it("counts every source independently while retaining provider and location filters", () => {
    const data = snapshot({
      memories: [
        memory({ id: "core", source: "codexMemory", agents: ["codex"], scopes: [{ type: "user" }] }).data,
        memory({ id: "automation", source: "automation", agents: ["codex"], scopes: [{ type: "user" }] })
          .data,
        memory({
          id: "claude",
          source: "claudeProject",
          agents: ["claude"],
          scopes: [{ type: "project", root: "/work/first" }],
        }).data,
        memory().data,
      ],
    });
    const options: FilterOptions = { ...base, memorySource: "codexMemory", search: "missing" };
    expect(countMemorySources(data, options)).toEqual({
      codexMemory: 1,
      automation: 1,
      claudeProject: 1,
      claudeSubagent: 0,
      instruction: 1,
      rule: 0,
    });
    expect(countEntitiesByKind(data, options).memory).toBe(4);
    expect(countMemorySources(data, { ...options, agent: "codex" })).toMatchObject({
      codexMemory: 1,
      automation: 1,
      claudeProject: 0,
      instruction: 1,
    });
    expect(
      countMemorySources(data, { ...options, location: { type: "project", root: "/work/first" } }),
    ).toMatchObject({
      codexMemory: 0,
      automation: 0,
      claudeProject: 1,
      instruction: 1,
    });
    expect(countMemorySources(undefined, options)).toEqual({
      codexMemory: 0,
      automation: 0,
      claudeProject: 0,
      claudeSubagent: 0,
      instruction: 0,
      rule: 0,
    });
  });

  it("describes memory roots using the detected homes", () => {
    const homes = {
      home: "/profile",
      claudeDir: "/profile/claude",
      claudeState: "/profile/.claude.json",
      codexHome: "/profile/codex",
      agentsDir: "/profile/.agents",
      sandbox: true,
      readOnly: false,
    };
    expect(memorySourceDescription("codexMemory", homes)).toContain("/profile/codex/memories");
    expect(memorySourceDescription("automation", homes)).toContain("/profile/codex/automations/*/memory.md");
    expect(memorySourceDescription("claudeProject", homes)).toContain("/profile/claude/projects/*/memory");
  });
});
