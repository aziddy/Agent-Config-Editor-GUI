import { describe, expect, it } from "vitest";
import { buildRrule, parseRrule, summarizeRrule } from "./rrule";

describe("rrule helpers", () => {
  it("round-trips the Codex weekly form", () => {
    const src = "RRULE:FREQ=WEEKLY;BYHOUR=1;BYMINUTE=15;BYDAY=SU,MO,TU,WE,TH,FR,SA";
    const d = parseRrule(src);
    expect(d).toEqual({
      freq: "WEEKLY",
      byday: ["SU", "MO", "TU", "WE", "TH", "FR", "SA"],
      hour: 1,
      minute: 15,
      interval: 1,
    });
    expect(buildRrule(d!)).toBe(src);
  });
  it("rejects rules the builder cannot express", () => {
    expect(parseRrule("RRULE:FREQ=WEEKLY;BYHOUR=9;BYMINUTE=0;BYMONTHDAY=1")).toBeNull();
    expect(parseRrule("")).toBeNull();
  });
  it("summarises with next occurrences", () => {
    const s = summarizeRrule("RRULE:FREQ=WEEKLY;BYHOUR=9;BYMINUTE=0;BYDAY=MO");
    expect(s.error).toBeNull();
    expect(s.text.toLowerCase()).toContain("monday");
    expect(s.next).toHaveLength(3);
    expect(summarizeRrule("nonsense").error).not.toBeNull();
  });
});
