/**
 * Lightweight release-notes renderer for the update dialog.
 *
 * The updater manifest ships `notes` as Markdown (see
 * scripts/generate-updater-manifest.mjs, which reads the CHANGELOG.md
 * section for the released version). We render a safe subset instead of
 * pulling in a full Markdown dependency: `##`/`###` headings, `- ` bullets,
 * `**bold**`, and `[label](url)` links.
 */

export type NoteBlock =
  | { type: "heading"; text: string }
  | { type: "subheading"; text: string }
  | { type: "bullet"; text: string }
  | { type: "paragraph"; text: string };

/**
 * Only http/https links may become clickable anchors in the update dialog.
 * Anything else (javascript:, data:, file:, vbscript:, relative or malformed
 * URLs) is rejected so release notes can never drive navigation or script
 * execution. The note text still renders, just without a link.
 */
export function isSafeUrl(raw: string | null | undefined): boolean {
  if (!raw) return false;
  // Reject whitespace, quotes and angle brackets outright: they can smuggle
  // attributes or multiple URLs past a naive parser.
  if (/[\s"'<>]/.test(raw)) return false;
  try {
    const url = new URL(raw);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

export function parseReleaseNotes(
  markdown: string | null | undefined,
): NoteBlock[] {
  if (!markdown) return [];
  const blocks: NoteBlock[] = [];
  for (const rawLine of markdown.split(/\r?\n/)) {
    const line = rawLine.trimEnd();
    if (!line.trim()) continue;
    if (line.startsWith("### ")) {
      blocks.push({ type: "subheading", text: line.slice(4).trim() });
    } else if (line.startsWith("## ")) {
      blocks.push({ type: "heading", text: line.slice(3).trim() });
    } else if (line.startsWith("- ")) {
      blocks.push({ type: "bullet", text: line.slice(2).trim() });
    } else {
      blocks.push({ type: "paragraph", text: line.trim() });
    }
  }
  return blocks;
}

export interface ChangelogSection {
  heading: string;
  body: string;
}

/** Keep a Changelog sections, newest first, skipping Unreleased. */
export function recentChangelogSections(
  markdown: string | null | undefined,
  limit = 5,
): ChangelogSection[] {
  if (!markdown) return [];
  const matches = [...markdown.matchAll(/^## \[([^\]]+)\][^\n]*$/gm)];
  const sections: ChangelogSection[] = [];
  for (let i = 0; i < matches.length; i += 1) {
    const match = matches[i];
    const version = match[1]?.trim() ?? "";
    if (!version || version.toLowerCase() === "unreleased") continue;
    const start = (match.index ?? 0) + match[0].length;
    const end = matches[i + 1]?.index ?? markdown.length;
    sections.push({
      heading: match[0].replace(/^##\s+/, "").trim(),
      body: markdown.slice(start, end).trim(),
    });
    if (sections.length >= limit) break;
  }
  return sections;
}
