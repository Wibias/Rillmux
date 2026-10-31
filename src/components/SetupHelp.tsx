import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import {
  CHATTERINO_INSTALL_URL,
  CHATTERINO_CHOCO,
  CHATTERINO_WINGET,
  MPV_INSTALL_URL,
  MPV_PORTABLE_URL,
  MPV_SCOOP,
  MPV_WINGET,
  STREAMLINK_INSTALL_URL,
  STREAMLINK_SCOOP,
  STREAMLINK_WINGET,
  type ToolStatus,
} from "../lib/doctor";
import { isTauri } from "../lib/tauri";
import "./SetupHelp.css";

interface SetupHelpProps {
  status?: ToolStatus | null;
  onRecheck?: () => void;
  checking?: boolean;
}

interface SetupCommand {
  label: string;
  command: string;
}

interface SetupAction {
  label: string;
  url: string;
}

interface ToolSetupViewProps extends SetupHelpProps {
  foundText: string;
  foundPathText?: string | null;
  missingTitle: string;
  bodyBeforeCommands: string[];
  commands: SetupCommand[];
  bodyAfterCommands?: string[];
  actions: SetupAction[];
}

async function openExternal(url: string) {
  if (isTauri()) {
    await openUrl(url);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

function RecheckButton({
  onRecheck,
  checking,
  secondary = false,
}: {
  onRecheck: () => void;
  checking: boolean;
  secondary?: boolean;
}) {
  const { t } = useTranslation("onboarding");
  return (
    <button
      type="button"
      className={`${secondary ? "button-secondary " : ""}button-with-spinner`}
      onClick={onRecheck}
      disabled={checking}
    >
      {checking ? <span className="spinner" aria-hidden /> : null}
      {checking ? t("checking") : t("recheck")}
    </button>
  );
}

function RecheckActions({
  found,
  onRecheck,
  checking,
}: {
  found: boolean;
  onRecheck?: () => void;
  checking: boolean;
}) {
  if (!onRecheck) return null;

  const className = found
    ? "setup-help__actions"
    : "setup-help__actions setup-help__actions--recheck";

  return (
    <div className={className}>
      <RecheckButton
        onRecheck={onRecheck}
        checking={checking}
        secondary={found}
      />
    </div>
  );
}

function FoundTool({ text, pathText }: { text: string; pathText?: string | null }) {
  return (
    <>
      <p className="setup-help__status">{text}</p>
      {pathText ? <p className="setup-help__path muted">{pathText}</p> : null}
    </>
  );
}

function SetupCommands({ commands }: { commands: SetupCommand[] }) {
  return (
    <div className="setup-help__cmds">
      {commands.map((item) => (
        <div key={item.command}>
          <span className="muted">{item.label}</span>
          <code>{item.command}</code>
        </div>
      ))}
    </div>
  );
}

function SetupActions({ actions }: { actions: SetupAction[] }) {
  return (
    <div className="setup-help__actions">
      {actions.map((action) => (
        <button
          key={action.url}
          type="button"
          className="button-secondary"
          onClick={() => void openExternal(action.url)}
        >
          {action.label}
        </button>
      ))}
    </div>
  );
}

function MissingTool({
  title,
  bodyBeforeCommands,
  commands,
  bodyAfterCommands = [],
  actions,
}: {
  title: string;
  bodyBeforeCommands: string[];
  commands: SetupCommand[];
  bodyAfterCommands?: string[];
  actions: SetupAction[];
}) {
  return (
    <>
      <p className="setup-help__title">{title}</p>
      {bodyBeforeCommands.map((text) => (
        <p key={text} className="setup-help__body muted">
          {text}
        </p>
      ))}
      <SetupCommands commands={commands} />
      {bodyAfterCommands.map((text) => (
        <p key={text} className="setup-help__body muted">
          {text}
        </p>
      ))}
      <SetupActions actions={actions} />
    </>
  );
}

function toolSetupHelpView({
  status,
  onRecheck,
  checking = false,
  foundText,
  foundPathText,
  missingTitle,
  bodyBeforeCommands,
  commands,
  bodyAfterCommands,
  actions,
}: ToolSetupViewProps) {
  const found = Boolean(status?.found);
  return (
    <div className={`setup-help${found ? " setup-help--ok" : ""}`}>
      {found ? (
        <FoundTool text={foundText} pathText={foundPathText} />
      ) : (
        <MissingTool
          title={missingTitle}
          bodyBeforeCommands={bodyBeforeCommands}
          commands={commands}
          bodyAfterCommands={bodyAfterCommands}
          actions={actions}
        />
      )}
      <RecheckActions found={found} onRecheck={onRecheck} checking={checking} />
    </div>
  );
}

export function StreamlinkSetupHelp({
  status,
  onRecheck,
  checking = false,
}: SetupHelpProps) {
  const { t } = useTranslation("onboarding");
  return toolSetupHelpView({
    status,
    onRecheck,
    checking,
    foundText: t("streamlinkFound", { version: status?.version ?? "?" }),
    foundPathText: status?.path
      ? t("streamlinkFoundPath", { path: status.path })
      : null,
    missingTitle: t("streamlinkMissingTitle"),
    bodyBeforeCommands: [t("streamlinkMissingBody")],
    commands: [
      { label: t("streamlinkWinget"), command: STREAMLINK_WINGET },
      { label: t("streamlinkScoop"), command: STREAMLINK_SCOOP },
    ],
    actions: [{ label: t("streamlinkDocs"), url: STREAMLINK_INSTALL_URL }],
  });
}

export function PlayerSetupHelp({
  status,
  onRecheck,
  checking = false,
}: SetupHelpProps) {
  const { t } = useTranslation("onboarding");
  return toolSetupHelpView({
    status,
    onRecheck,
    checking,
    foundText: t("playerFound"),
    foundPathText: status?.path ? t("playerFoundPath", { path: status.path }) : null,
    missingTitle: t("playerMissingTitle"),
    bodyBeforeCommands: [t("playerMissingBody"), t("playerOpenShell")],
    commands: [
      { label: t("playerWinget"), command: MPV_WINGET },
      { label: t("playerScoop"), command: MPV_SCOOP },
    ],
    bodyAfterCommands: [t("playerPortableBody")],
    actions: [
      { label: t("playerDocs"), url: MPV_INSTALL_URL },
      { label: t("playerPortable"), url: MPV_PORTABLE_URL },
    ],
  });
}

export function ChatterinoSetupHelp({
  status,
  onRecheck,
  checking = false,
}: SetupHelpProps) {
  const { t } = useTranslation("onboarding");
  return toolSetupHelpView({
    status,
    onRecheck,
    checking,
    foundText: t("chatterinoFound"),
    foundPathText: status?.path
      ? t("chatterinoFoundPath", { path: status.path })
      : null,
    missingTitle: t("chatterinoMissingTitle"),
    bodyBeforeCommands: [t("chatterinoMissingBody")],
    commands: [
      { label: t("chatterinoWinget"), command: CHATTERINO_WINGET },
      { label: t("chatterinoChoco"), command: CHATTERINO_CHOCO },
    ],
    actions: [{ label: t("chatterinoDocs"), url: CHATTERINO_INSTALL_URL }],
  });
}
