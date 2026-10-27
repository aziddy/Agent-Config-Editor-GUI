// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { App } from "@/App";
import { setSetting } from "@/lib/codexSettings";
import { queryKeys } from "@/lib/queries";
import type { CodexSettingChange, CodexSettings, JsonMap, Snapshot, WritePlan } from "@/lib/types";
import { usePlanStore } from "@/state/planStore";
import { useUiStore } from "@/state/uiStore";
import { useUnsavedChangesStore } from "@/state/unsavedChangesStore";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@/lib/rpc", () => ({ rpc: invoke }));
vi.mock("@/lib/events", () => ({ useConfigChangedListener: () => {} }));
vi.mock("@/components/CodeEditor", () => ({
  CodeEditor: ({ value, readOnly }: { value: string; readOnly: boolean }) => (
    <textarea aria-label="File text" value={value} readOnly={readOnly} onChange={() => {}} />
  ),
}));
const clients: QueryClient[] = [];
const path = "/profile/custom-codex/config.toml";

beforeEach(() => {
  vi.stubGlobal("matchMedia", () => ({ matches: false, addEventListener() {}, removeEventListener() {} }));
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
  HTMLDialogElement.prototype.close = function () {
    this.open = false;
  };
});

afterEach(async () => {
  cleanup();
  await act(async () => {
    usePlanStore.getState().finish(null);
    useUnsavedChangesStore.getState().finish("cancel");
  });
  for (const client of clients.splice(0)) client.clear();
  useUiStore.setState(useUiStore.getInitialState(), true);
  invoke.mockReset();
  vi.unstubAllGlobals();
});

function view({
  readOnly = false,
  named = false,
  custom = false,
  failRead = false,
  failApply = false,
  values = {},
  unsupportedPaths = [],
}: {
  readOnly?: boolean;
  named?: boolean;
  custom?: boolean;
  failRead?: boolean;
  failApply?: boolean;
  values?: JsonMap;
  unsupportedPaths?: string[][];
} = {}) {
  let data: CodexSettings = {
    path,
    hash: "original",
    mode: 0o600,
    values: {
      model: "custom-model",
      model_provider: "custom.provider",
      model_reasoning_effort: "future-effort",
      service_tier: "fast",
      features: { hooks: true },
      memories: { use_memories: true },
      ...(named ? { default_permissions: "team" } : { sandbox_mode: "workspace-write" }),
      ...values,
    },
    providerNames: ["custom.provider"],
    permissionProfiles: ["team"],
    namedPermissions: named,
    unsupportedPaths: [...(custom ? [["approval_policy"]] : []), ...unsupportedPaths],
  };
  const snapshot: Snapshot = {
    generatedAt: "",
    homes: {
      home: "/profile",
      codexHome: "/profile/custom-codex/",
      claudeDir: "/profile/.claude",
      claudeState: "/profile/.claude.json",
      agentsDir: "/profile/.agents",
      sandbox: true,
      readOnly,
    },
    skills: [],
    mcpServers: [],
    plugins: [],
    subagents: [],
    commands: [],
    hooks: [],
    automations: [],
    memories: [],
    projects: [],
    dupGroups: [],
    warnings: [],
    fileHashes: {},
    scanMillis: 1,
  };
  let pending: CodexSettingChange[] = [];
  invoke.mockImplementation(async (op, args) => {
    switch (op) {
      case "scan_all":
        return snapshot;
      case "get_app_settings":
        return { theme: "light" };
      case "get_codex_settings":
        if (failRead) throw new Error("Invalid TOML");
        return data;
      case "get_file_text":
        return {
          path,
          hash: data.hash,
          mode: 0o600,
          language: "toml",
          text: `model = "${data.values.model}"\n`,
        };
      case "preview_save_codex_settings": {
        pending = args.changes;
        const plan: WritePlan = {
          path,
          baseHash: data.hash,
          newText: "reviewed text",
          unifiedDiff: "-before\n+after",
          mode: 0o600,
          warnings: [],
          extraFiles: [],
          mutation: { type: "rawText" },
          description: "Save Codex settings",
        };
        return plan;
      }
      case "apply_write_plan": {
        if (failApply) throw new Error("External change");
        let values = data.values;
        for (const change of pending) values = setSetting(values, change.path, change.value);
        data = { ...data, hash: "saved", values };
        return { path, newHash: data.hash, backupPath: "/backups/config.toml" };
      }
      default:
        throw new Error(`Unexpected RPC: ${op}`);
    }
  });
  useUiStore.setState({
    nav: "codexSettings",
    agentFilter: "claude",
    locationFilter: { type: "project", root: "/work/project" },
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  clients.push(client);
  render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
  return {
    client,
    changeDisk() {
      data = { ...data, hash: "external", values: { ...data.values, model: "external-model" } };
    },
  };
}

async function model() {
  return (await screen.findByRole("textbox", { name: "Model" })) as HTMLInputElement;
}
function saveButton() {
  return screen.getByRole("button", { name: "Save…" }) as HTMLButtonElement;
}
async function dialog() {
  return await screen.findByRole("dialog", { name: "Save Codex settings" });
}

it("shows the expanded cards and unset options without writing defaults", async () => {
  view();
  expect((await model()).value).toBe("custom-model");
  expect(screen.getByRole("combobox", { name: "Provider" }).getAttribute("value")).toBe("custom.provider");
  expect((screen.getByRole("combobox", { name: "Reasoning effort" }) as HTMLInputElement).value).toBe(
    "future-effort",
  );
  for (const name of [
    "Model & reasoning",
    "Permissions & web",
    "Instructions",
    "Features",
    "Memories",
    "Terminal & notifications",
    "Agents",
    "Context & compaction",
    "Project instructions",
    "History & privacy",
    "Shell environment",
  ])
    expect(screen.getByRole("heading", { name })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Codex Config" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Codex Settings" }).getAttribute("aria-current")).toBe("page");
  expect(screen.queryByPlaceholderText(/Search/)).toBeNull();
  expect(saveButton().disabled).toBe(true);
  expect((screen.getByRole("combobox", { name: "Persisted goals" }) as HTMLSelectElement).value).toBe("");
  expect((screen.getByRole("textbox", { name: "Concurrent subagents" }) as HTMLInputElement).value).toBe("");
  expect(invoke).toHaveBeenCalledWith("get_codex_settings", {});
  expect(invoke.mock.calls.every(([op]) => !op.startsWith("preview_") && op !== "apply_write_plan")).toBe(
    true,
  );
});

