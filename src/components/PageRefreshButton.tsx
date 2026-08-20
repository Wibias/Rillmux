import { useTranslation } from "react-i18next";
import { RefreshIcon } from "./FollowedIcons";

type PageRefreshButtonProps = {
  onRefresh: () => void;
  refreshing?: boolean;
  disabled?: boolean;
  iconOnly?: boolean;
};

/** Header action to re-fetch the current browse list. */
export function PageRefreshButton({
  onRefresh,
  refreshing = false,
  disabled = false,
  iconOnly = false,
}: PageRefreshButtonProps) {
  const { t } = useTranslation("common");
  return (
    <button
      type="button"
      className={
        iconOnly
          ? "button-secondary page__refresh page__refresh--icon"
          : "button-secondary page__refresh"
      }
      disabled={disabled || refreshing}
      onClick={onRefresh}
      aria-busy={refreshing || undefined}
      aria-label={t("refresh")}
      title={t("refresh")}
    >
      <RefreshIcon />
      {iconOnly ? null : refreshing ? t("loading") : t("refresh")}
    </button>
  );
}
