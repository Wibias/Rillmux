import type { HelixStream } from "../../twitch/helix";
import type { RaidOutgoingEvent } from "../raid";

export interface StreamSession {
  id: string;
  channel: string;
  quality: string;
  title?: string | null;
  game?: string | null;
  running: boolean;
  status?: string;
  phase?: string;
  ready?: boolean;
  muted?: boolean;
}

export interface StreamStatusEvent {
  id: string;
  channel: string;
  line: string;
  status: string;
  phase: string;
  ready: boolean;
}

export interface WatchingState {
  sessions: StreamSession[];
  /** Ordered Multistream slots (lowercase logins); empty in other modes. */
  slotChannels: string[];
  activeChatChannel: string | null;
  error: string | null;
  refresh: () => Promise<void>;
  watchStream: (stream: HelixStream) => Promise<void>;
  /** Replace the raiding session without killing unrelated sessions. */
  followRaid: (raid: RaidOutgoingEvent) => Promise<void>;
  stopSession: (id: string) => Promise<void>;
  stopAll: () => Promise<void>;
  toggleMute: (id: string) => Promise<void>;
  moveSlot: (channel: string, direction: -1 | 1) => void;
  /** Drag & drop reorder: replace the slot order outright (same channels). */
  reorderSlots: (channels: string[]) => void;
  /** Retile + resync chat after mode/layout changes. */
  applyLayout: () => void;
  setActiveChat: (channel: string | null) => void;
  applyStatus: (payload: StreamStatusEvent) => void;
}