it("reviews a batch of new settings with numeric, boolean, and default-removal values intact", async () => {
  view();
  await model();
  fireEvent.change(screen.getByRole("textbox", { name: "Concurrent subagents" }), { target: { value: "6" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Auto-compaction threshold (tokens)" }), {
    target: { value: "120000" },
  });
  fireEvent.change(screen.getByRole("combobox", { name: "Command history" }), { target: { value: "none" } });
  fireEvent.change(screen.getByRole("combobox", { name: "Analytics" }), { target: { value: "false" } });
  fireEvent.change(screen.getByRole("combobox", { name: "Persisted goals" }), { target: { value: "false" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Maximum unused age (days)" }), {
    target: { value: "0" },
  });
  fireEvent.click(saveButton());
  const preview = await dialog();
  const changes = invoke.mock.calls.find(([op]) => op === "preview_save_codex_settings")?.[1].changes;
  expect(changes).toHaveLength(6);
  expect(changes).toEqual(
    expect.arrayContaining([
      { path: ["agents", "max_concurrent_threads_per_session"], value: 6 },
      { path: ["model_auto_compact_token_limit"], value: 120000 },
      { path: ["history", "persistence"], value: "none" },
      { path: ["analytics", "enabled"], value: false },
      { path: ["features", "goals"], value: false },
      { path: ["memories", "max_unused_days"], value: 0 },
    ]),
  );
  fireEvent.click(within(preview).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(saveButton().disabled).toBe(true));
  expect((screen.getByRole("textbox", { name: "Concurrent subagents" }) as HTMLInputElement).value).toBe("6");
  fireEvent.click(screen.getByRole("button", { name: "Reset Concurrent subagents to default" }));
  fireEvent.click(saveButton());
  await dialog();
  expect(invoke).toHaveBeenLastCalledWith("preview_save_codex_settings", {
    baseHash: "saved",
    changes: [{ path: ["agents", "max_concurrent_threads_per_session"], value: null }],
  });
});

it("keeps invalid numeric drafts visible and blocks previews until they are fixed or reset", async () => {
  view();
  await model();
  const input = screen.getByRole("textbox", { name: "Concurrent subagents" }) as HTMLInputElement;
  for (const value of ["-1", "0", "1.5", "1e3", "9007199254740992"]) {
    fireEvent.change(input, { target: { value } });
    expect(input.value).toBe(value);
    expect(screen.getByRole("alert")).toBeTruthy();
    expect(saveButton().disabled).toBe(true);
  }
  fireEvent.change(input, { target: { value: "4" } });
  expect(screen.queryByRole("alert")).toBeNull();
  expect(saveButton().disabled).toBe(false);
  const memory = screen.getByRole("textbox", { name: "Maximum unused age (days)" });
  fireEvent.change(memory, { target: { value: "366" } });
  expect(saveButton().disabled).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Reset Maximum unused age (days) to default" }));
  expect(saveButton().disabled).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Reset Concurrent subagents to default" }));
  expect(saveButton().disabled).toBe(true);
  expect(invoke.mock.calls.some(([op]) => op === "preview_save_codex_settings")).toBe(false);
});

it("edits environment filter rows without losing focus and blocks duplicate patterns", async () => {
  view({ values: { shell_environment_policy: { filters: { PATH: "include" } } } });
  await model();
  fireEvent.click(screen.getByRole("button", { name: "Add filter" }));
  expect(saveButton().disabled).toBe(true);
  const input = screen.getByRole("textbox", { name: "Environment pattern 2" }) as HTMLInputElement;
  input.focus();
  fireEvent.change(input, { target: { value: "path" } });
  expect(document.activeElement).toBe(input);
  expect(screen.getByRole("alert").textContent).toContain("unique");
  expect(saveButton().disabled).toBe(true);
  fireEvent.change(input, { target: { value: "SECRET.*" } });
  fireEvent.change(screen.getByRole("combobox", { name: "Environment policy 2" }), {
    target: { value: "exclude" },
  });
  fireEvent.click(saveButton());
  await dialog();
  expect(invoke).toHaveBeenCalledWith("preview_save_codex_settings", {
    baseHash: "original",
    changes: [
      { path: ["shell_environment_policy", "filters"], value: { PATH: "include", "SECRET.*": "exclude" } },
    ],
  });
});

it("supports notification event lists and retains list input focus", async () => {
  view();
  await model();
  const notifications = screen.getByRole("combobox", { name: "Terminal notifications" });
  fireEvent.change(notifications, { target: { value: "events" } });
  const group = screen.getByRole("group", { name: "Terminal notifications items" });
  fireEvent.click(within(group).getByRole("button", { name: "Add" }));
  expect(saveButton().disabled).toBe(true);
  const item = screen.getByRole("textbox", { name: "Terminal notifications 1" });
  item.focus();
  fireEvent.change(item, { target: { value: "agent-turn-complete" } });
  expect(document.activeElement).toBe(item);
  fireEvent.click(saveButton());
  await dialog();
  expect(invoke).toHaveBeenCalledWith("preview_save_codex_settings", {
    baseHash: "original",
    changes: [{ path: ["tui", "notifications"], value: ["agent-turn-complete"] }],
  });
});

it("keeps legacy alias conflicts and structured feature values read-only", async () => {
  view({
    values: { agents: { max_threads: 3 }, shell_environment_policy: { exclude: ["SECRET_*"] } },
    unsupportedPaths: [["features", "network_proxy"]],
  });
  await model();
  expect(screen.getByRole("textbox", { name: "Concurrent subagents" }).matches(":disabled")).toBe(true);
  expect(screen.getByRole("button", { name: "Add filter" }).matches(":disabled")).toBe(true);
  expect(screen.queryByRole("combobox", { name: "Sandbox command network proxy" })).toBeNull();
  fireEvent.change(await model(), { target: { value: "safe-unrelated-edit" } });
  expect(saveButton().disabled).toBe(false);
});

it("previews only changed leaves across cards, retains cancelled drafts, and refreshes both views after Apply", async () => {
  view();
  fireEvent.change(await model(), { target: { value: "next-model" } });
  fireEvent.change(screen.getByRole("combobox", { name: "Use memories" }), { target: { value: "false" } });
  fireEvent.click(screen.getByRole("button", { name: "Reset Service tier to default" }));
  fireEvent.click(saveButton());
  let preview = await dialog();
  expect(invoke).toHaveBeenCalledWith("preview_save_codex_settings", {
    baseHash: "original",
    changes: [
      { path: ["model"], value: "next-model" },
      { path: ["service_tier"], value: null },
      { path: ["memories", "use_memories"], value: false },
    ],
  });
  expect(invoke.mock.calls.some(([op]) => op === "apply_write_plan")).toBe(false);
  fireEvent.click(within(preview).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(saveButton().disabled).toBe(false));
  expect((await model()).value).toBe("next-model");
  fireEvent.click(saveButton());
  preview = await dialog();
  fireEvent.click(within(preview).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(screen.queryByText(/unsaved changes/)).toBeNull());
  expect(saveButton().disabled).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Open TOML" }));
  const raw = (await screen.findByRole("textbox", { name: "File text" })) as HTMLTextAreaElement;
  expect(raw.value).toContain("next-model");
  expect(useUiStore.getState().nav).toBe("codexConfig");
});

it("guards switching to TOML and discards only after the user chooses Discard", async () => {
  view();
  fireEvent.change(await model(), { target: { value: "draft-model" } });
  fireEvent.click(screen.getByRole("button", { name: "Open TOML" }));
  let prompt = await screen.findByRole("dialog", { name: "Unsaved changes" });
  fireEvent.click(within(prompt).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(useUiStore.getState().navigationPending).toBe(false));
  expect(useUiStore.getState().nav).toBe("codexSettings");
  expect((await model()).value).toBe("draft-model");
  fireEvent.click(screen.getByRole("button", { name: "Open TOML" }));
  prompt = await screen.findByRole("dialog", { name: "Unsaved changes" });
  fireEvent.click(within(prompt).getByRole("button", { name: "Discard" }));
  const raw = (await screen.findByRole("textbox", { name: "File text" })) as HTMLTextAreaElement;
  expect(raw.value).toContain("custom-model");
  expect(invoke.mock.calls.every(([op]) => op !== "apply_write_plan")).toBe(true);
});

it("refreshes through the shared file query and protects drafts from external writes", async () => {
  const { client, changeDisk } = view();
  fireEvent.change(await model(), { target: { value: "draft-model" } });
  changeDisk();
  await act(async () => {
    await client.invalidateQueries({ queryKey: queryKeys.file(path) });
  });
  await screen.findByText(/Saving is blocked until you reload/);
  expect((await model()).value).toBe("draft-model");
  expect(saveButton().disabled).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Reload" }));
  await waitFor(() =>
    expect((screen.getByRole("textbox", { name: "Model" }) as HTMLInputElement).value).toBe("external-model"),
  );
  expect(saveButton().disabled).toBe(true);
});

it("keeps a failed save dirty and lets the user retry", async () => {
  view({ failApply: true });
  fireEvent.change(await model(), { target: { value: "draft-model" } });
  fireEvent.click(saveButton());
  const preview = await dialog();
  fireEvent.click(within(preview).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(saveButton().disabled).toBe(false));
  expect((await model()).value).toBe("draft-model");
  expect(screen.getByText(/unsaved changes/)).toBeTruthy();
});

it("disables edits and saves in read-only mode", async () => {
  view({ readOnly: true });
  const input = await model();
  expect(input.matches(":disabled")).toBe(true);
  expect(saveButton().disabled).toBe(true);
  expect(
    (screen.getByRole("combobox", { name: "Use memories" }) as HTMLSelectElement).matches(":disabled"),
  ).toBe(true);
  expect(invoke.mock.calls.every(([op]) => !op.startsWith("preview_") && op !== "apply_write_plan")).toBe(
    true,
  );
});

it("uses named permission profiles without offering legacy sandbox settings", async () => {
  view({ named: true });
  await model();
  expect((screen.getByRole("combobox", { name: "Permissions profile" }) as HTMLSelectElement).value).toBe(
    "team",
  );
  expect(screen.queryByRole("combobox", { name: "Sandbox mode" })).toBeNull();
});

it("leaves custom TOML values in the raw view while allowing other form fields", async () => {
  view({ custom: true });
  await model();
  expect(screen.queryByRole("combobox", { name: "Approval policy" })).toBeNull();
  expect(screen.getByText("Custom TOML value. Edit it in Codex Config.")).toBeTruthy();
  expect(saveButton().disabled).toBe(true);
});

it("offers the TOML editor when the settings file fails to parse", async () => {
  view({ failRead: true });
  expect((await screen.findByRole("alert")).textContent).toContain("Invalid TOML");
  expect(screen.queryByRole("textbox", { name: "Model" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Open TOML" }));
  await screen.findByRole("textbox", { name: "File text" });
  expect(invoke.mock.calls.every(([op]) => !op.startsWith("preview_") && op !== "apply_write_plan")).toBe(
    true,
  );
});
