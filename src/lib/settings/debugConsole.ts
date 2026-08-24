/** Overlay webviews share SettingsBootstrap; only the main window owns the debug console. */
export function isOverlayWebview(search = window.location.search): boolean {
  return Boolean(new URLSearchParams(search).get("overlay"));
}

export function shouldAttachDebugConsole(
  search = window.location.search,
): boolean {
  return !isOverlayWebview(search);
}
