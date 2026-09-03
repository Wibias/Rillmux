import { describe, expect, it } from "vitest";
import { createDesktopTraySession, type DesktopTrayApis } from "./trayBootstrap";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((next, fail) => {
    resolve = next;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function createHarness(options?: { shouldCreateTray?: boolean }) {
  const load = deferred<{
    closeLeftover: () => Promise<void>;
    createMenu: () => Promise<void>;
    createTray: () => Promise<void>;
    closeTray: () => Promise<void>;
    onCloseRequested: (
      handler: (event: { preventDefault: () => void }) => Promise<void>,
    ) => Promise<() => void>;
  }>();
  const leftover = deferred<void>();
  const menu = deferred<void>();
  const tray = deferred<void>();
  const closeRequested = deferred<() => void>();
  const stats = {
    leftover: 0,
    menu: 0,
    tray: 0,
    closeTray: 0,
    closeUnlisten: 0,
    hide: 0,
    closeRequested: 0,
  };
  let closeHandler:
    | ((event: { preventDefault: () => void }) => Promise<void>)
    | undefined;

  const session = createDesktopTraySession({
    shouldCreateTray: options?.shouldCreateTray ?? true,
    closeToTray: () => true,
    hideWindow: async () => {
      stats.hide += 1;
    },
    loadApis: () => load.promise,
  });

  const apis = {
    async closeLeftover() {
      stats.leftover += 1;
      await leftover.promise;
    },
    async createMenu() {
      stats.menu += 1;
      await menu.promise;
    },
    async createTray() {
      stats.tray += 1;
      await tray.promise;
    },
    async closeTray() {
      stats.closeTray += 1;
    },
    async onCloseRequested(
      handler: (event: { preventDefault: () => void }) => Promise<void>,
    ) {
      stats.closeRequested += 1;
      closeHandler = handler;
      return closeRequested.promise;
    },
  };

  function start() {
    return session.start();
  }

  return {
    stats,
    start,
    async beginLoad() {
      load.resolve(apis);
      await Promise.resolve();
      await Promise.resolve();
    },
    async finishLeftover() {
      leftover.resolve();
      await Promise.resolve();
      await Promise.resolve();
    },
    async finishMenu() {
      menu.resolve();
      await Promise.resolve();
      await Promise.resolve();
    },
    async finishTray() {
      tray.resolve();
      await Promise.resolve();
      await Promise.resolve();
    },
    async finishCloseRequested() {
      closeRequested.resolve(() => {
        stats.closeUnlisten += 1;
      });
      await Promise.resolve();
      await Promise.resolve();
    },
    async failCloseRequested(error: Error) {
      closeRequested.reject(error);
      await Promise.resolve();
      await Promise.resolve();
    },
    closeHandler: () => closeHandler,
  };
}

async function flush() {
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
}

describe("createDesktopTraySession", () => {
  it("creates the tray and close listener, then tears both down", async () => {
    const harness = createHarness();
    const dispose = harness.start();
    await harness.beginLoad();
    await harness.finishLeftover();
    await harness.finishMenu();
    await harness.finishTray();
    await harness.finishCloseRequested();
    expect(harness.stats.tray).toBe(1);
    expect(harness.stats.closeRequested).toBe(1);
    dispose();
    await flush();
    expect(harness.stats.closeUnlisten).toBe(1);
    expect(harness.stats.closeTray).toBe(1);
  });

  it("does not create a leftover or tray after dispose during API load", async () => {
    const harness = createHarness();
    const dispose = harness.start();
    dispose();
    await harness.beginLoad();
    await flush();
    expect(harness.stats.leftover).toBe(0);
    expect(harness.stats.tray).toBe(0);
    expect(harness.stats.closeRequested).toBe(0);
  });

  it("closes the tray if disposed immediately after creation", async () => {
    const harness = createHarness();
    const dispose = harness.start();
    await harness.beginLoad();
    await harness.finishLeftover();
    await harness.finishMenu();
    await harness.finishTray();
    expect(harness.stats.tray).toBe(1);
    dispose();
    await flush();
    expect(harness.stats.closeTray).toBe(1);
  });

  it("rolls back a created tray if dispose happens before close registration finishes", async () => {
    const harness = createHarness();
    const dispose = harness.start();
    await harness.beginLoad();
    await harness.finishLeftover();
    await harness.finishMenu();
    await harness.finishTray();
    expect(harness.stats.tray).toBe(1);
    for (let i = 0; i < 40 && harness.stats.closeRequested === 0; i += 1) {
      await Promise.resolve();
    }
    expect(harness.stats.closeRequested).toBe(1);
    dispose();
    await flush();
    expect(harness.stats.closeTray).toBe(1);
    await harness.finishCloseRequested();
    await flush();
    expect(harness.stats.closeUnlisten).toBe(1);
    expect(harness.stats.closeTray).toBe(1);
  });

  it("closes the tray if close-listener registration fails after tray creation", async () => {
    const harness = createHarness();
    harness.start();
    await harness.beginLoad();
    await harness.finishLeftover();
    await harness.finishMenu();
    await harness.finishTray();
    await harness.failCloseRequested(new Error("close listener failed"));
    await flush();
    expect(harness.stats.closeTray).toBe(1);
  });

  it("lets a remount own the tray after StrictMode dispose", async () => {
    const harness = createHarness();
    const first = harness.start();
    first();
    const second = harness.start();
    await harness.beginLoad();
    await harness.finishLeftover();
    await harness.finishMenu();
    await harness.finishTray();
    await harness.finishCloseRequested();
    await flush();
    expect(harness.stats.tray).toBe(1);
    second();
    await flush();
    expect(harness.stats.closeTray).toBe(1);
  });

  it("does not let a stale first attempt close the remounted tray", async () => {
    const firstLoad = deferred<DesktopTrayApis>();
    const secondLoad = deferred<DesktopTrayApis>();
    let loadCalls = 0;
    const stats = { tray: 0, closeTray: 0, leftover: 0 };
    const session = createDesktopTraySession({
      shouldCreateTray: true,
      closeToTray: () => true,
      hideWindow: async () => {},
      loadApis: () => {
        loadCalls += 1;
        return loadCalls === 1 ? firstLoad.promise : secondLoad.promise;
      },
    });

    function apis(id: number) {
      return {
        async closeLeftover() {
          stats.leftover += 1;
        },
        async createMenu() {},
        async createTray() {
          stats.tray += 1;
        },
        async closeTray() {
          stats.closeTray += 1;
        },
        async onCloseRequested() {
          return () => {};
        },
        id,
      };
    }

    const first = session.start();
    const second = session.start();
    first();
    secondLoad.resolve(apis(2));
    await flush();
    firstLoad.resolve(apis(1));
    await flush();
    expect(stats.tray).toBe(1);
    expect(stats.closeTray).toBe(0);
    second();
    await flush();
    expect(stats.closeTray).toBe(1);
  });

  it("does not destroy a remounted tray while the stale close is still in flight", async () => {
    const firstLoad = deferred<DesktopTrayApis>();
    const secondLoad = deferred<DesktopTrayApis>();
    const firstClose = deferred<void>();
    let loadCalls = 0;
    const stats = { tray: 0, closeTray: 0 };
    const session = createDesktopTraySession({
      shouldCreateTray: true,
      closeToTray: () => true,
      hideWindow: async () => {},
      loadApis: () => {
        loadCalls += 1;
        return loadCalls === 1 ? firstLoad.promise : secondLoad.promise;
      },
    });

    const firstApis: DesktopTrayApis = {
      async closeLeftover() {},
      async createMenu() {},
      async createTray() {
        stats.tray += 1;
      },
      async closeTray() {
        stats.closeTray += 1;
        await firstClose.promise;
      },
      async onCloseRequested() {
        return () => {};
      },
    };
    const secondApis: DesktopTrayApis = {
      async closeLeftover() {},
      async createMenu() {},
      async createTray() {
        stats.tray += 1;
      },
      async closeTray() {
        stats.closeTray += 1;
      },
      async onCloseRequested() {
        return () => {};
      },
    };

    const first = session.start();
    firstLoad.resolve(firstApis);
    await flush();
    expect(stats.tray).toBe(1);
    first();
    await flush();
    expect(stats.closeTray).toBe(1);
    expect(stats.tray).toBe(1);

    const second = session.start();
    secondLoad.resolve(secondApis);
    await flush();
    expect(stats.tray).toBe(1);

    firstClose.resolve();
    await flush();
    expect(stats.tray).toBe(2);
    expect(stats.closeTray).toBe(1);
    second();
    await flush();
    expect(stats.closeTray).toBe(2);
  });

  it("does not double-destroy the tray on repeated cleanup", async () => {
    const harness = createHarness();
    const dispose = harness.start();
    await harness.beginLoad();
    await harness.finishLeftover();
    await harness.finishMenu();
    await harness.finishTray();
    await harness.finishCloseRequested();
    dispose();
    dispose();
    await flush();
    expect(harness.stats.closeTray).toBe(1);
    expect(harness.stats.closeUnlisten).toBe(1);
  });
});
