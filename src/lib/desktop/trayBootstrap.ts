export type DesktopCloseRequest = {
  preventDefault: () => void;
};

export type DesktopTrayApis = {
  closeLeftover: () => Promise<void>;
  createMenu: () => Promise<void>;
  createTray: () => Promise<void>;
  closeTray: () => Promise<void>;
  onCloseRequested: (
    handler: (event: DesktopCloseRequest) => Promise<void>,
  ) => Promise<() => void>;
};

export type DesktopTraySessionDeps = {
  loadApis: () => Promise<DesktopTrayApis>;
  shouldCreateTray: boolean;
  closeToTray: () => boolean;
  hideWindow: () => Promise<void>;
};

export function createDesktopTraySession(deps: DesktopTraySessionDeps) {
  let generation = 0;
  let trayOwner = 0;
  let trayOp: Promise<void> = Promise.resolve();

  function runTrayOp(op: () => Promise<void>): Promise<void> {
    const next = trayOp.then(op, op);
    trayOp = next.then(
      () => undefined,
      () => undefined,
    );
    return next;
  }

  return {
    start(): () => void {
      const attempt = ++generation;
      let cancelled = false;
      let unlistenClose: (() => void) | undefined;
      let createdTray = false;
      let cleaned = false;
      let apis: DesktopTrayApis | undefined;

      const abandoned = () => cancelled || attempt !== generation;

      async function rollback() {
        unlistenClose?.();
        unlistenClose = undefined;
        await runTrayOp(async () => {
          if (cleaned) return;
          cleaned = true;
          if (!createdTray || trayOwner !== attempt) return;
          createdTray = false;
          trayOwner = 0;
          await apis?.closeTray().catch(() => undefined);
        });
      }

      void (async () => {
        const loaded = await deps.loadApis();
        if (abandoned()) return;
        apis = loaded;
        await runTrayOp(async () => {
          if (abandoned()) return;
          await loaded.closeLeftover();
          if (abandoned()) return;
          if (!deps.shouldCreateTray) return;
          await loaded.createMenu();
          if (abandoned()) return;
          await loaded.createTray();
          createdTray = true;
          trayOwner = attempt;
        });
        if (abandoned()) {
          await rollback();
          return;
        }
        try {
          unlistenClose = await loaded.onCloseRequested(async (event) => {
            if (deps.shouldCreateTray && deps.closeToTray()) {
              event.preventDefault();
              await deps.hideWindow();
            }
          });
        } catch {
          await rollback();
          return;
        }
        if (abandoned()) {
          unlistenClose?.();
          unlistenClose = undefined;
          await rollback();
        }
      })();

      return () => {
        if (cancelled) return;
        cancelled = true;
        void rollback();
      };
    },
  };
}
