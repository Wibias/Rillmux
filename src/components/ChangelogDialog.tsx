import { useEffect, useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import changelog from "../../CHANGELOG.md?raw";
import {
  parseReleaseNotes,
  recentChangelogSections,
  type NoteBlock,
} from "../lib/updater/notes";
import "./UpdateDialog.css";
import "./ChangelogDialog.css";

function uniqueNoteKey(
  block: NoteBlock,
  seen: Map<string, number>,
): string {
  const base = `${block.type}:${block.text}`;
  const n = seen.get(base) ?? 0;
  seen.set(base, n + 1);
  return n === 0 ? base : `${base}:${n}`;
}

function ChangelogNotes({ blocks }: { blocks: NoteBlock[] }) {
  const out: React.ReactNode[] = [];
  let bullets: React.ReactNode[] = [];
  const seen = new Map<string, number>();
  const flush = (key: string) => {
    if (!bullets.length) return;
    out.push(
      <ul key={`${key}-ul`} className="changelog-dialog__list">
        {bullets}
      </ul>,
    );
    bullets = [];
  };

  for (const block of blocks) {
    const key = uniqueNoteKey(block, seen);
    switch (block.type) {
      case "bullet":
        bullets.push(
          <li key={key} className="changelog-dialog__bullet">
            {block.text}
          </li>,
        );
        continue;
      case "heading":
        flush(key);
        out.push(
          <h4 key={key} className="changelog-dialog__heading">
            {block.text}
          </h4>,
        );
        continue;
      case "subheading":
        flush(key);
        out.push(
          <h5 key={key} className="changelog-dialog__subheading">
            {block.text}
          </h5>,
        );
        continue;
      case "paragraph":
        flush(key);
        out.push(
          <p key={key} className="changelog-dialog__paragraph">
            {block.text}
          </p>,
        );
        continue;
      default: {
        const _never: never = block;
        return _never;
      }
    }
  }
  flush("tail");
  return <>{out}</>;
}

export function ChangelogDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation(["routes", "common"]);
  const titleId = useId();
  const [sections] = useState(() => recentChangelogSections(changelog, 5));
  const parsed = useMemo(
    () =>
      sections.map((section) => ({
        heading: section.heading,
        blocks: parseReleaseNotes(section.body),
      })),
    [sections],
  );

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <dialog
      className="update-dialog"
      open
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <button
        type="button"
        className="update-dialog__dismiss"
        aria-label={t("common:close")}
        tabIndex={-1}
        onClick={onClose}
      />
      <div className="update-dialog__panel changelog-dialog__panel">
        <header className="update-dialog__header changelog-dialog__header">
          <h2 id={titleId}>{t("routes:changelogTitle")}</h2>
        </header>
        <div className="update-dialog__body changelog-dialog__notes">
          {parsed.map((section) => (
            <section key={section.heading} className="changelog-dialog__section">
              <h3 className="changelog-dialog__version">{section.heading}</h3>
              <ChangelogNotes blocks={section.blocks} />
            </section>
          ))}
        </div>
        <footer className="update-dialog__footer">
          <button type="button" onClick={onClose}>
            {t("common:close")}
          </button>
        </footer>
      </div>
    </dialog>
  );
}
