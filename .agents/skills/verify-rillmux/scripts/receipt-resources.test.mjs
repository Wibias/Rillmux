import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { receiptStartedResources } from "./receipt-resources.mjs";

const mixedState = {
  runId: "verify-rillmux-web-stale",
  port: 18821,
  origin: "http://127.0.0.1:18821",
  previewPid: null,
  cdpPort: 18877,
  browserPid: null,
  webStartedResources: [
    {
      kind: "http",
      id: "vite-web",
      port: 18821,
      origin: "http://127.0.0.1:18821",
      pid: 111,
    },
  ],
  native: {
    runId: "verify-rillmux-native-current",
    port: 1420,
    origin: "http://localhost:1420",
    pid: null,
    cdpPort: 18889,
    appDataDir: "C:\\tmp\\native-appdata",
    startedResources: [
      {
        kind: "tauri-dev",
        id: "native",
        port: 1420,
        origin: "http://localhost:1420",
        pid: 222,
        cdpPort: 18889,
        appDataDir: "C:\\tmp\\native-appdata",
      },
    ],
  },
};

describe("receipt started_resources provenance", () => {
  it("does not put a stale web Vite resource on a native receipt", () => {
    const resources = receiptStartedResources(mixedState, {
      surface: "desktop",
      runId: "verify-rillmux-native-current",
    });
    assert.equal(resources.length, 1);
    assert.equal(resources[0].id, "native");
    assert.equal(resources[0].kind, "tauri-dev");
    assert.ok(!resources.some((r) => r.id === "vite-web" || r.kind === "http"));
  });

  it("does not put a native tauri-dev resource on a web receipt", () => {
    const resources = receiptStartedResources(mixedState, {
      surface: "web",
      runId: "verify-rillmux-web-stale",
    });
    assert.equal(resources.length, 1);
    assert.equal(resources[0].id, "vite-web");
    assert.ok(!resources.some((r) => r.id === "native" || r.kind === "tauri-dev"));
  });

  it("returns nothing when the receipt runId does not match the lane snapshot", () => {
    assert.deepEqual(
      receiptStartedResources(mixedState, {
        surface: "desktop",
        runId: "verify-rillmux-native-other",
      }),
      [],
    );
  });
});
