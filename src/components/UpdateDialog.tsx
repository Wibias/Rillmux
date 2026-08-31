import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { isTauri } from "../lib/tauri";
import { isSafeUrl, parseReleaseNotes } from "../lib/updater/notes";
import "./UpdateDialog.css";

type Phase =
  | "available"
  | "downloading"
  | "installing"
  | "error";

interface UpdateHandle {
  version: string;
  body?: string;
  downloadAndInstall: (
    cb?: (event: {
      event: "Started" | "Progress" | "Finished";
      data: { contentLength?: number; chunkLength: number };
    }) => void,
  ) => Promise<void>;
}

interface UpdateDialogProps {
  update: UpdateHandle;
  onCancel: () => void;
}

/** Format the version string for display, tolerating a leading "v". */
function formatVersion(version: string): string {
  return version.replace(/^v/i, "");
}

function uniquePartKey(base: string, seen: Map<string, number>): string {
  const n = seen.get(base) ?? 0;
  seen.set(base, n + 1);
  return n === 0 ? base : `${base}:${n}`;
}

function InlineText({ text }: { text: string }) {
  const parts = useMemo(() => {
    // Split **bold** and [label](url) spans; everything else is plain text.
    const out: {
      kind: "text" | "bold" | "link";
      value: string;
      href?: string;
      key: string;
    }[] = [];
    const seen = new Map<string, number>();
    const push = (part: {
      kind: "text" | "bold" | "link";
      value: string;
      href?: string;
    }) => {
      out.push({
        ...part,
        key: uniquePartKey(
          `${part.kind}:${part.value}:${part.href ?? ""}`,
          seen,
        ),
      });
    };
    const re = /(\*\*[^*]+\*\*|\[[^\]]+\]\([^)]+\))/g;
    let last = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text)) !== null) {
      if (m.index > last) {
        push({ kind: "text", value: text.slice(last, m.index) });
      }
      const token = m[0];
      if (token.startsWith("**") && token.endsWith("**")) {
        push({ kind: "bold", value: token.slice(2, -2) });
      } else {
        const link = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(token);
        if (link && isSafeUrl(link[2])) {
          push({ kind: "link", value: link[1], href: link[2] });
        } else {
          push({ kind: "text", value: token });
        }
      }
      last = m.index + token.length;
    }
    if (last < text.length) {
      push({ kind: "text", value: text.slice(last) });
    }
    return out;
  }, [text]);

  return (
    <>
      {parts.map((part) => {
        if (part.kind === "bold") {
          return <strong key={part.key}>{part.value}</strong>;
        }
        if (part.kind === "link") {
          return (
            <a
              key={part.key}
              href={part.href}
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => {
                // Let the Tauri opener handle external links from the
                // webview without navigating the app away.
                if (!isTauri()) return;
                e.preventDefault();
                void openUrl(part.href ?? "");
              }}
            >
              {part.value}
            </a>
          );
        }
        return <span key={part.key}>{part.value}</span>;
      })}
    </>
  );
}

function RenderNotes({ blocks }: { blocks: ReturnType<typeof parseReleaseNotes> }) {
  const out: React.ReactNode[] = [];
  let bulletGroup: React.ReactNode[] = [];
  const seen = new Map<string, number>();
  const flush = (keyBase: string) => {
    if (!bulletGroup.length) return;
    out.push(
      <ul key={`${keyBase}-ul`} className="update-dialog__list">
        {bulletGroup}
      </ul>,
    );
    bulletGroup = [];
  };

  for (const block of blocks) {
    const key = uniquePartKey(`${block.type}:${block.text}`, seen);
    if (block.type === "bullet") {
      bulletGroup.push(
        <li key={key} className="update-dialog__bullet">
          <InlineText text={block.text} />
        </li>,
      );
      continue;
    }
    flush(key);
    if (block.type === "heading") {
      out.push(
        <h3 key={key} className="update-dialog__heading">
          <InlineText text={block.text} />
        </h3>,
      );
    } else if (block.type === "subheading") {
      out.push(
        <h4 key={key} className="update-dialog__subheading">
          <InlineText text={block.text} />
        </h4>,
      );
    } else {
      out.push(
        <p key={key} className="update-dialog__paragraph">
          <InlineText text={block.text} />
        </p>,
      );
    }
  }
  flush("tail");
  return <div className="update-dialog__notes">{out}</div>;
}

/**
 * Modal shown when an update is available. Renders the release notes
 * (changelog) for the new version with "Install now" / "Cancel" actions.
 * Clicking the backdrop (outside the panel) cancels, like a native dialog.
 */
export function UpdateDialog({
  update,
  onCancel,
}: UpdateDialogProps) {
  const { t } = useTranslation("common");
  const [phase, setPhase] = useState<Phase>("available");
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const blocks = useMemo(() => parseReleaseNotes(update.body), [update.body]);
  const version = formatVersion(update.version);

  // Esc closes, matching "click outside closes".
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && phase === "available") {
        onCancel();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [phase, onCancel]);

  const install = async () => {
    if (phase !== "available" && phase !== "error") return;
    setPhase("downloading");
    setProgress(0);
    setError(null);
    try {
      let total = 0;
      let downloaded = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started" && event.data.contentLength) {
          total = event.data.contentLength;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (total > 0) {
            setProgress(Math.min(99, Math.round((downloaded / total) * 100)));
          }
        } else if (event.event === "Finished") {
          setPhase("installing");
        }
      });
      setPhase("installing");
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch (err) {
      setPhase("error");
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const close = () => {
    if (phase === "available" || phase === "error") onCancel();
  };

  return (
    <dialog
      className="update-dialog"
      open
      aria-labelledby="update-dialog-title"
      onCancel={(e) => {
        e.preventDefault();
        close();
      }}
    >
      <button
        type="button"
        className="update-dialog__dismiss"
        aria-label={t("cancel")}
        tabIndex={-1}
        onClick={close}
      />
      <div className="update-dialog__panel">
        <header className="update-dialog__header">
          <div>
            <h2 id="update-dialog-title">{t("updateDialogTitle")}</h2>
            <p className="muted">
              {t("updateDialogSubtitle", { version })}
            </p>
          </div>
        </header>

        <div className="update-dialog__body">
          {phase === "downloading" || phase === "installing" ? (
            <div className="update-dialog__status">
              <strong>
                {phase === "downloading"
                  ? t("updateDownloading", { progress })
                  : t("updateInstalling")}
              </strong>
              {phase === "downloading" ? (
                <span className="update-dialog__bar">
                  <span
                    className="update-dialog__fill"
                    style={{ width: `${progress}%` }}
                  />
                </span>
              ) : null}
            </div>
          ) : null}

          {phase === "error" ? (
            <p className="update-dialog__error">
              {t("updateFailed")}
              {error ? <span> — {error}</span> : null}
            </p>
          ) : null}

          {blocks.length ? (
            <RenderNotes blocks={blocks} />
          ) : (
            <p className="muted">{t("updateDialogNoNotes")}</p>
          )}
        </div>

        <footer className="update-dialog__footer">
          <button
            type="button"
            className="button-secondary"
            onClick={close}
            disabled={phase === "downloading" || phase === "installing"}
          >
            {t("cancel")}
          </button>
          {phase === "available" ? (
            <button type="button" onClick={() => void install()}>
              {t("updateNow")}
            </button>
          ) : phase === "error" ? (
            <button type="button" onClick={() => void install()}>
              {t("retry")}
            </button>
          ) : null}
        </footer>
      </div>
    </dialog>
  );
}
