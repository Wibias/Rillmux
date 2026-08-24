import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const shellCss = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "AppShell.css"),
  "utf8",
);
const globalCss = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "../styles/global.css"),
  "utf8",
);

describe("titlebar overlay chrome", () => {
  it("styles #tbo-controls in our stylesheet so CSP cannot hide the caption buttons", () => {
    // tauri-plugin-window-controls injects a <style> tag. Production CSP is
    // style-src 'self', so that stylesheet is dropped. The plugin still mounts
    // #tbo-controls, and we hide the HTML fallback when that node exists.
    // Caption layout must therefore live in a bundled CSS file.
    expect(shellCss).toMatch(
      /#tbo-controls\s*\{[^}]*position:\s*fixed[^}]*right:\s*0/s,
    );
    expect(shellCss).toMatch(/#tbo-controls\s+\.tbo-btn\s*\{[^}]*width:\s*46px/s);
    expect(shellCss).toMatch(
      /#tbo-controls\s+\.tbo-close:hover\s*\{[^}]*background:\s*#e81123/s,
    );
  });

  it("hides injected caption buttons inside overlay documents so X closes Rillmux", () => {
    expect(globalCss).toMatch(
      /html\[data-overlay\]\s+#tbo-controls\s*\{[^}]*display:\s*none\s*!important/s,
    );
  });

  it("keeps HTML caption buttons clickable so the native overlay cannot swallow X", () => {
    expect(shellCss).not.toMatch(
      /body:has\(#tbo-controls\)\s+\.shell__win-controls\s*\{[^}]*display:\s*none/s,
    );
    expect(shellCss).toMatch(/#tbo-controls\s*\{[^}]*pointer-events:\s*none/s);
  });
});
