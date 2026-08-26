import { describe, expect, it } from "vitest";
import { migrateSettings } from "./store";
import { SETTINGS_SCHEMA_VERSION } from "./types";

function modeOf(raw: unknown): unknown {
  return (migrateSettings(raw).streaming as { streamOpenMode?: unknown })
    .streamOpenMode;
}

describe("stream opening mode migration", () => {
  it("preserves legacy seamless switching as seamless mode", () => {
    expect(
      modeOf({ schemaVersion: 20, streaming: { seamlessSwitch: true } }),
    ).toBe("seamless");
  });

  it("preserves legacy seamless-off behavior as multistream mode", () => {
    expect(
      modeOf({ schemaVersion: 20, streaming: { seamlessSwitch: false } }),
    ).toBe("multistream");
  });

  it("preserves an explicit independent mode on the current schema", () => {
    expect(
      modeOf({
        schemaVersion: SETTINGS_SCHEMA_VERSION,
        streaming: { streamOpenMode: "independent" },
      }),
    ).toBe("independent");
  });
});
