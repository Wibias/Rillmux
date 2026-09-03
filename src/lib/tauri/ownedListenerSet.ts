export type ListenerRegister = () => Promise<() => void>;

export function createOwnedListenerSet() {
  let owners = 0;
  let unlistens: Array<() => void> | null = null;
  let inflight: Promise<void> | null = null;

  async function bindAll(registers: ListenerRegister[]): Promise<void> {
    const results = await Promise.allSettled(
      registers.map((register) => register()),
    );
    const owned: Array<() => void> = [];
    let failure: unknown;
    for (const result of results) {
      if (result.status === "fulfilled") {
        owned.push(result.value);
      } else if (failure === undefined) {
        failure = result.reason;
      }
    }
    if (failure !== undefined) {
      for (const unlisten of owned) unlisten();
      throw failure;
    }
    unlistens = owned;
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
