import { describe, expect, it } from "vitest";
import { maskActivityDetails, mergeConversation, mergeRuns, reviewStatus } from "./automationRuns";
import type { AutomationRun, RunConversationItem } from "./types";

function run(threadId: string, createdAt: number): AutomationRun {
  return {
    threadId,
    createdAt,
    updatedAt: createdAt,
    status: "PENDING_REVIEW",
    readAt: null,
    title: threadId,
    summary: null,
    sourceCwd: null,
    archivedReason: null,
  };
}

function item(id: string, position: number, text: string): RunConversationItem {
  return {
    id,
    position,
    version: position,
    kind: "assistant",
    label: "Codex",
    text,
    phase: "commentary",
    createdAt: null,
    nextCursor: null,
  };
}

describe("automation history refresh", () => {
  it("keeps loaded older runs while updating statuses and ordering new runs first", () => {
    const old = run("older", 1);
    const current = run("current", 2);
    const incoming = { ...current, status: "ARCHIVED" };
    expect(mergeRuns([current, old], [incoming, run("new", 3)])).toEqual([run("new", 3), incoming, old]);
    expect(current.status).toBe("PENDING_REVIEW");
  });

  it("retains earlier messages and stable ids when a tool finishes or new messages arrive", () => {
    const earlier = item("prompt", 0, "Recorded prompt");
    const current = item("reply", 2, "Partial reply");
    const updated = { ...current, version: 5, text: "Finished reply" };
    expect(mergeConversation([earlier, current], [item("later", 4, "Next"), updated])).toEqual([
      earlier,
      updated,
      item("later", 4, "Next"),
    ]);
    expect(current.text).toBe("Partial reply");
  });

  it("replaces protocol duplicates after their completed conversation item arrives", () => {
    const previous = [item("prompt", 0, "Prompt"), item("raw", 1, "Reply")];
    expect(mergeConversation(previous, [item("canonical", 2, "Reply")], ["raw"])).toEqual([
      previous[0],
      item("canonical", 2, "Reply"),
    ]);
  });

  it("labels review state independently of successful execution and retains unknown statuses", () => {
    expect(reviewStatus("PENDING_REVIEW")).toBe("Pending review");
    expect(reviewStatus("ARCHIVED")).toBe("Archived");
    expect(reviewStatus("NEW_DESKTOP_STATUS")).toBe("NEW_DESKTOP_STATUS");
  });

  it("masks nested secret-looking fields without changing the recorded data", () => {
    const details = {
      env: { API_KEY: "private", CODEX_HOME: "/custom", hotkey: "K" },
      nested: [{ password: "private" }],
    };
    const masked = maskActivityDetails(details);
    expect(JSON.stringify(masked)).not.toContain("private");
    expect(masked).toMatchObject({ env: { CODEX_HOME: "/custom", hotkey: "K" } });
    expect(details.env.API_KEY).toBe("private");
  });
});
