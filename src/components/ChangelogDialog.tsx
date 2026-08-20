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

function ChangelogNotes({ blocks }: { blocks: NoteBlock[] }) {
  const out: React.ReactNode[] = [];
  let bullets: React.ReactNode[] = [];
  const flush = (key: string) => {
    if (!bullets.length) return;
    out.push(
      <ul key={`${key}-ul`} className="changelog-dialog__list">
        {bullets}
      </ul>,
    );
    bullets = [];
  };

  blocks.forEach((block, index) => {
    switch (block.type) {
      case "bullet":
        bullets.push(
          <li key={index} className="changelog-dialog__bullet">
            {block.text}
          </li>,
        );
        return;
      case "heading":
        flush(String(index));
        out.push(
          <h4 key={index} className="changelog-dialog__heading">
            {block.text}
          </h4>,
        );
        return;
      case "subheading":
        flush(String(index));
        out.push(
          <h5 key={index} className="changelog-dialog__subheading">
            {block.text}
          </h5>,
        );
        return;
      case "paragraph":
        flush(String(index));
        out.push(
          <p key={index} className="changelog-dialog__paragraph">
            {block.text}
          </p>,
        );
        return;
      default: {
        const _never: never = block;
        return _never;
      }
    }
  });
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
    <div
      className="update-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
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
    </div>
  );
}
