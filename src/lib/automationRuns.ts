import { isSecretKey, maskValue } from "./mask";
import type { AutomationRun, JsonValue, RunConversationItem } from "./types";

export function mergeRuns(previous: AutomationRun[], incoming: AutomationRun[]): AutomationRun[] {
  const runs = new Map(previous.map((run) => [run.threadId, run]));
  for (const run of incoming) runs.set(run.threadId, run);
  return [...runs.values()].sort((a, b) => b.createdAt - a.createdAt || b.threadId.localeCompare(a.threadId));
}

export function mergeConversation(
  previous: RunConversationItem[],
  incoming: RunConversationItem[],
  removed: string[] = [],
): RunConversationItem[] {
  const excluded = new Set(removed);
  const items = new Map(previous.filter((item) => !excluded.has(item.id)).map((item) => [item.id, item]));
  for (const item of incoming) items.set(item.id, item);
  return [...items.values()].sort((a, b) => a.position - b.position || a.id.localeCompare(b.id));
}

export function reviewStatus(status: string): string {
  return (
    { PENDING_REVIEW: "Pending review", ACCEPTED: "Accepted", ARCHIVED: "Archived", RUNNING: "Running" }[
      status
    ] ?? status
  );
}

export function maskActivityDetails(value: JsonValue): JsonValue {
  if (Array.isArray(value)) return value.map(maskActivityDetails);
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, child]) => [
        key,
        isSecretKey(key) && child !== null ? maskValue(String(child)) : maskActivityDetails(child),
      ]),
    );
  }
  return value;
}
