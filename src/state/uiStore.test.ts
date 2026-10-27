import { afterEach, describe, expect, it, vi } from "vitest";
import { useUiStore } from "./uiStore";

afterEach(() => useUiStore.setState(useUiStore.getInitialState(), true));

it("starts expanded and folds each sidebar without resetting the current edit or filters", () => {
  const actions = useUiStore.getState();
  expect(actions).toMatchObject({ sidebarCollapsed: false, entityListCollapsed: false });
  actions.setAgentFilter("codex");
  actions.setLocationFilter({ type: "project", root: "/work/repo" });
  actions.select("project-skill");
  actions.setDetailTab("editor");
  actions.setSearch("shared");
  actions.toggleDuplicates();

  const editState = {
    agentFilter: "codex",
    locationFilter: { type: "project", root: "/work/repo" },
    selectedId: "project-skill",
    detailTab: "editor",
    search: "shared",
    showDuplicates: true,
  };
  actions.toggleSidebar();
  expect(useUiStore.getState()).toMatchObject({
    ...editState,
    sidebarCollapsed: true,
    entityListCollapsed: false,
  });
  actions.toggleEntityList();
  expect(useUiStore.getState()).toMatchObject({
    ...editState,
    sidebarCollapsed: true,
    entityListCollapsed: true,
  });
  actions.toggleSidebar();
  expect(useUiStore.getState()).toMatchObject({
    ...editState,
    sidebarCollapsed: false,
    entityListCollapsed: true,
  });
  actions.toggleEntityList();
  expect(useUiStore.getState()).toMatchObject({
    ...editState,
    sidebarCollapsed: false,
    entityListCollapsed: false,
  });
});

it("reopens the list on category changes while retaining the navigation rail", () => {
  const actions = useUiStore.getState();
  actions.toggleSidebar();
  actions.toggleEntityList();
  actions.setNav("subAgent");
  expect(useUiStore.getState()).toMatchObject({
    nav: "subAgent",
    sidebarCollapsed: true,
    entityListCollapsed: false,
  });
  actions.toggleEntityList();
  actions.setNav("projects");
  actions.setNav("settings");
  expect(useUiStore.getState()).toMatchObject({
    sidebarCollapsed: true,
    entityListCollapsed: true,
  });
  actions.setNav("skill");
  expect(useUiStore.getState()).toMatchObject({
    nav: "skill",
    sidebarCollapsed: true,
    entityListCollapsed: false,
  });
});

it("retains location across navigation and agents, and clears details when location changes", () => {
  const actions = useUiStore.getState();
  expect(actions.locationFilter).toEqual({ type: "all" });
  actions.select("user-skill");
  actions.setDetailTab("editor");
  actions.setSearch("shared");
  actions.setLocationFilter({ type: "project", root: "/work/repo" });
  expect(useUiStore.getState()).toMatchObject({
    locationFilter: { type: "project", root: "/work/repo" },
    selectedId: null,
    detailTab: "overview",
    search: "shared",
  });
  actions.setNav("subAgent");
  actions.setAgentFilter("codex");
  expect(useUiStore.getState().locationFilter).toEqual({ type: "project", root: "/work/repo" });
  actions.select("project-agent");
  actions.setLocationFilter({ type: "all" });
  expect(useUiStore.getState()).toMatchObject({ locationFilter: { type: "all" }, selectedId: null });
});

function editingState() {
  const actions = useUiStore.getState();
  actions.select("current-skill");
  actions.setDetailTab("editor");
  actions.setSearch("shared");
  return actions;
}

