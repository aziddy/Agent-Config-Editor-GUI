import { describe, expect, it } from "vitest";
import { type AnyEntity, filterEntities, scopeLabel } from "./entities";
import type { Skill } from "./types";

function skill(
  id: string,
  dupGroup: string | null,
  root: string,
  agent: "claude" | "codex" = "claude",
): AnyEntity {
  const data: Skill = {
    id,
    name: "shared",
    description: "d",
    agent,
    scope: { type: "project", root },
    origin: {
      agent,
      scope: { type: "project", root },
      file: `${root}/.claude/skills/shared/SKILL.md`,
      locator: { type: "markdownFile" },
    },
    dir: `${root}/.claude/skills/shared`,
    enabled: { enabled: null, editable: false, toggle: null, reason: null },
    frontmatter: { fields: {}, raw: "", present: true },
    isSystem: false,
    lockManaged: false,
    contentHash: "abc",
    dupGroup,
    bodyPreview: "",
  };
  return { kind: "skill", data };
}

describe("filterEntities", () => {
  const base = {
    agent: "all" as const,
    search: "",
    showSystemSkills: true,
    showAgentsDirSkills: true,
    agentsDir: null,
  };
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
});
