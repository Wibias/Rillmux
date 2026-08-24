/** Shared id so HMR / quit can find and delete the same Windows NotifyIcon. */
export const MAIN_TRAY_ID = "main-tray";

/**
 * `tauri:dev` (and Vite HMR) used to leave a tray icon in the Windows
 * overflow chevron on every run. Killing the console never sends NIM_DELETE,
 * so ghosts stacked. Skip the tray while Vite is in dev; close-to-tray is
 * also ignored there so X actually quits.
 */
export function shouldCreateDesktopTray(viteDev: boolean): boolean {
  return !viteDev;
}
