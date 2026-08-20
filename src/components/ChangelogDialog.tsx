import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import changelog from "../../CHANGELOG.md?raw";
import { parseReleaseNotes, recentChangelogSections } from "../lib/updater/notes";
import "./UpdateDialog.css";
import "./ChangelogDialog.css";

export function ChangelogDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation("routes");
  const [sections] = useState(() => recentChangelogSections(changelog, 5));

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="update-dialog__backdrop"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="update-dialog changelog-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="changelog-dialog-title"
      >
        <header className="update-dialog__header">
          <h2 id="changelog-dialog-title">{t("changelogTitle")}</h2>
          <button type="button" className="button-secondary" onClick={onClose}>
            {t("common:cancel")}
          </button>
        </header>
        <div className="update-dialog__notes changelog-dialog__notes">
          {sections.map((section) => (
            <section key={section.heading} className="changelog-dialog__section">
              <h3>{section.heading}</h3>
              {parseReleaseNotes(section.body).map((block, index) => {
                if (block.type === "heading") return <h3 key={index}>{block.text}</h3>;
                if (block.type === "subheading") return <h4 key={index}>{block.text}</h4>;
                if (block.type === "bullet") return <li key={index}>{block.text}</li>;
                return <p key={index}>{block.text}</p>;
              })}
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}
