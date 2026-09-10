import { describe, expect, it } from "vitest";
import { diffDraft, FIELD_SPECS, initialDraft, listOf } from "./frontmatter";
import type { Frontmatter } from "./types";

const fm: Frontmatter = {
  present: true,
  raw: "",
  fields: { name: "x", description: "d", "allowed-tools": "Bash(a:*), Bash(b:*)", version: "1" },
};

describe("frontmatter drafts", () => {
  it("normalises comma lists and arrays", () => {
    expect(listOf("Bash(a:*), Bash(b:*)")).toEqual(["Bash(a:*)", "Bash(b:*)"]);
    expect(listOf(["Read", "Grep"])).toEqual(["Read", "Grep"]);
    expect(listOf(undefined)).toEqual([]);
  });
  it("emits only changed keys, deleting emptied ones", () => {
    const specs = FIELD_SPECS.skill;
    const draft = initialDraft(fm, specs);
    expect(diffDraft(fm, specs, draft)).toEqual([]);
    draft.description = "";
    draft["allowed-tools"] = ["Bash(a:*)"];
    draft["disable-model-invocation"] = true;
    draft.model = "   ";
    expect(diffDraft(fm, specs, draft)).toEqual([
      { key: "description", value: null },
      { key: "allowed-tools", value: ["Bash(a:*)"] },
      { key: "disable-model-invocation", value: true },
    ]);
  });
});
