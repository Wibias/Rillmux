import type { PresenceMetadata } from "../presence";

// Store-owned mutable state shared between the sync helpers and the store actions.
export const chatSyncState = {
  lastKey: "",
  inflight: "",
  generation: 0,
};

export const layoutState: { timer: ReturnType<typeof setTimeout> | null } = {
  timer: null,
};

export const presenceState: {
  metadata: PresenceMetadata;
  lastSyncKey: string;
} = {
  metadata: {},
  lastSyncKey: "",
};

export const independentChatChannels = new Set<string>();