describe("editor navigation guard", () => {
  const navigations = [
    { name: "entity selection", navigate: () => useUiStore.getState().select("other-skill") },
    { name: "category", navigate: () => useUiStore.getState().setNav("automation") },
    { name: "agent filter", navigate: () => useUiStore.getState().setAgentFilter("codex") },
    { name: "memory source", navigate: () => useUiStore.getState().navigateToMemorySource("automation") },
    {
      name: "location filter",
      navigate: () => useUiStore.getState().setLocationFilter({ type: "project", root: "/work/repo" }),
    },
    {
      name: "entity link",
      navigate: () => useUiStore.getState().navigateToEntity("plugin", "provider-plugin"),
    },
  ];

  it.each(navigations)(
    "cancelling $name preserves the selected entity, tab and filters",
    async ({ navigate }) => {
      const actions = editingState();
      const before = useUiStore.getState();
      const guard = vi.fn(() => false);
      actions.setNavigationGuard(guard);

      expect(await navigate()).toBe(false);
      expect(guard).toHaveBeenCalledOnce();
      expect(useUiStore.getState()).toMatchObject({
        selectedId: before.selectedId,
        nav: before.nav,
        agentFilter: before.agentFilter,
        locationFilter: before.locationFilter,
        memorySource: before.memorySource,
        detailTab: before.detailTab,
        search: before.search,
        navigationPending: false,
      });
    },
  );

  it("waits for a successful save and Apply before committing navigation", async () => {
    const actions = editingState();
    let completeSave: (saved: boolean) => void = () => {};
    actions.setNavigationGuard(() => new Promise((resolve) => (completeSave = resolve)));

    const navigation = actions.select("other-skill");
    expect(useUiStore.getState()).toMatchObject({ selectedId: "current-skill", navigationPending: true });
    completeSave(true);
    expect(await navigation).toBe(true);
    expect(useUiStore.getState()).toMatchObject({
      selectedId: "other-skill",
      detailTab: "overview",
      navigationPending: false,
    });
  });

  it("does not navigate when a save preview is cancelled", async () => {
    const actions = editingState();
    actions.setNavigationGuard(async () => false);
    expect(await actions.select("other-skill")).toBe(false);
    expect(useUiStore.getState()).toMatchObject({
      selectedId: "current-skill",
      detailTab: "editor",
      navigationPending: false,
    });
  });

  it("retains the selection when saving fails and allows another navigation attempt", async () => {
    const actions = editingState();
    const guard = vi
      .fn<() => Promise<boolean>>()
      .mockRejectedValueOnce(new Error("File changed on disk"))
      .mockResolvedValueOnce(true);
    actions.setNavigationGuard(guard);

    expect(await actions.select("other-skill")).toBe(false);
    expect(useUiStore.getState()).toMatchObject({ selectedId: "current-skill", navigationPending: false });
    expect(await actions.select("other-skill")).toBe(true);
    expect(useUiStore.getState().selectedId).toBe("other-skill");
  });

  it("continues after Discard without introducing a second prompt or intermediate selection", async () => {
    const actions = editingState();
    const guard = vi.fn(() => true);
    actions.setNavigationGuard(guard);
    const selections: Array<{ nav: string; selectedId: string | null }> = [];
    const unsubscribe = useUiStore.subscribe((state) => {
      selections.push({ nav: state.nav, selectedId: state.selectedId });
    });

    expect(await actions.navigateToEntity("plugin", "provider-plugin")).toBe(true);
    unsubscribe();
    expect(guard).toHaveBeenCalledOnce();
    expect(selections).toContainEqual({ nav: "plugin", selectedId: "provider-plugin" });
    expect(selections).not.toContainEqual({ nav: "plugin", selectedId: null });
    expect(useUiStore.getState()).toMatchObject({
      nav: "plugin",
      selectedId: "provider-plugin",
      search: "",
      detailTab: "overview",
    });
  });

  it("guards file changes through the same navigation action", async () => {
    const actions = editingState();
    const openFile = vi.fn();
    actions.setNavigationGuard(() => false);
    expect(await actions.requestNavigation(openFile)).toBe(false);
    expect(openFile).not.toHaveBeenCalled();
    actions.setNavigationGuard(() => true);
    expect(await actions.requestNavigation(openFile)).toBe(true);
    expect(openFile).toHaveBeenCalledOnce();
  });

  it("ignores repeated navigation while the first decision or save is pending", async () => {
    const actions = editingState();
    let finish: (allowed: boolean) => void = () => {};
    const guard = vi.fn(() => new Promise<boolean>((resolve) => (finish = resolve)));
    actions.setNavigationGuard(guard);

    const first = actions.select("other-skill");
    expect(await actions.setNav("plugin")).toBe(false);
    expect(await actions.setAgentFilter("codex")).toBe(false);
    expect(guard).toHaveBeenCalledOnce();
    finish(true);
    expect(await first).toBe(true);
    expect(useUiStore.getState()).toMatchObject({
      nav: "skill",
      selectedId: "other-skill",
      agentFilter: "all",
    });
  });

  it("keeps the draft tab on same-selection navigation without asking to leave", async () => {
    const actions = editingState();
    const guard = vi.fn(() => false);
    actions.setNavigationGuard(guard);

    await actions.select("current-skill");
    await actions.setNav("skill");
    await actions.navigateToEntity("skill", "current-skill");
    await actions.setAgentFilter("all");
    await actions.setLocationFilter({ type: "all" });
    expect(guard).not.toHaveBeenCalled();
    expect(useUiStore.getState()).toMatchObject({ selectedId: "current-skill", detailTab: "editor" });
  });

  it("lets Overview, sidebar folding and tree folding preserve the mounted editor without prompting", () => {
    const actions = editingState();
    const guard = vi.fn(() => false);
    actions.setNavigationGuard(guard);
    actions.setDetailTab("overview");
    actions.setDetailTab("runs");
    actions.toggleSidebar();
    actions.toggleEntityList();
    actions.toggleEditorTree();
    expect(guard).not.toHaveBeenCalled();
    expect(useUiStore.getState().selectedId).toBe("current-skill");
  });
});

