// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MemoryFile } from "@/lib/types";
import { MemoryDetail } from "./MemoryDetail";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@/lib/rpc", () => ({ rpc: invoke }));

const memory: MemoryFile = {
  id: "memory:fixture",
  name: "MEMORY.md",
  category: "memory",
  source: "codexMemory",
  agents: ["codex"],
  scopes: [{ type: "user" }],
  paths: ["/profile/memories/MEMORY.md"],
  file: "/shared/MEMORY.md",
  dir: "/profile/memories",
  contentHash: "hash",
  generated: true,
};

function view(data = memory) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const openFile = vi.fn();
  render(
    <QueryClientProvider client={client}>
      <MemoryDetail memory={data} onOpenFile={openFile} />
    </QueryClientProvider>,
  );
  return { client, openFile };
}

beforeEach(() => invoke.mockReset());
afterEach(cleanup);

describe("memory detail", () => {
  it("loads the complete file on demand and keeps Codex directives in its preview", async () => {
    const text = `# Saved context\n${"Earlier context.\n".repeat(150)}\nLast remembered fact.\n::archive-thread{}`;
    invoke.mockResolvedValue({
      path: memory.paths[0],
      text,
      hash: "hash",
      mode: 0o600,
      language: "markdown",
    });
    const { client, openFile } = view();
    expect(screen.getByText("Loading file…")).toBeTruthy();
    await screen.findByRole("heading", { name: "Saved context" });
    expect(screen.getByText(/Last remembered fact/)).toBeTruthy();
    expect(screen.getByText(/::archive-thread/)).toBeTruthy();
    expect(screen.getByText("/profile/memories/MEMORY.md")).toBeTruthy();
    expect(screen.getByText("/shared/MEMORY.md")).toBeTruthy();
    expect(invoke).toHaveBeenCalledWith("get_file_text", { path: "/profile/memories/MEMORY.md" });
    fireEvent.click(screen.getByRole("button", { name: "Edit memory" }));
    expect(openFile).toHaveBeenCalledWith("/profile/memories/MEMORY.md");
    expect(invoke.mock.calls.every(([op]) => op === "get_file_text")).toBe(true);
    client.clear();
  });

  it("explains empty and managed instruction files", async () => {
    invoke.mockResolvedValue({
      path: memory.paths[0],
      text: "",
      hash: "empty",
      mode: 0o644,
      language: "markdown",
    });
    const { client } = view({
      ...memory,
      category: "instruction",
      generated: false,
      scopes: [{ type: "managed" }],
    });
    await screen.findByText("This file is empty.");
    expect(screen.getByText("Managed instructions are read-only.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "View source" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Edit instructions" })).toBeNull();
    client.clear();
  });

  it("does not show a Codex regeneration notice for Claude memories", async () => {
    invoke.mockResolvedValue({
      path: memory.paths[0],
      text: "Claude's saved memory",
      hash: "hash",
      mode: 0o600,
      language: "markdown",
    });
    const { client } = view({ ...memory, source: "claudeProject", agents: ["claude"] });
    await screen.findByText("Claude's saved memory");
    expect(screen.queryByText(/Codex may regenerate/)).toBeNull();
    client.clear();
  });

  it("retains a useful read failure and lets the user retry", async () => {
    invoke.mockRejectedValueOnce(new Error("Memory file is unavailable"));
    const { client } = view();
    await screen.findByRole("alert");
    expect(screen.getByText("Memory file is unavailable")).toBeTruthy();
    invoke.mockResolvedValueOnce({
      path: memory.paths[0],
      text: "Recovered memory.",
      hash: "new",
      mode: 0o600,
      language: "markdown",
    });
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await screen.findByText("Recovered memory.");
    client.clear();
  });
});
