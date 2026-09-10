import { RRule, rrulestr } from "rrule";

export type Freq = "DAILY" | "WEEKLY" | "HOURLY" | "MONTHLY";
export const WEEKDAYS = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"] as const;
export type Weekday = (typeof WEEKDAYS)[number];

export interface ScheduleDraft {
  freq: Freq;
  byday: Weekday[];
  hour: number;
  minute: number;
  interval: number;
}

const DEFAULT: ScheduleDraft = { freq: "DAILY", byday: [], hour: 9, minute: 0, interval: 1 };

/** Parse `RRULE:FREQ=…` into builder state; returns null when the rule is not builder-shaped. */
export function parseRrule(text: string): ScheduleDraft | null {
  const body = text.trim().replace(/^RRULE:/i, "");
  if (!body) return null;
  const parts = new Map<string, string>();
  for (const kv of body.split(";")) {
    const [k, v] = kv.split("=");
    if (k && v !== undefined) parts.set(k.toUpperCase(), v);
  }
  const freq = parts.get("FREQ")?.toUpperCase();
  if (freq !== "DAILY" && freq !== "WEEKLY" && freq !== "HOURLY" && freq !== "MONTHLY") return null;
  const hour = Number(parts.get("BYHOUR") ?? DEFAULT.hour);
  const minute = Number(parts.get("BYMINUTE") ?? DEFAULT.minute);
  const interval = Number(parts.get("INTERVAL") ?? 1);
  const byday = (parts.get("BYDAY") ?? "")
    .split(",")
    .map((d) => d.trim().toUpperCase())
    .filter((d): d is Weekday => (WEEKDAYS as readonly string[]).includes(d));
  if (!Number.isInteger(hour) || !Number.isInteger(minute) || !Number.isInteger(interval)) return null;
  for (const k of parts.keys()) {
    if (!["FREQ", "BYHOUR", "BYMINUTE", "BYDAY", "INTERVAL"].includes(k)) return null;
  }
  return { freq, byday, hour, minute, interval };
}

export function buildRrule(d: ScheduleDraft): string {
  const parts = [`FREQ=${d.freq}`];
  if (d.interval > 1) parts.push(`INTERVAL=${d.interval}`);
  if (d.freq !== "HOURLY") {
    parts.push(`BYHOUR=${d.hour}`);
  }
  parts.push(`BYMINUTE=${d.minute}`);
  if (d.freq === "WEEKLY" && d.byday.length > 0) {
    // Keep Codex's Sunday-first ordering.
    const order: Weekday[] = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"];
    parts.push(`BYDAY=${order.filter((w) => d.byday.includes(w)).join(",")}`);
  }
  return `RRULE:${parts.join(";")}`;
}

export function defaultSchedule(): ScheduleDraft {
  return { ...DEFAULT, byday: [] };
}

export interface RruleSummary {
  text: string;
  next: Date[];
  error: string | null;
}

/** Human text plus the next few occurrences (interpreted in local time). */
export function summarizeRrule(text: string, count = 3): RruleSummary {
  try {
    const rule = rrulestr(text.trim(), { forceset: false }) as RRule;
    const now = new Date();
    const local = new RRule({ ...rule.origOptions, dtstart: now });
    const next = local.all((_, i) => i < count);
    return { text: local.toText(), next, error: null };
  } catch (err) {
    return { text: "", next: [], error: err instanceof Error ? err.message : String(err) };
  }
}
