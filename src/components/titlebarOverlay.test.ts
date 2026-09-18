import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const here = dirname(fileURLToPath(import.meta.url));
const shellCss = readFileSync(join(here, "AppShell.css"), "utf8");
const controlsTsx = readFileSync(join(here, "TitlebarControls.tsx"), "utf8");

describe("titlebar overlay chrome", () => {
  it("styles decorum's injected titlebar from our stylesheet, not its own <style> tag", () => {
    // tauri-plugin-decorum injects a <style> tag plus inline styles (32px
    // strip height). Production CSP is style-src 'self', so that geometry is
    // dropped and the 38px contract has to live in a bundled CSS file.
    expect(shellCss).toMatch(
      /div\[data-tauri-decorum-tb\]\s*\{[^}]*--title-bar-height:\s*38px/s,
    );
    expect(shellCss).toMatch(
      /div\[data-tauri-decorum-tb\]\s*\{[^}]*height:\s*var\(--title-bar-height\)\s*!important/s,
    );
  });

  it("keeps decorum's strip out of the hit-test path and suppresses its buttons", () => {
    // Rillmux renders its own caption buttons (.shell__win-controls): they
    // follow the theme palette and close through app_quit, while decorum's own
    // close uses win.close(). A second button set would also land on the
    // titlebar chips, so the injected strip must not intercept clicks.
    expect(shellCss).toMatch(
      /div\[data-tauri-decorum-tb\]\s*\{[^}]*pointer-events:\s*none/s,
    );
    expect(shellCss).toMatch(
      /div\[data-tauri-decorum-tb\]\s+\[data-tauri-drag-region\]\s*\{[^}]*pointer-events:\s*none/s,
    );
    expect(shellCss).toMatch(
      /div\[data-tauri-decorum-tb\]\s+\.decorum-tb-btn\s*\{[^}]*display:\s*none/s,
    );
  });

  it("keeps the visible caption buttons on the 46px by 38px contract", () => {
    // 2.875rem is 46px at the app's 16px root size, matching
    // POINTS_HUD_CAPTION_WIDTH_CSS (3 x 46px) in the Rust overlay geometry.
    expect(shellCss).toMatch(
      /\.shell__win-controls\s+button\s*\{[^}]*width:\s*2\.875rem[^}]*height:\s*var\(--title-bar-height,\s*38px\)/s,
    );
    expect(shellCss).toMatch(
      /\.shell__win-close:hover\s*\{[^}]*background:\s*#e81123/s,
    );
  });

  it("keeps the caption behavior decorum's own buttons would replace", () => {
    // X must stay on app_quit (decorum's close is win.close()), and the
    // maximize button must still raise decorum's snap overlay.
    expect(controlsTsx).toMatch(/shell__win-close[\s\S]*?invoke\("app_quit"\)/);
    expect(controlsTsx).toMatch(/plugin:decorum\|show_snap_overlay/);
  });
});
