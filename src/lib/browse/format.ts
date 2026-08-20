export function formatViewers(count: number): string {
  if (count >= 1_000_000) {
    return `${(count / 1_000_000).toFixed(1)}M`;
  }
  if (count >= 1_000) {
    return `${(count / 1_000).toFixed(count >= 10_000 ? 0 : 1)}K`;
  }
  return String(count);
}

export function formatUptime(startedAt: string, now = Date.now()): string {
  const started = Date.parse(startedAt);
  if (!Number.isFinite(started) || started > now) return "—";
  const minutes = Math.floor((now - started) / 60_000);
  const days = Math.floor(minutes / (60 * 24));
  const hours = Math.floor((minutes % (60 * 24)) / 60);
  const mins = minutes % 60;
  if (days > 0) {
    return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  }
  if (hours > 0) {
    return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`;
  }
  return `${Math.max(0, mins)}m`;
}

export function twitchChannelUrl(login: string): string {
  return `https://www.twitch.tv/${encodeURIComponent(login.trim())}`;
}

export function formatStartedAt(
  startedAt: string,
  now = Date.now(),
  locale = "en-US",
): string {
  const started = Date.parse(startedAt);
  if (!Number.isFinite(started)) return "—";
  const date = new Date(started);
  const current = new Date(now);
  const time = date.toLocaleTimeString(locale, {
    hour: "numeric",
    minute: "2-digit",
  });
  const startDay = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const today = new Date(
    current.getFullYear(),
    current.getMonth(),
    current.getDate(),
  );
  const diffDays = Math.round(
    (startDay.getTime() - today.getTime()) / 86_400_000,
  );
  if (diffDays === 0) return `Today, ${time}`;
  if (diffDays === -1) return `Yesterday, ${time}`;
  const day = date.toLocaleDateString(locale, {
    month: "short",
    day: "numeric",
  });
  return `${day}, ${time}`;
}
