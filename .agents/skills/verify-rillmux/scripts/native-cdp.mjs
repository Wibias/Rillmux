/** Native CDP discovery helpers. Keep fetch timeouts short so probes cannot hang the driver. */

export function cdpListUrls(port) {
  return [
    `http://127.0.0.1:${port}/json/list`,
    `http://localhost:${port}/json/list`,
    `http://[::1]:${port}/json/list`,
  ];
}

export function selectCdpPage(targets) {
  const pages = (targets || []).filter(
    (target) => target?.type === "page" && typeof target.webSocketDebuggerUrl === "string",
  );
  const urlOf = (target) => String(target.url || "");
  const rillmux = pages.find((target) => {
    const url = urlOf(target);
    return (
      target.title === "Rillmux" ||
      /localhost:1420|127\.0\.0\.1:1420|\[::1\]:1420/.test(url)
    );
  });
  if (rillmux) return rillmux;
  return (
    pages.find((target) =>
      /^https?:\/\/(127\.0\.0\.1|localhost|\[::1\])/.test(urlOf(target)),
    ) || null
  );
}

export function nativeCleanupPids(state) {
  const native = state?.native;
  if (!native) return [];
  return [...new Set([...(native.ownedPids || []), native.pid].filter(Boolean))];
}

export async function fetchCdpJson(port, timeoutMs = 1500) {
  let last = "no CDP endpoints tried";
  for (const url of cdpListUrls(port)) {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
      if (res.ok) return await res.json();
      last = `${url} HTTP ${res.status}`;
    } catch (error) {
      last = `${url} ${error.cause?.message || error.message}`;
    }
  }
  throw new Error(last);
}
