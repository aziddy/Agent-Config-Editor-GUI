// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Automation, AutomationRun, AutomationRunConversation, RunConversationItem } from "@/lib/types";
import { AutomationRuns } from "./AutomationRuns";

const { invoke } = vi.hoisted(() => ({
  invoke: vi.fn<(op: string, args: Record<string, unknown>) => Promise<unknown>>(),
}));
vi.mock("@/lib/rpc", () => ({ rpc: invoke }));

const automation: Automation = {
  id: "automation",
  name: "Daily review",
  agent: "codex",
  scope: { type: "user" },
  kind: "cron",
  status: "ACTIVE",
  prompt: "Current edited prompt",
  rrule: null,
  model: null,
  reasoningEffort: null,
  executionEnvironment: null,
  target: { type: "projectless" },
  cwds: [],
  createdAt: 1,
  updatedAt: 1,
  dir: "/fixture",
  origin: {
    agent: "codex",
    scope: { type: "user" },
    file: "/fixture/automation.toml",
    locator: { type: "tomlPath", path: [] },
  },
  memoryPath: null,
  reportPaths: [],
  extra: {},
};

const runs: AutomationRun[] = [
  {
    threadId: "new",
    title: "Latest recorded run",
    createdAt: 2,
    updatedAt: 2,
    status: "PENDING_REVIEW",
    readAt: null,
    summary: null,
    sourceCwd: null,
    archivedReason: null,
  },
  {
    threadId: "old",
    title: "Older recorded run",
    createdAt: 1,
    updatedAt: 1,
    status: "ARCHIVED",
    readAt: 1,
    summary: null,
    sourceCwd: null,
    archivedReason: "auto",
  },
];

function message(id: string, position: number, text: string): RunConversationItem {
  return {
    id,
    position,
    version: position,
    kind: "assistant",
    label: "Codex",
    text,
    phase: "final_answer",
    createdAt: null,
    nextCursor: null,
  };
}

function page(run: AutomationRun): AutomationRunConversation {
  return {
    run,
    items: [
      {
        id: "command",
        position: 1,
        version: 1,
        kind: "activity",
        label: "Ran command",
        text: null,
        phase: null,
        createdAt: null,
        nextCursor: null,
      },
      message("result", 2, `${run.threadId} final result`),
    ],
    beforeCursor: 1,
    afterCursor: 2,
    hasMoreAfter: false,
    availability: "local",
    notices: [],
    executionStatus: "completed",
    durationMs: 44000,
    historySource: "history:fixture",
    removedItemIds: [],
    reset: false,
  };
}

function view(
  active = true,
  client = new QueryClient({
    defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false }, mutations: { retry: false } },
  }),
) {
  return {
    client,
    ...render(
      <QueryClientProvider client={client}>
        <AutomationRuns automation={automation} active={active} />
      </QueryClientProvider>,
    ),
  };
}

beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  invoke.mockReset();
  invoke.mockImplementation(async (op, args) => {
    if (op === "list_automation_runs")
      return { runs, nextCursor: null, storageAvailable: true, warnings: [] };
    if (op === "get_automation_run_activity")
      return {
        details: { command: "echo test" },
        text: "$ echo test\n\nRecorded tool output",
        nextCursor: null,
      };
    const run = runs.find((run) => run.threadId === args.threadId) ?? runs[0]!;
    const result = page(run);
    if (args.before != null)
      return {
        ...result,
        items: [{ ...message("prompt", 0, "Original recorded prompt"), kind: "user", label: "You" }],
        beforeCursor: null,
      };
    if (args.after != null) return { ...result, items: [] };
    return result;
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("selected automation run reader", () => {
  it("loads activity only on expansion and retains the conversation when revisiting a cached run", async () => {
    const { client } = view();
    await screen.findByText("new final result");
    expect(invoke.mock.calls.some(([op]) => op === "get_automation_run_activity")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Ran command" }));
    await screen.findByText(/Recorded tool output/);
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() =>
      expect(invoke.mock.calls.some(([op, args]) => op === "get_automation_run" && args.after === 2)).toBe(
        true,
      ),
    );
    await waitFor(() => expect(client.isFetching()).toBe(0));
    expect(screen.getByRole("button", { name: "Ran command" }).getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: /Older recorded run/ }));
    await screen.findByText("old final result");
    fireEvent.click(screen.getByRole("button", { name: /Latest recorded run/ }));
    await screen.findByText("new final result");
    expect(screen.getByRole("button", { name: /Latest recorded run/ }).getAttribute("aria-pressed")).toBe(
      "true",
    );
    client.clear();
  });

  it("keeps earlier messages and the refresh cursor after paging backwards", async () => {
    const { client } = view();
    await screen.findByText("new final result");
    fireEvent.click(screen.getByRole("button", { name: "Load earlier messages" }));
    await screen.findByText("Original recorded prompt");
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(client.isFetching()).toBe(0));
    expect(screen.getByText("Original recorded prompt")).toBeTruthy();
    expect(screen.getByText("new final result")).toBeTruthy();
    expect(invoke.mock.calls.some(([op, args]) => op === "get_automation_run" && args.after === 2)).toBe(
      true,
    );
    expect(invoke.mock.calls.every(([op]) => !op.startsWith("preview_") && op !== "apply_write_plan")).toBe(
      true,
    );
    client.clear();
  });

  it("polls while visible and stops history requests when Runs is hidden", async () => {
    const { client, rerender } = view();
    await screen.findByText("new final result");
    await waitFor(() => expect(client.isFetching()).toBe(0));
    vi.useFakeTimers();
    rerender(
      <QueryClientProvider client={client}>
        <AutomationRuns automation={automation} active={false} />
      </QueryClientProvider>,
    );
    rerender(
      <QueryClientProvider client={client}>
        <AutomationRuns automation={automation} active />
      </QueryClientProvider>,
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(15_001);
    });
    const activeCalls = invoke.mock.calls.length;
    expect(invoke.mock.calls.some(([op, args]) => op === "get_automation_run" && args.after === 2)).toBe(
      true,
    );
    rerender(
      <QueryClientProvider client={client}>
        <AutomationRuns automation={automation} active={false} />
      </QueryClientProvider>,
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_001);
    });
    expect(invoke.mock.calls.length).toBe(activeCalls);
    expect(screen.getByText("new final result")).toBeTruthy();
    client.clear();
  });
});
