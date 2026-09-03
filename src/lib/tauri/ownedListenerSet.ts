export type ListenerRegister = () => Promise<() => void>;

export function createOwnedListenerSet() {
  let owners = 0;
  let unlistens: Array<() => void> | null = null;
  let inflight: Promise<void> | null = null;

  async function bindAll(registers: ListenerRegister[]): Promise<void> {
    if (registers.length === 0) {
      unlistens = [];
      return;
    }

    const owned: Array<() => void> = [];
    let failure: unknown;

    await new Promise<void>((resolve, reject) => {
      let remaining = registers.length;

      const fail = (reason: unknown) => {
        if (failure !== undefined) return;
        failure = reason;
        for (const unlisten of owned) unlisten();
        owned.length = 0;
        reject(reason);
      };

      for (const register of registers) {
        void Promise.resolve()
          .then(() => register())
          .then(
            (unlisten) => {
              if (failure !== undefined) {
                unlisten();
                return;
              }
              owned.push(unlisten);
              remaining -= 1;
              if (remaining === 0) {
                unlistens = owned.slice();
                resolve();
              }
            },
            (reason) => {
              fail(reason);
            },
          );
      }
    });
  }

  function release() {
    const current = unlistens;
    unlistens = null;
    for (const unlisten of current ?? []) unlisten();
  }

  async function ensureBound(registers: ListenerRegister[]): Promise<void> {
    if (unlistens) return;
    if (!inflight) {
      inflight = bindAll(registers).finally(() => {
        inflight = null;
      });
    }
    await inflight;
  }

  return {
    get bound() {
      return unlistens != null;
    },
    async bind(registers: ListenerRegister[]): Promise<() => void> {
      owners += 1;
      try {
        await ensureBound(registers);
      } catch (error) {
        owners -= 1;
        throw error;
      }
      let released = false;
      return () => {
        if (released) return;
        released = true;
        owners -= 1;
        if (owners === 0) release();
      };
    },
  };
}
