// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { MemoryFile, Snapshot } from "@/lib/types";
import { useUiStore } from "@/state/uiStore";
import { Sidebar } from "./Sidebar";

const { snapshotQuery } = vi.hoisted(() => ({ snapshotQuery: vi.fn() }));
vi.mock("@/lib/queries", () => ({
  useSnapshot: snapshotQuery,
  useAppSettings: () => ({ data: {} }),
}));

function memory(source: MemoryFile["source"], agent: "codex" | "claude"): MemoryFile {
  return {
    id: source,
    name: "memory.md",
    source,
    category: "memory",
    agents: [agent],
    scopes: [{ type: "user" }],
    paths: [`/profile/${source}/memory.md`],
    file: `/profile/${source}/memory.md`,
    dir: `/profile/${source}`,
    contentHash: source,
    generated: true,
  };
}

function view() {
  const snapshot: Snapshot = {
    generatedAt: "",
    homes: {
      home: "/profile",
      codexHome: "/profile/custom-codex",
      claudeDir: "/profile/.claude",
      claudeState: "/profile/.claude.json",
      agentsDir: "/profile/.agents",
      sandbox: true,
      readOnly: false,
    },
    skills: [],
    mcpServers: [],
    plugins: [],
    subagents: [],
    commands: [],
    hooks: [],
    automations: [],
    memories: [
      memory("codexMemory", "codex"),
      memory("automation", "codex"),
      memory("claudeProject", "claude"),
    ],
    dupGroups: [],
    projects: [],
    warnings: [],
    fileHashes: {},
    scanMillis: 1,
  };
  snapshotQuery.mockReturnValue({ data: snapshot });
  render(<Sidebar />);
}

async function click(name: string) {
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name }));
  });
}

afterEach(() => {
  cleanup();
  useUiStore.setState(useUiStore.getInitialState(), true);
});

it("expands source subitems with stable counts and configured-path tooltips", async () => {
  view();
  expect(screen.queryByRole("list", { name: "Memory sources" })).toBeNull();
  await click("Memories");
  expect(screen.getByRole("list", { name: "Memory sources" })).toBeTruthy();
  const parent = screen.getByRole("button", { name: "Memories" });
  expect(parent.getAttribute("aria-expanded")).toBe("true");
  expect(within(parent).getByText("3")).toBeTruthy();
  const core = screen.getByRole("button", { name: "Core Codex" });
  expect(core.title).toContain("/profile/custom-codex/memories");
  await click("Core Codex");
  expect(core.getAttribute("aria-current")).toBe("page");
  expect(parent.hasAttribute("aria-current")).toBe(false);
  expect(within(parent).getByText("3")).toBeTruthy();
  expect(within(screen.getByRole("button", { name: "Automation memories" })).getByText("1")).toBeTruthy();
  expect(
    within(screen.getByRole("button", { name: "Claude subagent memories" })).getByText("0"),
  ).toBeTruthy();
  await click("Codex");
  expect(within(parent).getByText("2")).toBeTruthy();
  expect(within(screen.getByRole("button", { name: "Claude project memories" })).getByText("0")).toBeTruthy();
});

it("guards source navigation and returns to all sources from the collapsed parent", async () => {
  view();
  await click("Memories");
  await click("Core Codex");
  act(() => {
    useUiStore.getState().select("current-memory");
    useUiStore.getState().setDetailTab("editor");
    useUiStore.getState().setNavigationGuard(() => false);
  });
  await click("Automation memories");
  expect(useUiStore.getState().memorySource).toBe("codexMemory");
  expect(useUiStore.getState().selectedId).toBe("current-memory");
  expect(screen.getByRole("button", { name: "Core Codex" }).getAttribute("aria-current")).toBe("page");
  act(() => {
    useUiStore.getState().setNavigationGuard(null);
    useUiStore.getState().toggleSidebar();
  });
  expect(screen.queryByRole("list", { name: "Memory sources" })).toBeNull();
  expect(useUiStore.getState().memorySource).toBe("codexMemory");
  await click("Memories");
  expect(useUiStore.getState().memorySource).toBe("all");
  act(() => useUiStore.getState().toggleSidebar());
  expect(screen.getByRole("list", { name: "Memory sources" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Memories" }).getAttribute("aria-current")).toBe("page");
});
