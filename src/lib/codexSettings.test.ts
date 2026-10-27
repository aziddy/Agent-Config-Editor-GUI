import { expect, it } from "vitest";
import { codexSettingChanges, featureFields, setSetting, settingsValid, settingValue } from "./codexSettings";

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
  expect(settingsValid(before, { ...before, model: "changed" })).toBe(true);
  expect(settingsValid({}, { notify: [""] })).toBe(false);
  expect(settingsValid({}, { notify: ["notify-send", "Codex"] })).toBe(true);
  expect(settingsValid({}, { tui: { status_line: [] } })).toBe(true);
});

it("keeps invalid numeric drafts and enforces integer bounds without normalizing existing values", () => {
  expect(settingsValid({}, { agents: { max_concurrent_threads_per_session: 0 } })).toBe(false);
  expect(settingsValid({}, { model_context_window: 1.5 })).toBe(false);
  expect(settingsValid({}, { model_context_window: "bad" })).toBe(false);
  expect(settingsValid({}, { history: { max_bytes: Number.MAX_SAFE_INTEGER + 1 } })).toBe(false);
  expect(settingsValid({}, { memories: { max_unused_days: 366 } })).toBe(false);
  expect(settingsValid({}, { memories: { max_unused_days: 0 } })).toBe(true);
  expect(settingsValid({}, { skills: { max_context_tokens: 10001 } })).toBe(false);
  const before = { memories: { max_unused_days: 366 } };
  expect(settingsValid(before, { ...before, model: "changed" })).toBe(true);
  expect(codexSettingChanges({}, { history: { max_bytes: 0 }, analytics: { enabled: false } })).toEqual([
    { path: ["history", "max_bytes"], value: 0 },
    { path: ["analytics", "enabled"], value: false },
  ]);
});

it("replaces filter maps explicitly, preserves dotted patterns, and treats row reordering as a no-op", () => {
  const before = { shell_environment_policy: { filters: { PATH: "include", "SECRET.*": "exclude" } } };
  const rows = [
    { pattern: "SECRET.*", policy: "exclude" },
    { pattern: "PATH", policy: "include" },
  ];
  expect(codexSettingChanges(before, { shell_environment_policy: { filters: rows } })).toEqual([]);
  expect(codexSettingChanges(before, { shell_environment_policy: { filters: [] } })).toEqual([
    { path: ["shell_environment_policy", "filters"], value: {} },
  ]);
  expect(
    codexSettingChanges(before, setSetting(before, ["shell_environment_policy", "filters"], null)),
  ).toEqual([{ path: ["shell_environment_policy", "filters"], value: null }]);
  expect(
    settingsValid({}, { shell_environment_policy: { filters: [{ pattern: "", policy: "include" }] } }),
  ).toBe(false);
  expect(
    settingsValid(
      {},
      {
        shell_environment_policy: {
          filters: [
            { pattern: "PATH", policy: "include" },
            { pattern: "path", policy: "exclude" },
          ],
        },
      },
    ),
  ).toBe(false);
  expect(settingsValid({}, { project_doc_fallback_filenames: [""] })).toBe(false);
  expect(settingsValid({}, { tui: { notifications: [""] } })).toBe(false);
});

it("offers documented unset flags without suggesting ignored switches or duplicating custom flags", () => {
  const fields = featureFields({ features: { hooks: true, js_repl: true, unified_exec: false } }, {}, [
    ["features", "code_mode"],
  ]);
  expect(fields.filter((field) => field.path[1] === "hooks")).toHaveLength(1);
  expect(fields.find((field) => field.path[1] === "fast_mode")?.help).toContain("default enabled");
  expect(fields.find((field) => field.path[1] === "prevent_idle_sleep")?.help).toContain("Experimental");
  expect(fields.find((field) => field.path[1] === "js_repl")).toBeTruthy();
  expect(fields.find((field) => field.path[1] === "code_mode")).toBeTruthy();
  expect(fields.find((field) => field.path[1] === "unified_exec")?.readOnly).toBe(true);
  expect(featureFields({}, {}, []).some((field) => field.path[1] === "unified_exec")).toBe(false);
});
