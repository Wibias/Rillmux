import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  cdpListUrls,
  nativeCleanupPids,
  selectCdpPage,
} from "./native-cdp.mjs";

describe("native CDP page selection", () => {
  it("prefers the Rillmux localhost:1420 page over an earlier about:blank target", () => {
    const chosen = selectCdpPage([
      {
        type: "page",
        title: "",
        url: "about:blank",
        webSocketDebuggerUrl: "ws://127.0.0.1:9/devtools/page/blank",
      },
      {
        type: "page",
        title: "Rillmux",
        url: "http://localhost:1420/",
        webSocketDebuggerUrl: "ws://127.0.0.1:9/devtools/page/rillmux",
      },
    ]);
    assert.equal(chosen?.url, "http://localhost:1420/");
  });
});

describe("native CDP endpoints", () => {
  it("probes IPv4, localhost, and IPv6 loopback for /json/list", () => {
    const urls = cdpListUrls(18749);
    assert.ok(urls.some((u) => u.includes("127.0.0.1:18749")));
    assert.ok(urls.some((u) => u.includes("localhost:18749")));
    assert.ok(urls.some((u) => u.includes("[::1]:18749")));
  });
});

describe("native cleanup pid list", () => {
  it("is empty and does not throw when native state is missing", () => {
    assert.deepEqual(nativeCleanupPids(null), []);
    assert.deepEqual(nativeCleanupPids({}), []);
    assert.deepEqual(nativeCleanupPids({ native: undefined }), []);
  });
});
