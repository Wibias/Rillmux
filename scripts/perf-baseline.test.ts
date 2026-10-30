import { describe, expect, it } from "vitest";
// @ts-expect-error The runtime script is plain ESM and intentionally has no declaration file.
import { median, summarizeRuns } from "./perf-baseline.mjs";

function run(windowMs: number, privateMb: number, passed: boolean) {
  return {
    window_ms: windowMs,
    working_set_mb: privateMb + 30,
    private_mb: privateMb,
    cpu_pct_one_core: windowMs / 1000,
    pass: passed,
  };
}

describe("performance baseline", () => {
  it("uses the middle value for odd and averaged middle values for even samples", () => {
    expect(median([9, 2, 4])).toBe(4);
    expect(median([4, 1, 3, 2])).toBe(2.5);
  });

  it("summarizes repeated runs without hiding a budget failure", () => {
    const summary = summarizeRuns([
      run(1200, 210, true),
      run(1800, 260, true),
      run(2800, 300, false),
    ]);
    expect(summary).toMatchObject({
      runs: 3,
      budget_pass: false,
      window_ms: 1800,
      private_mb: 260,
    });
  });

  it("rejects incomplete measurements", () => {
    expect(() => summarizeRuns([])).toThrow(/at least one/);
    expect(() => summarizeRuns([{ window_ms: 1 }])).toThrow(/working_set_mb/);
  });
});
