const SKIPPED_UPDATE_KEY = "rillmux.skippedUpdateVersion";

export function normalizeAppVersion(version?: string | null): string {
  return (version ?? "").trim().replace(/^v/i, "");
}

/** Numeric dotted compare; missing segments count as 0. */
export function compareAppVersions(left: string, right: string): number {
  const a = normalizeAppVersion(left)
    .split(".")
    .map((part) => Number.parseInt(part, 10) || 0);
  const b = normalizeAppVersion(right)
    .split(".")
    .map((part) => Number.parseInt(part, 10) || 0);
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i += 1) {
    const av = a[i] ?? 0;
    const bv = b[i] ?? 0;
    if (av > bv) return 1;
    if (av < bv) return -1;
  }
  return 0;
}

export function shouldPromptAppUpdate(opts: {
  viteDev: boolean;
  currentVersion?: string | null;
  availableVersion?: string | null;
  skippedVersion?: string | null;
}): boolean {
  if (opts.viteDev) return false;
  const available = normalizeAppVersion(opts.availableVersion);
  if (!available) return true;
  const skipped = normalizeAppVersion(opts.skippedVersion);
  if (skipped && skipped === available) return false;
  const current = normalizeAppVersion(opts.currentVersion);
  if (!current) return true;
  return compareAppVersions(available, current) > 0;
}

export function readSkippedUpdateVersion(
  storage: Pick<Storage, "getItem"> | null | undefined,
): string | null {
  if (!storage) return null;
  try {
    return storage.getItem(SKIPPED_UPDATE_KEY);
  } catch {
    return null;
  }
}

export function writeSkippedUpdateVersion(
  storage: Pick<Storage, "setItem"> | null | undefined,
  version: string,
): void {
  if (!storage) return;
  try {
    storage.setItem(SKIPPED_UPDATE_KEY, normalizeAppVersion(version));
  } catch {
    // Private mode / disabled storage: keep the in-session skip only.
  }
}
