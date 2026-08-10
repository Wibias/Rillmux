import { describe, expect, it } from "vitest";
import { isSafeUrl, parseReleaseNotes } from "./notes";

describe("parseReleaseNotes", () => {
  it("returns [] for empty notes", () => {
    expect(parseReleaseNotes(null)).toEqual([]);
    expect(parseReleaseNotes("")).toEqual([]);
    expect(parseReleaseNotes("  \n\n  ")).toEqual([]);
  });

  it("maps headings, subheadings, bullets and paragraphs", () => {
    const blocks = parseReleaseNotes(
      [
        "## [0.5.0] - 2026-08-10",
        "",
        "### Added",
        "- **Twitch Website authentication** for authenticated playback",
        "- Multistream vertical layouts",
        "",
        "Plain paragraph describing the release.",
      ].join("\n"),
    );
    expect(blocks).toEqual([
      { type: "heading", text: "[0.5.0] - 2026-08-10" },
      { type: "subheading", text: "Added" },
      {
        type: "bullet",
        text: "**Twitch Website authentication** for authenticated playback",
      },
      { type: "bullet", text: "Multistream vertical layouts" },
      { type: "paragraph", text: "Plain paragraph describing the release." },
    ]);
  });

  it("skips blank lines", () => {
    const blocks = parseReleaseNotes("## Head\n\n\n- one\n\n- two\n");
    expect(blocks).toHaveLength(3);
    expect(blocks.map((b) => b.text)).toEqual(["Head", "one", "two"]);
  });
});

describe("isSafeUrl", () => {
  it("allows https and http", () => {
    expect(isSafeUrl("https://github.com/Wibias/streamlink-twitch-gui/releases/tag/v0.5.0")).toBe(true);
    expect(isSafeUrl("http://example.com/x")).toBe(true);
    expect(isSafeUrl("https://example.com/a?b=c#d")).toBe(true);
  });

  it("rejects javascript:, data:, file:, vbscript: and mailto:", () => {
    expect(isSafeUrl("javascript:alert(1)")).toBe(false);
    expect(isSafeUrl("data:text/html,<script>alert(1)</script>")).toBe(false);
    expect(isSafeUrl("file:///C:/Windows/notepad.exe")).toBe(false);
    expect(isSafeUrl("vbscript:msgbox(1)")).toBe(false);
    expect(isSafeUrl("mailto:a@b.c")).toBe(false);
  });

  it("rejects relative and malformed URLs", () => {
    expect(isSafeUrl("/relative/path")).toBe(false);
    expect(isSafeUrl("not a url")).toBe(false);
    expect(isSafeUrl("")).toBe(false);
    expect(isSafeUrl(null)).toBe(false);
  });

  it("rejects whitespace, quotes and angle brackets", () => {
    expect(isSafeUrl("https://example.com/a b")).toBe(false);
    expect(isSafeUrl('https://example.com/"onclick=alert(1)')).toBe(false);
    expect(isSafeUrl("https://example.com/<script>")).toBe(false);
    expect(isSafeUrl("https://example.com/\t\n")).toBe(false);
  });
});
