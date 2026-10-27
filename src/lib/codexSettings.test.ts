import { expect, it } from "vitest";
import { codexSettingChanges, setSetting, settingsListsValid, settingValue } from "./codexSettings";

it("builds changed-leaf patches without writing defaults or losing explicit false and empty lists", () => {
  const before = { model: "custom-model", features: { hooks: true }, tui: { status_line: ["model"] } };
  let after = setSetting(before, ["features", "hooks"], false);
  after = setSetting(after, ["model"], null);
  after = setSetting(after, ["tui", "status_line"], []);
  expect(codexSettingChanges(before, before)).toEqual([]);
  expect(codexSettingChanges(before, after)).toEqual([
    { path: ["model"], value: null },
    { path: ["features", "hooks"], value: false },
    { path: ["tui", "status_line"], value: [] },
  ]);
  expect(before.model).toBe("custom-model");
  expect(before.features.hooks).toBe(true);
});

it("keeps dotted keys as single path segments and handles unusual keys without prototype mutation", () => {
  const updated = setSetting({}, ["features", "flag.with.dots"], true);
  const unusual = setSetting(updated, ["features", "__proto__"], false);
  expect(codexSettingChanges({}, unusual)).toEqual([
    { path: ["features", "flag.with.dots"], value: true },
    { path: ["features", "__proto__"], value: false },
  ]);
  expect(settingValue(unusual, ["features", "__proto__"])).toBe(false);
  expect(settingValue({}, ["__proto__"])).toBeUndefined();
});

it("validates edited list rows without blocking unrelated saves for existing custom lists", () => {
  const before = { notify: [""] };
  expect(settingsListsValid(before, { ...before, model: "changed" })).toBe(true);
  expect(settingsListsValid({}, { notify: [""] })).toBe(false);
  expect(settingsListsValid({}, { notify: ["notify-send", "Codex"] })).toBe(true);
  expect(settingsListsValid({}, { tui: { status_line: [] } })).toBe(true);
});
