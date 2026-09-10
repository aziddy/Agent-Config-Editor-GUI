import { useMemo } from "react";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import {
  buildRrule,
  defaultSchedule,
  type Freq,
  parseRrule,
  summarizeRrule,
  WEEKDAYS,
  type Weekday,
} from "@/lib/rrule";
import { cn } from "@/lib/utils";

const DAY_LABEL: Record<Weekday, string> = {
  MO: "Mon",
  TU: "Tue",
  WE: "Wed",
  TH: "Thu",
  FR: "Fri",
  SA: "Sat",
  SU: "Sun",
};

export function ScheduleBuilder({
  value,
  onChange,
  disabled,
}: {
  value: string;
  onChange: (v: string) => void;
  disabled?: boolean;
}) {
  const draft = useMemo(() => parseRrule(value), [value]);
  const summary = useMemo(() => summarizeRrule(value), [value]);
  const d = draft ?? defaultSchedule();
  const update = (patch: Partial<typeof d>) => onChange(buildRrule({ ...d, ...patch }));

  return (
    <div className="space-y-2">
      {draft ? (
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span>Every</span>
          <Input
            type="number"
            min={1}
            value={d.interval}
            disabled={disabled}
            onChange={(e) => update({ interval: Math.max(1, Number(e.target.value) || 1) })}
            className="h-8 w-16 text-sm"
          />
          <Select
            value={d.freq}
            disabled={disabled}
            onChange={(e) => update({ freq: e.target.value as Freq })}
            className="h-8 w-28 text-sm"
          >
            <option value="HOURLY">hour(s)</option>
            <option value="DAILY">day(s)</option>
            <option value="WEEKLY">week(s)</option>
            <option value="MONTHLY">month(s)</option>
          </Select>
          {d.freq !== "HOURLY" && (
            <>
              <span>at</span>
              <Input
                type="number"
                min={0}
                max={23}
                value={d.hour}
                disabled={disabled}
                onChange={(e) => update({ hour: Math.min(23, Math.max(0, Number(e.target.value) || 0)) })}
                className="h-8 w-16 text-sm"
              />
              <span>:</span>
            </>
          )}
          {d.freq === "HOURLY" && <span>at minute</span>}
          <Input
            type="number"
            min={0}
            max={59}
            value={d.minute}
            disabled={disabled}
            onChange={(e) => update({ minute: Math.min(59, Math.max(0, Number(e.target.value) || 0)) })}
            className="h-8 w-16 text-sm"
          />
          {d.freq === "WEEKLY" && (
            <div className="flex gap-1">
              {WEEKDAYS.map((w) => {
                const on = d.byday.includes(w);
                return (
                  <button
                    key={w}
                    type="button"
                    disabled={disabled}
                    onClick={() => update({ byday: on ? d.byday.filter((x) => x !== w) : [...d.byday, w] })}
                    className={cn(
                      "rounded border px-1.5 py-0.5 text-xs",
                      on
                        ? "border-primary bg-primary text-primary-foreground"
                        : "text-muted-foreground hover:bg-accent",
                    )}
                  >
                    {DAY_LABEL[w]}
                  </button>
                );
              })}
            </div>
          )}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">
          Custom rule (not expressible in the builder). Edit the RRULE text directly.
        </p>
      )}
      <Input
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        className="h-8 font-mono text-xs"
        placeholder="RRULE:FREQ=DAILY;BYHOUR=9;BYMINUTE=0"
      />
      {summary.error ? (
        <p className="text-xs text-destructive">Invalid rule: {summary.error}</p>
      ) : (
        <p className="text-xs text-muted-foreground">
          {summary.text}
          {summary.next.length > 0 && <> · next: {summary.next.map((n) => n.toLocaleString()).join(", ")}</>}
        </p>
      )}
    </div>
  );
}
