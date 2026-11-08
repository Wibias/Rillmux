import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, it } from "node:test";
import { runInNewContext } from "node:vm";
import { SNAPSHOT_JS } from "./control-rillmux.mjs";
import { snapshotReady } from "./web-snapshot.mjs";

function browserSnapshot({
  brand = "Rillmux",
  legacyBrand = null,
  shell = true,
  splash = false,
  alerts = [],
} = {}) {
  const nodes = new Map();
  if (shell) nodes.set(".shell", {});
  if (brand !== null) {
    nodes.set(".shell__brand-title", { innerText: brand });
  }
  if (legacyBrand !== null) {
    nodes.set(".shell__titlebar-title", { innerText: legacyBrand });
  }
  const document = {
    title: "Rillmux",
    body: { innerText: "Rillmux is visible in page text" },
    querySelector: (selector) => nodes.get(selector) ?? null,
    querySelectorAll: (selector) => selector === '[role="alert"]'
      ? alerts.map((msg) => ({ innerText: msg }))
      : [],
    getElementById: (id) => id === "splash" && splash ? {} : null,
  };
  return runInNewContext(SNAPSHOT_JS, {
    document,
    location: {
      href: "http://127.0.0.1:1420/settings/interface",
      pathname: "/settings/interface",
    },
  }, { timeout: 1000 });
}

describe("actual Rillmux shell snapshot and lane readiness", () => {
  it("accepts a hydrated current brand and both correct lane contexts", () => {
    const snap = browserSnapshot({ brand: "  Rillmux  " });
    assert.equal(snap.appName, "Rillmux");
    assert.equal(snap.hydrated, true);
    assert.equal(snapshotReady(snap), true);
    assert.equal(snapshotReady(snap, { native: true }), true);
  });

  it("rejects the removed titlebar selector even if the page title and text say Rillmux", () => {
    const snap = browserSnapshot({
      brand: null,
      legacyBrand: "Rillmux",
    });
    assert.equal(snap.appName, "");
    assert.equal(snapshotReady(snap), false);
    assert.equal(snapshotReady(snap, { native: true }), false);
  });

  it("rejects a brand with missing shell hydration and an incorrect brand", () => {
    assert.equal(snapshotReady(browserSnapshot({ shell: false })), false);
    assert.equal(snapshotReady(browserSnapshot({ brand: "Another application" })), false);
    assert.equal(snapshotReady(browserSnapshot({ brand: "" })), false);
  });

  it("rejects Vite's desktop-only guard in native mode but allows it in web mode", () => {
    const snap = browserSnapshot({
      alerts: ["Desktop shell required (npm run tauri:dev)."],
    });
    assert.equal(snapshotReady(snap), true);
    assert.equal(snapshotReady(snap, { native: true }), false);
  });

  it("does not allow a missing or malformed readiness snapshot to pass", () => {
    assert.equal(snapshotReady(null), false);
    assert.equal(snapshotReady({ hydrated: true, appName: null }), false);
    assert.equal(snapshotReady({ hydrated: false, appName: "Rillmux" }), false);
  });


  it("still executes the real CLI entrypoint on Windows/Linux after making it import-safe", () => {
    const file = fileURLToPath(new URL("./control-rillmux.mjs", import.meta.url));
    const run = spawnSync(process.execPath, [file, "help"], {
      encoding: "utf8", timeout: 10_000,
    });
    assert.equal(run.status, 0, run.stderr);
    assert.match(run.stdout, /Rillmux verification control surface/);
  });
});
