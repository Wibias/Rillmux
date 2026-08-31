import { useTranslation } from "react-i18next";
import "./EmbeddedChat.css";

function twitchChatFrame(src: string, title: string) {
  // Twitch's embed requires scripts + same-origin; dropping either breaks chat.
  return (
    // react-doctor-disable-next-line react-doctor/iframe-missing-sandbox
    <iframe
      className="embedded-chat__frame"
      title={title}
      src={src}
      allow="clipboard-write"
      sandbox="allow-scripts allow-same-origin allow-popups allow-popups-to-escape-sandbox allow-forms"
      referrerPolicy="no-referrer"
    />
  );
}

export function EmbeddedChat({ channel }: { channel: string | null }) {
  const { t } = useTranslation("routes");

  if (!channel) {
    return (
      <aside className="embedded-chat embedded-chat--empty">
        <p className="muted">{t("chatEmpty")}</p>
      </aside>
    );
  }

  const src = `https://www.twitch.tv/embed/${encodeURIComponent(channel)}/chat?parent=localhost&parent=tauri.localhost&parent=127.0.0.1&darkpopout`;

  return (
    <aside className="embedded-chat">
      <header className="embedded-chat__header">
        {t("chatTitle", { channel })}
      </header>
      {twitchChatFrame(src, t("chatTitle", { channel }))}
    </aside>
  );
}
