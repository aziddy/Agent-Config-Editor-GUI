// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { App } from "@/App";
import { setSetting } from "@/lib/codexSettings";
import { queryKeys } from "@/lib/queries";
import type { CodexSettingChange, CodexSettings, Snapshot, WritePlan } from "@/lib/types";
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

function view({ readOnly = false, named = false, custom = false, failRead = false, failApply = false } = {}) {
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
    },
    providerNames: ["custom.provider"],
    permissionProfiles: ["team"],
    namedPermissions: named,
    unsupportedPaths: custom ? [["approval_policy"]] : [],
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

it("adds a separate six-card page without changing the existing TOML destination", async () => {
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
  ])
    expect(screen.getByRole("heading", { name })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Codex Config" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Codex Settings" }).getAttribute("aria-current")).toBe("page");
  expect(screen.queryByPlaceholderText(/Search/)).toBeNull();
  expect(saveButton().disabled).toBe(true);
  expect(invoke).toHaveBeenCalledWith("get_codex_settings", {});
  expect(invoke.mock.calls.every(([op]) => !op.startsWith("preview_") && op !== "apply_write_plan")).toBe(
    true,
  );
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
