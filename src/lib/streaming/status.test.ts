import { describe, expect, it } from "vitest";
import { watchingPhase, watchingStatusText, nextSessionStatus, sessionStatusPatch } from "./status";

describe("watchingPhase", () => {
  it("keeps known phases", () => {
    expect(watchingPhase("ready")).toBe("ready");
    expect(watchingPhase("ads")).toBe("ads");
    expect(watchingPhase("ended")).toBe("ended");
  });

  it("falls back to info", () => {
    expect(watchingPhase()).toBe("info");
    expect(watchingPhase("unknown")).toBe("info");
  });
});

describe("watchingStatusText", () => {
  const t = (key: string) => key;

  it("maps ready/ads/error/starting to locale keys", () => {
    expect(watchingStatusText("ready", "raw", t)).toBe(
      "routes:watchingStatusReady",
    );
    expect(watchingStatusText("ads", "raw", t)).toBe("routes:watchingStatusAds");
    expect(watchingStatusText("error", "raw", t)).toBe(
      "routes:watchingStatusError",
    );
    expect(watchingStatusText("starting", "raw", t)).toBe(
      "routes:watchingStatusStarting",
    );
    expect(watchingStatusText("info", "Low latency streaming (HLS live edge: 2)", t)).toBe(
      "routes:watchingStatusStarting",
    );
  });

  it("uses the fallback for ended", () => {
    expect(watchingStatusText("ended", "Stopped", t)).toBe("Stopped");
  });
});

describe("nextSessionStatus", () => {
  it("keeps Playing when a later info line arrives", () => {
    expect(
      nextSessionStatus(
        { phase: "ready", ready: true, status: "Playing" },
        {
          phase: "info",
          ready: false,
          status: "Low latency streaming (HLS live edge: 2)",
        },
      ),
    ).toEqual({ phase: "ready", ready: true, status: "Playing" });
  });

  it("keeps Playing when a non-fatal HLS error line arrives", () => {
    expect(
      nextSessionStatus(
        { phase: "ready", ready: true, status: "Playing" },
        { phase: "error", ready: false, status: "Failed to reload playlist" },
      ),
    ).toEqual({ phase: "ready", ready: true, status: "Playing" });
  });

  it("allows ended after ready", () => {
    expect(
      nextSessionStatus(
        { phase: "ready", ready: true, status: "Playing" },
        { phase: "ended", ready: false, status: "Stopped" },
      ),
    ).toEqual({ phase: "ended", ready: false, status: "Stopped" });
  });
});

describe("sessionStatusPatch", () => {
  it("skips identical Playing updates so the UI is not redrawn", () => {
    const patch = sessionStatusPatch(
      { phase: "ready", ready: true, status: "Playing" },
      {
        phase: "info",
        ready: false,
        status: "Low latency streaming (HLS live edge: 2)",
      },
    );
    expect(patch.changed).toBe(false);
    expect(patch.becameReady).toBe(false);
  });

  it("marks the first ready transition", () => {
    const patch = sessionStatusPatch(
      { phase: "starting", ready: false, status: "Opening stream" },
      { phase: "ready", ready: true, status: "Playing" },
    );
    expect(patch.changed).toBe(true);
    expect(patch.becameReady).toBe(true);
  });
});
