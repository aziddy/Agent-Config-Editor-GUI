// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { App } from "@/App";
import { queryKeys } from "@/lib/queries";
import type { AppSettings, FileText, Snapshot, WritePlan } from "@/lib/types";
import { usePlanStore } from "@/state/planStore";
import { useUiStore } from "@/state/uiStore";
import { useUnsavedChangesStore } from "@/state/unsavedChangesStore";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@/lib/rpc", () => ({ rpc: invoke }));
vi.mock("@/lib/events", () => ({ useConfigChangedListener: () => {} }));
vi.mock("@/components/CodeEditor", () => ({
  CodeEditor: ({
    value,
    language,
    readOnly,
    onChange,
  }: {
    value: string;
    language: string;
    readOnly: boolean;
    onChange: (text: string) => void;
  }) => (
    <textarea
      aria-label="File text"
      data-language={language}
      value={value}
      readOnly={readOnly}
      onChange={(event) => onChange(event.target.value)}
    />
  ),
}));

const original = '# Keep this comment\nmodel = "before"\n';
const edited = '# Keep this comment\nmodel = "after"\n';
const clients: QueryClient[] = [];

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
  codexHome = "/profile/custom-codex",
  sandbox = false,
  readOnly = false,
  readError = false,
} = {}) {
  const path = `${codexHome.replace(/\/+$/, "")}/config.toml`;
  let disk: FileText = { path, text: original, hash: "original", mtimeMs: 1, mode: 0o600, language: "toml" };
  const snapshot: Snapshot = {
    generatedAt: "",
    homes: {
      home: "/profile",
      claudeDir: "/profile/.claude",
      claudeState: "/profile/.claude.json",
      codexHome,
      agentsDir: "/profile/.agents",
      sandbox,
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
  const settings: AppSettings = {
    editorCommand: null,
    codexBinary: null,
    backupLimit: 5,
    extraSkillRoots: [],
    theme: "light",
    showSystemSkills: false,
    showAgentsDirSkills: true,
    showMissingProjects: false,
  };
  const plan = (newText: string): WritePlan => ({
    path,
    newText,
    baseHash: disk.hash,
    unifiedDiff: `--- config.toml\n+++ config.toml\n-${disk.text}+${newText}`,
    mode: 0o600,
    warnings: [],
    extraFiles: [],
    mutation: { type: "rawText" },
    description: "Save config.toml",
  });
  invoke.mockImplementation(async (op, args) => {
    switch (op) {
      case "scan_all":
        return snapshot;
      case "get_app_settings":
        return settings;
      case "get_file_text":
        if (readError) {
          readError = false;
          throw new Error("Config could not be read");
        }
        return disk;
      case "preview_save_file_text":
        return plan(args.text);
      case "apply_write_plan":
        disk = {
          ...disk,
          text: args.plan.newText,
          hash: args.plan.newText === original ? "restored" : "saved",
        };
        return { path, newHash: disk.hash, backupPath: "/backups/config.toml" };
      case "list_backups":
        return [{ id: "backup", createdAt: "2026-01-01T00:00:00Z", size: original.length }];
      case "preview_restore_backup":
        return plan(original);
      default:
        throw new Error(`Unexpected RPC: ${op}`);
    }
  });
  useUiStore.setState({
    nav: "settings",
    agentFilter: "claude",
    locationFilter: { type: "project", root: "/work/project" },
    entityListCollapsed: true,
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
    path,
    changeDisk(text: string) {
      disk = { ...disk, text, hash: "external" };
    },
  };
}

async function openConfig() {
  fireEvent.click(screen.getByRole("button", { name: "Codex Config" }));
  return (await screen.findByRole("textbox", { name: "File text" })) as HTMLTextAreaElement;
}

it.each([
  { codexHome: "/profile/.codex", sandbox: false, path: "/profile/.codex/config.toml" },
  { codexHome: "/custom/codex/", sandbox: false, path: "/custom/codex/config.toml" },
  { codexHome: "/sandbox/.codex", sandbox: true, path: "/sandbox/.codex/config.toml" },
])("opens $path regardless of agent and project filters", async ({ codexHome, sandbox, path }) => {
  view({ codexHome, sandbox });
  const editor = await openConfig();
  expect(invoke).toHaveBeenCalledWith("get_file_text", { path });
  expect(editor.value).toBe(original);
  expect(editor.dataset.language).toBe("toml");
  expect(screen.getByRole("heading", { name: "Codex Config" })).toBeTruthy();
  expect(screen.queryByPlaceholderText(/Search/)).toBeNull();
  expect(screen.queryByRole("button", { name: /item list|duplicates/i })).toBeNull();
  expect(useUiStore.getState()).toMatchObject({
    agentFilter: "claude",
    locationFilter: { type: "project", root: "/work/project" },
    entityListCollapsed: true,
  });
  fireEvent.click(screen.getByRole("button", { name: "Collapse navigation" }));
  const entry = screen.getByRole("button", { name: "Codex Config" });
  expect(entry.title).toBe("Codex Config");
  expect(entry.getAttribute("aria-current")).toBe("page");
});

it("previews edits before applying, keeps cancelled drafts, and restores backups through a preview", async () => {
  const { path } = view();
  const editor = await openConfig();
  fireEvent.change(editor, { target: { value: edited } });
  fireEvent.click(screen.getByRole("button", { name: "Save…" }));
  let dialog = await screen.findByRole("dialog", { name: "Save config.toml" });
  expect(invoke).toHaveBeenCalledWith("preview_save_file_text", { path, text: edited, baseHash: "original" });
  expect(invoke.mock.calls.some(([op]) => op === "apply_write_plan")).toBe(false);
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await waitFor(() =>
    expect((screen.getByRole("button", { name: "Save…" }) as HTMLButtonElement).disabled).toBe(false),
  );
  expect(editor.value).toBe(edited);
  fireEvent.click(screen.getByRole("button", { name: "Save…" }));
  dialog = await screen.findByRole("dialog", { name: "Save config.toml" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(screen.queryByText(/unsaved changes/)).toBeNull());
  expect(invoke.mock.calls.filter(([op]) => op === "apply_write_plan")).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: "Backups" }));
  fireEvent.click(await screen.findByRole("button", { name: "Restore…" }));
  dialog = await screen.findByRole("dialog", { name: "Save config.toml" });
  expect(invoke).toHaveBeenCalledWith("preview_restore_backup", { id: "backup" });
  expect(invoke.mock.calls.filter(([op]) => op === "apply_write_plan")).toHaveLength(1);
  fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
  await screen.findByText("This file changed on disk.");
  fireEvent.click(screen.getByRole("button", { name: "Reload" }));
  expect(editor.value).toBe(original);
});

it("keeps unsaved edits on cancelled navigation and waits for Apply before leaving", async () => {
  view();
  const editor = await openConfig();
  fireEvent.change(editor, { target: { value: edited } });
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  let dialog = await screen.findByRole("dialog", { name: "Unsaved changes" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(useUiStore.getState().navigationPending).toBe(false));
  expect(useUiStore.getState().nav).toBe("codexConfig");
  expect(editor.value).toBe(edited);
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  dialog = await screen.findByRole("dialog", { name: "Unsaved changes" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Save…" }));
  dialog = await screen.findByRole("dialog", { name: "Save config.toml" });
  expect(useUiStore.getState().nav).toBe("codexConfig");
  fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
  await screen.findByRole("heading", { name: "Settings" });
  expect(useUiStore.getState().nav).toBe("settings");
});

it("blocks conflicting saves until an explicit reload", async () => {
  const { client, path, changeDisk } = view();
  const editor = await openConfig();
  fireEvent.change(editor, { target: { value: edited } });
  changeDisk('model = "external"\n');
  await act(async () => {
    await client.invalidateQueries({ queryKey: queryKeys.file(path) });
  });
  await screen.findByText(/Saving is blocked until you reload/);
  expect(editor.value).toBe(edited);
  expect((screen.getByRole("button", { name: "Save…" }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Reload" }));
  expect(editor.value).toBe('model = "external"\n');
});

it("keeps the config and backup restores read-only", async () => {
  view({ readOnly: true });
  const editor = await openConfig();
  expect(editor.readOnly).toBe(true);
  expect((screen.getByRole("button", { name: "Save…" }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Backups" }));
  const restore = (await screen.findByRole("button", { name: "Restore…" })) as HTMLButtonElement;
  expect(restore.disabled).toBe(true);
  expect(invoke.mock.calls.every(([op]) => !op.startsWith("preview_") && op !== "apply_write_plan")).toBe(
    true,
  );
});

it("shows an unreadable config error and retries without creating a file", async () => {
  const { path } = view({ readError: true });
  fireEvent.click(screen.getByRole("button", { name: "Codex Config" }));
  expect((await screen.findByRole("alert")).textContent).toContain("Config could not be read");
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  const editor = (await screen.findByRole("textbox", { name: "File text" })) as HTMLTextAreaElement;
  expect(editor.value).toBe(original);
  expect(invoke).toHaveBeenCalledWith("get_file_text", { path });
  expect(invoke.mock.calls.every(([op]) => !op.startsWith("preview_") && op !== "apply_write_plan")).toBe(
    true,
  );
});
