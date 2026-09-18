/** Run-scoped receipt provenance. Shared state.json may still list both lanes. */

export function webRunResources(state) {
  if (!state?.runId) return [];
  const list = [];
  if (state.previewPid || state.port) {
    list.push({
      kind: "http",
      id: "vite-web",
      port: state.port,
      origin: state.origin,
      pid: state.previewPid ?? null,
    });
  }
  if (state.browserPid) {
    list.push({
      kind: "browser-cdp",
      id: "edge-chrome",
      port: state.cdpPort,
      pid: state.browserPid,
      profileDir: state.profileDir,
    });
  }
  return list;
}

export function nativeRunResources(native) {
  if (!native?.runId) return [];
  return [
    {
      kind: "tauri-dev",
      id: "native",
      port: native.port,
      origin: native.origin,
      pid: native.pid ?? null,
      cdpPort: native.cdpPort,
      appDataDir: native.appDataDir,
    },
  ];
}

export function receiptStartedResources(state, { surface, runId } = {}) {
  const desktop = surface === "desktop" || surface === "native";
  if (desktop) {
    const native = state?.native;
    if (!native?.runId) return [];
    if (runId && native.runId !== runId) return [];
    return Array.isArray(native.startedResources)
      ? native.startedResources
      : nativeRunResources(native);
  }
  if (!state?.runId) return [];
  if (runId && state.runId !== runId) return [];
  return Array.isArray(state.webStartedResources)
    ? state.webStartedResources
    : webRunResources(state);
}
