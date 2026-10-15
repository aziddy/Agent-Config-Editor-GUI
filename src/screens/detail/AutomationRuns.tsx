import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowDown, ChevronDown, ChevronRight, Clock, Loader2, LockKeyhole, RefreshCw } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { RunMarkdown } from "@/components/RunMarkdown";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { maskActivityDetails, mergeConversation, mergeRuns, reviewStatus } from "@/lib/automationRuns";
import { describeError } from "@/lib/errors";
import { rpc } from "@/lib/rpc";
import type { Automation, AutomationRun, AutomationRunConversation, RunConversationItem } from "@/lib/types";
import { cn } from "@/lib/utils";

const REFRESH_MS = 15_000;
const date = (ms: number) => new Date(ms).toLocaleString();

function RunStatus({ status }: { status: string }) {
  return (
    <Badge variant={status === "ACCEPTED" ? "success" : status === "PENDING_REVIEW" ? "warning" : "muted"}>
      {reviewStatus(status)}
    </Badge>
  );
}

export function AutomationRuns({ automation, active }: { automation: Automation; active: boolean }) {
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<AutomationRun | null>(null);
  const [wide, setWide] = useState(false);
  const container = useRef<HTMLDivElement>(null);
  const history = useInfiniteQuery({
    queryKey: ["automationRuns", automation.id],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => rpc("list_automation_runs", { id: automation.id, cursor: pageParam }),
    getNextPageParam: (page) => page.nextCursor ?? undefined,
    enabled: active,
    refetchInterval: active ? REFRESH_MS : false,
    staleTime: REFRESH_MS,
  });
  const runs = mergeRuns([], history.data?.pages.flatMap((page) => page.runs) ?? []);
  const newest = runs[0];
  useEffect(() => {
    if (newest) setSelected((previous) => previous ?? newest);
  }, [newest]);
  useEffect(() => {
    const element = container.current;
    if (!element) return;
    const measure = () => {
      if (element.clientWidth > 0) setWide(element.clientWidth >= 720);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const first = history.data?.pages[0];
  const selectedRun = runs.find((run) => run.threadId === selected?.threadId) ?? selected;

  return (
    <div ref={container} className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center justify-between gap-2 border-b px-4 py-2">
        <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <LockKeyhole className="h-3 w-3" /> Read-only history
        </span>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1.5 text-xs"
          disabled={history.isFetching}
          onClick={() => {
            void history.refetch();
            void queryClient.invalidateQueries({ queryKey: ["automationRun", automation.id] });
          }}
        >
          <RefreshCw className={cn("h-3 w-3", history.isFetching && "animate-spin")} /> Refresh
        </Button>
      </div>
      {first?.warnings.map((warning) => (
        <p key={warning} role="alert" className="border-b px-4 py-2 text-xs text-destructive">
          {warning}
        </p>
      ))}
      {history.error && (
        <p role="alert" className="border-b px-4 py-2 text-xs text-destructive">
          {describeError(history.error)}
        </p>
      )}
      <div className={cn("flex min-h-0 flex-1", !wide && "flex-col")}>
        <section
          className={cn("shrink-0 overflow-y-auto", wide ? "w-[240px] border-r" : "max-h-[200px] border-b")}
          aria-label="Automation runs"
        >
          {history.isLoading && <p className="p-4 text-xs text-muted-foreground">Loading runs…</p>}
          {first && runs.length === 0 && (
            <p className="p-4 text-sm text-muted-foreground">
              {first.storageAvailable
                ? "No runs recorded for this automation yet."
                : "Local Codex run history is unavailable."}
            </p>
          )}
          {runs.map((run) => (
            <button
              key={run.threadId}
              type="button"
              aria-pressed={selectedRun?.threadId === run.threadId}
              className={cn(
                "block w-full space-y-1.5 border-b px-3 py-3 text-left hover:bg-accent/60",
                selectedRun?.threadId === run.threadId && "bg-accent",
              )}
              onClick={() => setSelected(run)}
            >
              <div className="flex items-start gap-2">
                <span className="line-clamp-2 min-w-0 flex-1 text-xs font-medium">{run.title}</span>
                {run.readAt === null && (
                  <span
                    className="mt-1 h-1.5 w-1.5 shrink-0 rounded-full bg-blue-500"
                    title="Unread in Codex"
                  />
                )}
              </div>
              <p className="text-[11px] text-muted-foreground">{date(run.createdAt)}</p>
              <RunStatus status={run.status} />
              {run.summary && <p className="line-clamp-2 text-xs text-muted-foreground">{run.summary}</p>}
              {run.sourceCwd && (
                <p className="truncate font-mono text-[10px] text-muted-foreground" title={run.sourceCwd}>
                  {run.sourceCwd}
                </p>
              )}
            </button>
          ))}
          {history.hasNextPage && (
            <Button
              variant="ghost"
              size="sm"
              className="m-2 text-xs"
              disabled={history.isFetching}
              onClick={() => void history.fetchNextPage()}
            >
              {history.isFetchingNextPage ? "Loading…" : "Load older"}
            </Button>
          )}
        </section>
        {selectedRun ? (
          <RunReader
            key={selectedRun.threadId}
            automationId={automation.id}
            run={selectedRun}
            active={active}
          />
        ) : (
          <div className="flex min-h-0 flex-1 items-center justify-center p-6 text-sm text-muted-foreground">
            Select a run to read its conversation and output.
          </div>
        )}
      </div>
    </div>
  );
}

function RunReader({
  automationId,
  run,
  active,
}: {
  automationId: string;
  run: AutomationRun;
  active: boolean;
}) {
  const queryClient = useQueryClient();
  const queryKey = ["automationRun", automationId, run.threadId];
  const [conversation, setConversation] = useState<AutomationRunConversation | null>(null);
  const latest = useRef<AutomationRunConversation | null>(null);
  const scroller = useRef<HTMLElement>(null);
  const initialized = useRef(false);
  const prependAnchor = useRef<{ height: number; top: number } | null>(null);
  const reader = useQuery({
    queryKey,
    queryFn: async ({ signal }) => {
      const previous = latest.current ?? queryClient.getQueryData<AutomationRunConversation>(queryKey);
      let page = await rpc("get_automation_run", {
        id: automationId,
        threadId: run.threadId,
        after: previous?.afterCursor,
      });
      if (previous && page.historySource !== previous.historySource) {
        page = {
          ...(await rpc("get_automation_run", { id: automationId, threadId: run.threadId })),
          reset: true,
        };
      }
      while (page.hasMoreAfter && !signal.aborted) {
        const more = await rpc("get_automation_run", {
          id: automationId,
          threadId: run.threadId,
          after: page.afterCursor,
        });
        page = {
          ...more,
          items: mergeConversation(page.items, more.items, more.removedItemIds),
          removedItemIds: [...page.removedItemIds, ...more.removedItemIds],
        };
      }
      return previous && !page.reset
        ? {
            ...page,
            items: mergeConversation(previous.items, page.items, page.removedItemIds),
            beforeCursor: previous.beforeCursor,
            afterCursor: Math.max(previous.afterCursor, page.afterCursor),
          }
        : page;
    },
    enabled: active,
    staleTime: REFRESH_MS,
    refetchInterval: active ? REFRESH_MS : false,
  });
  useEffect(() => {
    if (!reader.data) return;
    const incoming = reader.data;
    setConversation((previous) => {
      const next =
        previous && !incoming.reset
          ? {
              ...incoming,
              items: mergeConversation(previous.items, incoming.items, incoming.removedItemIds),
              beforeCursor: previous.beforeCursor,
              afterCursor: Math.max(previous.afterCursor, incoming.afterCursor),
            }
          : incoming;
      latest.current = next;
      return next;
    });
  }, [reader.data]);
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element || !conversation || !active) return;
    if (prependAnchor.current) {
      element.scrollTop = prependAnchor.current.top + element.scrollHeight - prependAnchor.current.height;
      prependAnchor.current = null;
    } else if (!initialized.current) {
      element.scrollTop = element.scrollHeight;
      initialized.current = true;
    }
  }, [conversation, active]);
  const earlier = useMutation({
    mutationFn: async () => {
      const previous = latest.current;
      const page = await rpc("get_automation_run", {
        id: automationId,
        threadId: run.threadId,
        before: previous?.beforeCursor,
      });
      return previous && page.historySource !== previous.historySource
        ? { ...(await rpc("get_automation_run", { id: automationId, threadId: run.threadId })), reset: true }
        : page;
    },
    onSuccess: (page) => {
      const element = scroller.current;
      if (element) prependAnchor.current = { height: element.scrollHeight, top: element.scrollTop };
      const previous = latest.current;
      const next =
        previous && !page.reset
          ? {
              ...previous,
              items: mergeConversation(previous.items, page.items),
              beforeCursor: page.beforeCursor,
            }
          : page;
      latest.current = next;
      setConversation(next);
      queryClient.setQueryData(queryKey, next);
    },
  });
  const visibleRun = conversation?.run ?? run;
  const execution = conversation?.executionStatus;
  const executionLabel = execution
    ? ({ completed: "Completed", inProgress: "In progress", interrupted: "Interrupted", failed: "Failed" }[
        execution
      ] ?? execution)
    : null;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="shrink-0 space-y-1.5 border-b px-4 py-3">
        <h3 className="text-sm font-semibold">{visibleRun.title}</h3>
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <span className="flex items-center gap-1">
            <Clock className="h-3 w-3" /> {date(visibleRun.createdAt)}
          </span>
          <RunStatus status={visibleRun.status} />
          {reader.isFetching && (
            <Loader2 className="h-3 w-3 animate-spin" aria-label="Refreshing conversation" />
          )}
        </div>
        {executionLabel && (
          <p className="text-xs text-muted-foreground">
            Last turn: {executionLabel}
            {conversation?.durationMs != null ? ` · ${Math.round(conversation.durationMs / 1000)}s` : ""}
          </p>
        )}
        {visibleRun.sourceCwd && (
          <p className="break-all font-mono text-[11px] text-muted-foreground">{visibleRun.sourceCwd}</p>
        )}
      </div>
      <section
        ref={scroller}
        className="min-h-0 flex-1 space-y-4 overflow-y-auto p-4"
        aria-label="Run conversation"
      >
        {reader.error && (
          <p role="alert" className="text-sm text-destructive">
            {describeError(reader.error)}
          </p>
        )}
        {earlier.error && (
          <p role="alert" className="text-sm text-destructive">
            {describeError(earlier.error)}
          </p>
        )}
        {conversation?.notices.map((notice) => (
          <p key={notice} className="rounded-md border p-3 text-xs text-muted-foreground">
            {notice}
          </p>
        ))}
        {!conversation && reader.isLoading && (
          <p className="text-sm text-muted-foreground">Loading conversation…</p>
        )}
        {conversation?.beforeCursor != null && (
          <Button
            variant="outline"
            size="sm"
            className="text-xs"
            disabled={earlier.isPending || reader.isFetching}
            onClick={() => earlier.mutate()}
          >
            {earlier.isPending ? "Loading…" : "Load earlier messages"}
          </Button>
        )}
        {conversation?.items.map((item) => (
          <RunItem
            key={item.id}
            automationId={automationId}
            threadId={run.threadId}
            item={item}
            active={active}
          />
        ))}
        {conversation?.availability === "unavailable" && visibleRun.summary && (
          <div className="rounded-md border p-3">
            <RunMarkdown text={visibleRun.summary} />
          </div>
        )}
      </section>
      {conversation && conversation.items.length > 0 && (
        <div className="flex shrink-0 justify-end border-t px-3 py-1">
          <Button
            variant="ghost"
            size="sm"
            className="h-7 gap-1 text-xs"
            onClick={() =>
              scroller.current?.scrollTo({ top: scroller.current.scrollHeight, behavior: "smooth" })
            }
          >
            <ArrowDown className="h-3 w-3" /> Jump to latest
          </Button>
        </div>
      )}
    </div>
  );
}

