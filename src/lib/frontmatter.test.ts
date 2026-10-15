import { describe, expect, it } from "vitest";
import { diffDraft, FIELD_SPECS, initialDraft, listOf, otherFields } from "./frontmatter";
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
    expect(diffDraft(fm, specs, draft, "claude")).toEqual([]);
    draft.description = "";
    draft["allowed-tools"] = ["Bash(a:*)"];
    draft["disable-model-invocation"] = true;
    draft.model = "   ";
    expect(diffDraft(fm, specs, draft, "claude")).toEqual([
      { key: "description", value: null },
      { key: "allowed-tools", value: ["Bash(a:*)"] },
      { key: "disable-model-invocation", value: true },
    ]);
  });
  it("preserves incompatible and unknown fields when a Codex skill is edited", () => {
    const original: Frontmatter = {
      ...fm,
      fields: { ...fm.fields, model: "sonnet", "disable-model-invocation": true, "user-invocable": false },
    };
    const specs = FIELD_SPECS.skill;
    const draft = initialDraft(original, specs);
    expect(diffDraft(original, specs, draft, "codex")).toEqual([]);
    draft.description = "Updated description";
    draft.model = "opus";
    draft["allowed-tools"] = [];
    draft["disable-model-invocation"] = false;
    draft["user-invocable"] = true;
    expect(diffDraft(original, specs, draft, "codex")).toEqual([
      { key: "description", value: "Updated description" },
    ]);
    expect(otherFields(original, specs)).toEqual([["version", "1"]]);
  });
  it("uses the entity kind when deciding which Codex fields can change", () => {
    const modelDraft = { model: "new-model" };
    const empty: Frontmatter = { present: false, raw: "", fields: {} };
    expect(diffDraft(empty, FIELD_SPECS.skill, modelDraft, "codex")).toEqual([]);
    expect(diffDraft(empty, FIELD_SPECS.subAgent, modelDraft, "codex")).toEqual([
      { key: "model", value: "new-model" },
    ]);
    const specs = FIELD_SPECS.slashCommand;
    const draft = initialDraft(fm, specs);
    draft["argument-hint"] = "[target]";
    draft["allowed-tools"] = [];
    draft.model = "sonnet";
    expect(diffDraft(fm, specs, draft, "codex")).toEqual([{ key: "argument-hint", value: "[target]" }]);
  });
});