it("retains the tree width and folded state across entity changes and clamps widths", () => {
  const actions = useUiStore.getState();
  expect(actions).toMatchObject({ editorTreeWidth: 220, editorTreeCollapsed: false });
  actions.setEditorTreeWidth(275);
  actions.toggleEditorTree();
  actions.select("a-skill");
  actions.setNav("automation");
  actions.select("an-automation");
  expect(useUiStore.getState()).toMatchObject({ editorTreeWidth: 275, editorTreeCollapsed: true });
  actions.setEditorTreeWidth(500);
  expect(useUiStore.getState().editorTreeWidth).toBe(360);
  actions.setEditorTreeWidth(100);
  expect(useUiStore.getState().editorTreeWidth).toBe(140);
  actions.setEditorTreeWidth(Number.NaN);
  expect(useUiStore.getState().editorTreeWidth).toBe(140);
});

it("changes memory source only after the draft guard allows navigation", async () => {
  const actions = useUiStore.getState();
  await actions.setNav("memory");
  await actions.select("memory-file");
  actions.setDetailTab("editor");
  actions.setNavigationGuard(() => false);
  expect(await actions.navigateToMemorySource("automation")).toBe(false);
  expect(useUiStore.getState()).toMatchObject({
    memorySource: "all",
    selectedId: "memory-file",
    detailTab: "editor",
  });
  actions.setNavigationGuard(() => true);
  expect(await actions.navigateToMemorySource("automation")).toBe(true);
  expect(useUiStore.getState()).toMatchObject({
    memorySource: "automation",
    selectedId: null,
    detailTab: "overview",
  });
});

it("enters a memory source atomically and returns to all sources through the parent", async () => {
  const actions = editingState();
  const guard = vi.fn(() => true);
  actions.setNavigationGuard(guard);
  const destinations: string[] = [];
  const unsubscribe = useUiStore.subscribe((state) => {
    if (state.nav === "memory") destinations.push(state.memorySource);
  });
  expect(await actions.navigateToMemorySource("codexMemory")).toBe(true);
  unsubscribe();
  expect(guard).toHaveBeenCalledOnce();
  expect(destinations.every((source) => source === "codexMemory")).toBe(true);
  expect(useUiStore.getState()).toMatchObject({
    nav: "memory",
    memorySource: "codexMemory",
    search: "",
    selectedId: null,
    entityListCollapsed: false,
  });
  await actions.select("core-file");
  actions.setDetailTab("editor");
  guard.mockClear();
  expect(await actions.navigateToMemorySource("codexMemory")).toBe(true);
  expect(guard).not.toHaveBeenCalled();
  actions.toggleSidebar();
  expect(useUiStore.getState().memorySource).toBe("codexMemory");
  actions.setNavigationGuard(() => false);
  expect(await actions.setNav("memory")).toBe(false);
  expect(useUiStore.getState()).toMatchObject({
    memorySource: "codexMemory",
    selectedId: "core-file",
    detailTab: "editor",
  });
  actions.setNavigationGuard(() => true);
  expect(await actions.setNav("memory")).toBe(true);
  expect(useUiStore.getState()).toMatchObject({
    memorySource: "all",
    selectedId: null,
    sidebarCollapsed: true,
  });
});

it("clears source filtering for direct memory links and resets incompatible sources on provider changes", async () => {
  const actions = useUiStore.getState();
  await actions.navigateToMemorySource("codexMemory");
  await actions.navigateToEntity("memory", "automation-file");
  expect(useUiStore.getState()).toMatchObject({
    nav: "memory",
    memorySource: "all",
    selectedId: "automation-file",
  });
  await actions.navigateToMemorySource("codexMemory");
  actions.setNavigationGuard(() => false);
  expect(await actions.setAgentFilter("claude")).toBe(false);
  expect(useUiStore.getState()).toMatchObject({ agentFilter: "all", memorySource: "codexMemory" });
  actions.setNavigationGuard(null);
  await actions.setAgentFilter("claude");
  expect(useUiStore.getState()).toMatchObject({ agentFilter: "claude", memorySource: "all" });
  await actions.navigateToMemorySource("claudeProject");
  await actions.setAgentFilter("all");
  expect(useUiStore.getState().memorySource).toBe("claudeProject");
  await actions.setAgentFilter("codex");
  expect(useUiStore.getState().memorySource).toBe("all");
  await actions.navigateToMemorySource("instruction");
  await actions.setAgentFilter("claude");
  expect(useUiStore.getState().memorySource).toBe("instruction");
});
