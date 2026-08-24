/** Stable key so slot order does not kill and relaunch Chatterino. */
export function chatterinoSyncKey(channels: string[]): string {
  return [...new Set(channels.map((c) => c.trim().toLowerCase()).filter(Boolean))]
    .sort()
    .join(",");
}

/** Skip while an open is in flight, and skip a successful same-channel open.
 * Kill+spawn on every layout tick hits Chatterino's single-instance lock and
 * the new process exits with no window. Watchdog relaunches if the process dies. */
export function chatterinoShouldSkipSync(
  key: string,
  lastKey: string,
  inflightKey: string,
): boolean {
  if (!key) return false;
  return key === inflightKey || key === lastKey;
}

/** Don't WM_CLOSE chat because stream_list raced empty while an open is in flight.
 *  When no sessions remain, always close — otherwise a late open leaves chat up. */
export function chatterinoShouldCloseOnEmpty(
  inflightKey: string,
  hasRunningSessions = true,
): boolean {
  if (!hasRunningSessions) return true;
  return !inflightKey;
}
