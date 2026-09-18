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
