import type { ChannelPointsSnapshot } from "./types";

// Host-side overlay state shared by the poll/prediction actions and the hook.
export const hostState: {
  lastHostSnapshot: ChannelPointsSnapshot | null;
  pollRefreshRunning: boolean;
} = {
  lastHostSnapshot: null,
  pollRefreshRunning: false,
};

