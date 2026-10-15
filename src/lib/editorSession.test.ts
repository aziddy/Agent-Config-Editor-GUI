import { afterEach, expect, it, vi } from "vitest";
import { useUiStore } from "@/state/uiStore";
import { useUnsavedChangesStore } from "@/state/unsavedChangesStore";
import {
  type EditorSession,
  editorDirty,
  emptyEditorSession,
  editorSessionReducer as reduce,
  resolveEditorNavigation,
} from "./editorSession";
import type { FileText } from "./types";

const original: FileText = {
  path: "/skills/sample/SKILL.md",
  text: "original",
  hash: "original-hash",
  language: "markdown",
  mode: 0o600,
  mtimeMs: 1,
};

afterEach(() => {
  useUiStore.setState(useUiStore.getInitialState(), true);
  useUnsavedChangesStore.getState().finish("cancel");
});

it("loads a file and keeps a dirty buffer and base hash across external changes", () => {
  const loaded = reduce(emptyEditorSession, { type: "loaded", file: original });
  expect(loaded.draft).toBe("original");
  expect(editorDirty(loaded)).toBe(false);
  const edited = reduce(loaded, { type: "edit", text: "draft" });
  const changed = { ...original, text: "external", hash: "external-hash" };
  expect(reduce(edited, { type: "loaded", file: changed })).toBe(edited);
  expect(editorDirty(edited)).toBe(true);
  expect(reduce(edited, { type: "reload", file: changed })).toEqual({ base: changed, draft: "external" });
});

it("keeps a clean buffer on external changes until explicit reload", () => {
  const loaded = reduce(emptyEditorSession, { type: "loaded", file: original });
  expect(reduce(loaded, { type: "loaded", file: { ...original, text: "external" } })).toBe(loaded);
  expect(editorDirty(loaded)).toBe(false);
});

it("only marks a successful apply clean, including while the old file query is still cached", () => {
  const edited = reduce(
    { base: original, draft: "draft" },
    { type: "saved", text: "draft", hash: "new-hash" },
  );
  expect(editorDirty(edited)).toBe(false);
  expect(edited.base?.hash).toBe("new-hash");
  expect(reduce(edited, { type: "loaded", file: original })).toBe(edited);
  const failed = { base: original, draft: "draft" };
  expect(reduce(failed, { type: "saved", text: "draft", hash: null })).toBe(failed);
});

it("can discard a buffer after the file becomes unavailable or unreadable", () => {
  expect(reduce({ base: original, draft: "draft" }, { type: "reload" })).toEqual({
    base: original,
    draft: "original",
  });
});

it.each(["cancel", "discard", "save"] as const)(
  "handles %s through the shared dirty-navigation prompt",
  async (choice) => {
    const ui = useUiStore.getState();
    await ui.select("current");
    let session: EditorSession = { base: original, draft: "draft" };
    const save = vi.fn(async () => {
      session = reduce(session, { type: "saved", text: "draft", hash: "saved-hash" });
      return true;
    });
    const discard = vi.fn(() => {
      session = reduce(session, { type: "reload" });
    });
    ui.setNavigationGuard(async () =>
      resolveEditorNavigation(await useUnsavedChangesStore.getState().confirm(original.path), save, discard),
    );
    const navigation = ui.select("next");
    expect(useUiStore.getState().selectedId).toBe("current");
    expect(useUnsavedChangesStore.getState().path).toBe(original.path);
    useUnsavedChangesStore.getState().finish(choice);
    expect(await navigation).toBe(choice !== "cancel");
    expect(useUiStore.getState().selectedId).toBe(choice === "cancel" ? "current" : "next");
    expect(editorDirty(session)).toBe(choice === "cancel");
    expect(save).toHaveBeenCalledTimes(choice === "save" ? 1 : 0);
    expect(discard).toHaveBeenCalledTimes(choice === "discard" ? 1 : 0);
  },
);

it("keeps selection and draft when saving is cancelled or unsuccessful", async () => {
  const ui = useUiStore.getState();
  await ui.select("current");
  const session = { base: original, draft: "draft" };
  const discard = vi.fn();
  ui.setNavigationGuard(() => resolveEditorNavigation("save", async () => false, discard));
  expect(await ui.navigateToEntity("plugin", "next")).toBe(false);
  expect(useUiStore.getState()).toMatchObject({ nav: "skill", selectedId: "current" });
  expect(editorDirty(session)).toBe(true);
  expect(discard).not.toHaveBeenCalled();
});

it("waits for Apply before changing file and handles a rejected save without losing the draft", async () => {
  const ui = useUiStore.getState();
  let finishApply: (result: boolean) => void = () => {};
  const save = () =>
    new Promise<boolean>((resolve) => {
      finishApply = resolve;
    });
  const openNextFile = vi.fn();
  ui.setNavigationGuard(() => resolveEditorNavigation("save", save, vi.fn()));
  const navigation = ui.requestNavigation(openNextFile);
  expect(openNextFile).not.toHaveBeenCalled();
  finishApply(true);
  expect(await navigation).toBe(true);
  expect(openNextFile).toHaveBeenCalledOnce();
  ui.setNavigationGuard(() =>
    resolveEditorNavigation(
      "save",
      async () => {
        throw new Error("failed");
      },
      vi.fn(),
    ),
  );
  expect(await ui.requestNavigation(openNextFile)).toBe(false);
  expect(openNextFile).toHaveBeenCalledOnce();
});