function RunItem({
  automationId,
  threadId,
  item,
  active,
}: {
  automationId: string;
  threadId: string;
  item: RunConversationItem;
  active: boolean;
}) {
  const activity = item.kind === "activity";
  const [open, setOpen] = useState(false);
  const [showPrompt, setShowPrompt] = useState(false);
  const body = useInfiniteQuery({
    queryKey: [
      "automationRunActivity",
      automationId,
      threadId,
      item.id,
      item.version,
      activity ? "activity" : "message",
    ],
    initialPageParam: (activity ? null : item.nextCursor) as string | null,
    queryFn: ({ pageParam }) =>
      rpc("get_automation_run_activity", { id: automationId, threadId, itemId: item.id, cursor: pageParam }),
    getNextPageParam: (page) => page.nextCursor ?? undefined,
    enabled: active && activity && open,
    staleTime: Number.POSITIVE_INFINITY,
  });
  const text = (item.text ?? "") + (body.data?.pages.map((page) => page.text).join("") ?? "");
  const truncatedPrompt = item.kind === "user" && text.length > 1200 && !showPrompt;
  const more = body.data ? body.hasNextPage : item.nextCursor !== null;

  if (activity) {
    const details = body.data?.pages[0]?.details;
    return (
      <div className="rounded-md border">
        <button
          type="button"
          aria-expanded={open}
          className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-muted-foreground hover:bg-accent/50"
          onClick={() => setOpen((value) => !value)}
        >
          {open ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
          {item.label}
        </button>
        {open && (
          <div className="space-y-2 border-t p-3">
            {body.isLoading && <p className="text-xs text-muted-foreground">Loading activity…</p>}
            {body.error && (
              <p role="alert" className="text-xs text-destructive">
                {describeError(body.error)}
              </p>
            )}
            {details && Object.keys(details).length > 0 && (
              <pre className="whitespace-pre-wrap break-all text-[11px] text-muted-foreground">
                {JSON.stringify(maskActivityDetails(details), null, 2)}
              </pre>
            )}
            {text && (
              <pre className="max-h-96 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/40 p-2 font-mono text-xs">
                {text}
              </pre>
            )}
            {body.hasNextPage && (
              <Button
                variant="outline"
                size="sm"
                className="text-xs"
                disabled={body.isFetching}
                onClick={() => void body.fetchNextPage()}
              >
                Load more output
              </Button>
            )}
          </div>
        )}
      </div>
    );
  }
  return (
    <div
      className={cn(
        "space-y-2",
        item.kind === "user" && "rounded-lg bg-muted/60 p-3",
        item.phase === "final_answer" && "rounded-lg border p-3",
      )}
    >
      <p className="text-[11px] font-medium text-muted-foreground">
        {item.label}
        {item.phase === "final_answer" && item.kind === "assistant" ? " · Result" : ""}
      </p>
      <RunMarkdown text={truncatedPrompt ? `${text.slice(0, 1200)}…` : text} />
      {item.kind === "user" && text.length > 1200 && (
        <Button
          variant="ghost"
          size="sm"
          className="h-7 text-xs"
          onClick={() => setShowPrompt((value) => !value)}
        >
          {showPrompt ? "Show less" : "Show full prompt"}
        </Button>
      )}
      {body.error && (
        <p role="alert" className="text-xs text-destructive">
          {describeError(body.error)}
        </p>
      )}
      {more && (
        <Button
          variant="outline"
          size="sm"
          className="text-xs"
          disabled={body.isFetching}
          onClick={() => void body.fetchNextPage()}
        >
          Load more text
        </Button>
      )}
    </div>
  );
}
