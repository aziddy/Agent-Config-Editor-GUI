import { describe, expect, it } from "vitest";
import { clampEditorTreeWidth, editorTreeFits, fileTreeAncestors } from "./fileTree";

describe("fileTreeAncestors", () => {
  it("opens the root and every folder containing the primary file", () => {
    expect(fileTreeAncestors("/skills/example", "/skills/example/SKILL.md")).toEqual(["/skills/example"]);
    expect(fileTreeAncestors("/plugin", "/plugin/skills/example/SKILL.md")).toEqual([
      "/plugin",
      "/plugin/skills",
      "/plugin/skills/example",
    ]);
  });

  it("expands the destination of file switching, including trailing-slash and filesystem roots", () => {
    expect(fileTreeAncestors("/skills/example/", "/skills/example/references/setup.md")).toEqual([
      "/skills/example",
      "/skills/example/references",
    ]);
    expect(fileTreeAncestors("/", "/etc/agent/config.toml")).toEqual(["/", "/etc", "/etc/agent"]);
  });

  it("leaves the entity tree rooted in place for automation reports outside its directory", () => {
    expect(fileTreeAncestors("/automation", "/reports/run.md")).toEqual([]);
    expect(fileTreeAncestors("/automation", "/automation-other/run.md")).toEqual([]);
    expect(fileTreeAncestors("/automation", null)).toEqual([]);
  });
});

describe("editor tree sizing", () => {
  it("clamps the remembered width between 140 and 360 pixels", () => {
    expect(clampEditorTreeWidth(20)).toBe(140);
    expect(clampEditorTreeWidth(220)).toBe(220);
    expect(clampEditorTreeWidth(400)).toBe(360);
  });

  it("leaves 200 pixels for the editor, plus the divider and padding, when the pane narrows", () => {
    expect(clampEditorTreeWidth(360, 452)).toBe(220);
    expect(clampEditorTreeWidth(220, 372)).toBe(140);
    expect(editorTreeFits(372)).toBe(true);
    expect(editorTreeFits(371)).toBe(false);
    expect(editorTreeFits(324)).toBe(false);
  });
});
