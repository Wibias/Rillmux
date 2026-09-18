import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { DIALOG_SELECTOR, dialogRecords } from "./web-snapshot.mjs";

function textOf(el) {
  return (el?.innerText || el?.textContent || "").replace(/\s+/g, " ").trim();
}

function makeAriaChangelogDocument() {
  const title = { id: "chg-title", innerText: "Changelog", textContent: "Changelog" };
  const dialog = {
    getAttribute(name) {
      if (name === "aria-labelledby") return "chg-title";
      if (name === "role") return "dialog";
      return null;
    },
    querySelector() {
      return title;
    },
    innerText: "Changelog [0.5.6] notes",
    textContent: "Changelog [0.5.6] notes",
  };
  return {
    getElementById(id) {
      return id === "chg-title" ? title : null;
    },
    querySelectorAll(sel) {
      if (sel === DIALOG_SELECTOR) return sel.includes("role") ? [dialog] : [];
      if (sel === "dialog[open]") return [];
      if (sel === '[role="dialog"]') return [dialog];
      return [];
    },
  };
}

describe("web snapshot dialogs", () => {
  it("records an ARIA changelog dialog that is not a native <dialog open>", () => {
    const rows = dialogRecords(makeAriaChangelogDocument(), textOf);
    assert.match(DIALOG_SELECTOR, /role="dialog"/);
    assert.equal(rows.some((row) => row.label === "Changelog"), true);
  });
});
