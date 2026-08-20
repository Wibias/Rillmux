import type { HelixStream } from "../twitch/helix";
import { MAX_MULTISTREAMS } from "./layout";

export interface PresenceSource {
  channelLogin: string;
  channelId: string;
  broadcastId: string;
}

export type PresenceMetadata = Record<string, PresenceSource>;

export interface PresenceSession {
  id: string;
  running: boolean;
  ready?: boolean;
}

export interface ViewerPresenceTarget extends PresenceSource {
  sessionId: string;
}

export interface ViewerPresenceWorkerStatus {
  sessionId: string;
  channelLogin: string;
  lastStage: string;
  lastHttpStatus?: number | null;
  lastError?: string | null;
  lastSuccessUnixMs?: number | null;
}

export interface ViewerPresenceStatus {
  enabled: boolean;
  activeSessionIds: string[];
  limited: boolean;
  workers: ViewerPresenceWorkerStatus[];
}

export function prunePresenceMetadata(
  metadata: PresenceMetadata,
  sessions: PresenceSession[],
): PresenceMetadata {
  const active = new Set(sessions.map((session) => session.id));
  return Object.fromEntries(
    Object.entries(metadata).filter(([sessionId]) => active.has(sessionId)),
  );
}

export function presenceSourceFromStream(
  stream: Pick<HelixStream, "id" | "user_id" | "user_login">,
): PresenceSource | null {
  const channelLogin = stream.user_login.trim().toLowerCase();
  const channelId = stream.user_id.trim();
  const broadcastId = stream.id.trim();
  if (!channelLogin || !channelId || !broadcastId) {
    return null;
  }
  return { channelLogin, channelId, broadcastId };
}

export function buildPresenceTargets(
  sessions: PresenceSession[],
  metadata: PresenceMetadata,
  preferredSessionIds: string[] = [],
): ViewerPresenceTarget[] {
  const rank = new Map(
    preferredSessionIds.map((sessionId, index) => [sessionId, index]),
  );
  const fallbackRank = preferredSessionIds.length;
  const ordered = sessions
    .map((session, index) => ({ session, index }))
    .sort((left, right) => {
      const leftRank = rank.get(left.session.id) ?? fallbackRank + left.index;
      const rightRank = rank.get(right.session.id) ?? fallbackRank + right.index;
      return leftRank - rightRank;
    })
    .map(({ session }) => session);

  return ordered
    .filter((session) => session.running && session.ready)
    .flatMap((session) => {
      const source = metadata[session.id];
      if (
        !source?.channelLogin.trim() ||
        !source.channelId.trim() ||
        !source.broadcastId.trim()
      ) {
        return [];
      }
      return [
        {
          sessionId: session.id,
          channelLogin: source.channelLogin.toLowerCase(),
          channelId: source.channelId,
          broadcastId: source.broadcastId,
        },
      ];
    })
    .slice(0, MAX_MULTISTREAMS);
}

export function describeViewerPresenceStatus(
  status: ViewerPresenceStatus | null,
): string {
  if (!status) return "Diagnostics unavailable.";
  if (!status.enabled) return "Channel Points presence is disabled.";
  if (!status.workers.length) return "Waiting for a ready Streamlink session.";

  return status.workers
    .map((worker) => {
      const http = worker.lastHttpStatus ? ` HTTP ${worker.lastHttpStatus}` : "";
      if (worker.lastError) {
        return `${worker.channelLogin}: ${worker.lastStage}${http} — ${worker.lastError}`;
      }
      if (worker.lastStage === "telemetry-accepted" && worker.lastSuccessUnixMs) {
        return `${worker.channelLogin}: telemetry accepted${http}`;
      }
      return `${worker.channelLogin}: ${worker.lastStage}${http}`;
    })
    .join(" | ");
}
