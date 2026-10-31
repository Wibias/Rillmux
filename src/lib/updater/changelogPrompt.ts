import { normalizeAppVersion } from "./prompt";

/** Show release notes once per installed version, after first-run setup. */
export function shouldShowChangelogAfterUpdate(opts: {
  currentVersion?: string | null;
  lastSeenVersion?: string | null;
  onboardingDone: boolean;
}): boolean {
  if (!opts.onboardingDone) return false;
  const current = normalizeAppVersion(opts.currentVersion);
  if (!current) return false;
  return normalizeAppVersion(opts.lastSeenVersion) !== current;
}
