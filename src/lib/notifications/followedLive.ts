export function followedStreamsQueryKey(userId: string | null | undefined) {
  return ["followed-streams", userId] as const;
}

export function liveFollowedLogins(
  streams: Array<{ user_login: string }>,
): Set<string> {
  return new Set(streams.map((stream) => stream.user_login.toLowerCase()));
}

export function newlyLiveFollowedLogins(
  previous: Set<string>,
  next: Set<string>,
): string[] {
  return [...next].filter((login) => !previous.has(login));
}

/** True when a newly-live followed login should raise a desktop notification. */
export function shouldNotifyFollowedLive(
  login: string,
  opts: { followedOnline: boolean; mutedFollowed: string[] },
): boolean {
  if (!opts.followedOnline) return false;
  const key = login.trim().toLowerCase();
  if (!key) return false;
  return !opts.mutedFollowed.some((m) => m.toLowerCase() === key);
}

export function toggleMutedFollowed(
  muted: string[],
  login: string,
  notify: boolean,
): string[] {
  const key = login.trim().toLowerCase();
  if (!key) return muted;
  const without = muted.filter((m) => m.toLowerCase() !== key);
  if (notify) return without;
  return [...without, key];
}
