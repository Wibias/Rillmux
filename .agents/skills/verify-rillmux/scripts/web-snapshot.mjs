/** Shared web-lane snapshot helpers. Evaluated in Node tests and inlined into CDP. */

export const DIALOG_SELECTOR = 'dialog[open], [role="dialog"]';

export function dialogRecords(document, text) {
  const labelOf = (el) => {
    const labelledBy = el.getAttribute("aria-labelledby");
    if (labelledBy) return text(document.getElementById(labelledBy));
    return text(el.querySelector("h2"));
  };
  return [...document.querySelectorAll(DIALOG_SELECTOR)].map((el) => ({
    label: labelOf(el),
    text: text(el).slice(0, 400),
  }));
}

/** A hydrated shell is only ready when the actual brand label identifies Rillmux.
 *  The native lane must additionally reject a Vite-only Tauri guard. */
export function snapshotReady(snapshot, { native = false } = {}) {
  if (snapshot?.hydrated !== true || snapshot?.appName !== "Rillmux") return false;
  return !native || !(snapshot.alerts || []).some((alert) =>
    typeof alert === "string" && alert.includes("Desktop shell required"));
}
