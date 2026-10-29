// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { App } from "@/App";
import { DEFAULT_SKILL_FILTERS } from "@/lib/entities";
import type { Skill, Snapshot } from "@/lib/types";
import { useUiStore } from "@/state/uiStore";

const { snapshotQuery, settingsQuery } = vi.hoisted(() => ({
  snapshotQuery: vi.fn(),
  settingsQuery: vi.fn(),
}));
vi.mock("@/lib/queries", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/queries")>()),
  useSnapshot: snapshotQuery,
  useAppSettings: settingsQuery,
}));
vi.mock("@/lib/events", () => ({ useConfigChangedListener: () => {} }));

const clients: QueryClient[] = [];
beforeEach(() => {
  vi.stubGlobal("matchMedia", () => ({ matches: false, addEventListener() {}, removeEventListener() {} }));
  settingsQuery.mockReturnValue({ data: { showSystemSkills: true, showAgentsDirSkills: true } });
});
afterEach(() => {
  cleanup();
  for (const client of clients.splice(0)) client.clear();
  useUiStore.setState(useUiStore.getInitialState(), true);
  vi.unstubAllGlobals();
});

function skill(id: string, overrides: Partial<Skill> = {}): Skill {
  return {
    id,
    name: id,
    description: null,
    agent: "codex",
    scope: { type: "user" },
    origin: {
      agent: "codex",
      scope: { type: "user" },
      file: `/profile/.codex/skills/${id}/SKILL.md`,
      locator: { type: "markdownFile" },
    },
    dir: `/profile/.codex/skills/${id}`,
    enabled: { enabled: true, editable: false, toggle: null, reason: null },
    allowImplicitInvocation: true,
    frontmatter: { fields: {}, raw: "", present: false },
    isSystem: false,
    lockManaged: false,
    contentHash: id,
    dupGroup: null,
    bodyPreview: "",
    ...overrides,
  };
}

function view() {
  const data: Snapshot = {
    generatedAt: "",
    homes: {
      home: "/profile",
      codexHome: "/profile/.codex",
      claudeDir: "/profile/.claude",
      claudeState: "/profile/.claude.json",
      agentsDir: "/profile/.agents",
      sandbox: true,
      readOnly: true,
    },
    skills: [
      skill("Automatic"),
      skill("Manual", {
        allowImplicitInvocation: false,
        enabled: { enabled: false, editable: false, toggle: null, reason: null },
      }),
      skill("Claude manual", {
        agent: "claude",
        allowImplicitInvocation: false,
        enabled: { enabled: null, editable: false, toggle: null, reason: "No enable switch" },
      }),
      skill("Unreadable", { allowImplicitInvocation: null }),
      skill("Built-in", { isSystem: true, scope: { type: "system" } }),
      skill("Installed", { lockManaged: true, allowImplicitInvocation: false }),
    ],
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
  snapshotQuery.mockReturnValue({ data, isPending: false, isFetching: false });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  clients.push(client);
  return {
    data,
    ...render(
      <QueryClientProvider client={client}>
        <App />
      </QueryClientProvider>,
    ),
  };
}

async function choose(name: string, value: string) {
  await act(async () => fireEvent.change(screen.getByRole("combobox", { name }), { target: { value } }));
}

it("combines visible controls with the skill list, result count, and stable sidebar total", async () => {
  view();
  expect(within(screen.getByRole("group", { name: "Skill filters" })).getAllByRole("combobox")).toHaveLength(
    4,
  );
  expect(screen.getByRole("status").textContent).toBe("6 skills match");
  await choose("Implicit invocation", "blocked");
  expect(screen.getByRole("status").textContent).toBe("3 skills match");
  expect(screen.queryByText("Automatic")).toBeNull();
  expect(screen.getByText("Claude manual")).toBeTruthy();
  expect(within(screen.getByRole("button", { name: "Skills" })).getByText("6")).toBeTruthy();
  await choose("Enabled state", "disabled");
  expect(screen.getByRole("status").textContent).toBe("1 skill matches");
  expect(screen.getByText("Manual")).toBeTruthy();
  expect(screen.queryByText("Claude manual")).toBeNull();
  await choose("Installer-managed", "yes");
  expect(screen.getByText("No skills match.")).toBeTruthy();
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Clear skill filters" })));
  expect(screen.getByRole("status").textContent).toBe("6 skills match");
  expect(useUiStore.getState().skillFilters).toEqual(DEFAULT_SKILL_FILTERS);
  expect(screen.queryByRole("button", { name: "Clear skill filters" })).toBeNull();
});

it("offers unknown invocation and Claude's absent enable switch as distinct filters", async () => {
  view();
  await choose("Implicit invocation", "unknown");
  expect(screen.getByText("Unreadable")).toBeTruthy();
  expect(screen.getByRole("status").textContent).toBe("1 skill matches");
  await choose("Implicit invocation", "all");
  await choose("Enabled state", "notApplicable");
  expect(screen.getByText("Claude manual")).toBeTruthy();
  expect(screen.queryByText("Automatic")).toBeNull();
});

it("keeps the displayed control and draft unchanged when navigation is cancelled", async () => {
  view();
  act(() =>
    useUiStore.setState({ selectedId: "unsaved", detailTab: "editor", navigationGuard: () => false }),
  );
  await choose("Implicit invocation", "blocked");
  expect((screen.getByRole("combobox", { name: "Implicit invocation" }) as HTMLSelectElement).value).toBe(
    "all",
  );
  expect(screen.getByRole("status").textContent).toBe("6 skills match");
  expect(useUiStore.getState()).toMatchObject({ selectedId: "unsaved", detailTab: "editor" });
});

it("explains visibility settings and keeps filter controls limited to Skills", async () => {
  settingsQuery.mockReturnValue({ data: { showSystemSkills: false, showAgentsDirSkills: false } });
  view();
  await choose("Built-in", "yes");
  expect(screen.getByText("No skills match.")).toBeTruthy();
  expect(screen.getByText(/Enable “Show Codex built-in skills”/)).toBeTruthy();
  await choose("Installer-managed", "yes");
  expect(screen.getByText(/Enable “Show ~\/.agents skills”/)).toBeTruthy();
  await act(async () => useUiStore.getState().setNav("mcpServer"));
  expect(screen.queryByRole("group", { name: "Skill filters" })).toBeNull();
  await act(async () => useUiStore.getState().setNav("skill"));
  expect((screen.getByRole("combobox", { name: "Built-in" }) as HTMLSelectElement).value).toBe("yes");
});

it("updates matching results when a refreshed snapshot changes invocation metadata", async () => {
  const { data, rerender } = view();
  await choose("Implicit invocation", "unknown");
  const updated = {
    ...data,
    skills: data.skills.map((entry) =>
      entry.id === "Unreadable" ? { ...entry, allowImplicitInvocation: true } : entry,
    ),
  };
  snapshotQuery.mockReturnValue({ data: updated, isPending: false, isFetching: false });
  const client = clients[0]!;
  rerender(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
  expect(screen.getByText("No skills match.")).toBeTruthy();
  expect(screen.getByRole("status").textContent).toBe("0 skills match");
});
