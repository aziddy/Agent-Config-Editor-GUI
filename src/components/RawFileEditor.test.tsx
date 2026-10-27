// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useUiStore } from "@/state/uiStore";
import { RawFileEditor } from "./RawFileEditor";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@/lib/rpc", () => ({ rpc: invoke }));
vi.mock("@/components/CodeEditor", () => ({
  CodeEditor: ({ value, readOnly }: { value: string; readOnly: boolean }) => (
    <textarea aria-label="File text" value={value} readOnly={readOnly} />
  ),
}));

afterEach(() => {
  cleanup();
  useUiStore.setState(useUiStore.getInitialState(), true);
  invoke.mockReset();
});

it("keeps managed source text and backup restores read-only", async () => {
  invoke.mockImplementation(async (op) =>
    op === "get_file_text"
      ? {
          path: "/managed/CLAUDE.md",
          text: "Managed guidance",
          hash: "hash",
          mode: 0o644,
          language: "markdown",
        }
      : [{ id: "backup", createdAt: "2026-01-01T00:00:00Z", size: 10 }],
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <RawFileEditor path="/managed/CLAUDE.md" readOnly readOnlyHint="Managed instructions are read-only." />
    </QueryClientProvider>,
  );
  const text = await screen.findByRole("textbox", { name: "File text" });
  expect((text as HTMLTextAreaElement).readOnly).toBe(true);
  expect((screen.getByRole("button", { name: "Save…" }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Backups" }));
  const restore = await screen.findByRole("button", { name: "Restore…" });
  expect((restore as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(restore);
  expect(invoke.mock.calls.every(([op]) => op === "get_file_text" || op === "list_backups")).toBe(true);
  client.clear();
});
