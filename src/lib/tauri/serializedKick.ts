export type SerializedPass = (isCurrent: () => boolean) => Promise<void>;

export function createSerializedKick(run: SerializedPass) {
  let generation = 0;
  let inflight = false;
  let queued = false;
  let active = true;

  async function runKick() {
    if (!active) return;
    if (inflight) {
      queued = true;
      return;
    }
    inflight = true;
    try {
      do {
        queued = false;
        if (!active) break;
        const gen = generation;
        await run(() => active && generation === gen);
      } while (queued && active);
    } finally {
      inflight = false;
    }
  }

  return {
    get inflight() {
      return inflight;
    },
    kick() {
      void runKick();
    },
    invalidate() {
      generation += 1;
    },
    dispose() {
      active = false;
      generation += 1;
      queued = false;
    },
  };
}
