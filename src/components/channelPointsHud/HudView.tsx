import { POINTS_HUD_CHIP_HEIGHT, cssPx, type OverlayRect } from "../../lib/streaming/pointsHud";
import type { useChannelPointsHudModel } from "./model";

export function ChannelPointsHudView({
  channel,
  showLogin,
  chip,
  overlay,
  panel,
  scale,
  dragChip,
  snapshot,
  catalogOpen,
  geometryConcealed,
  hostHidden,
  error,
  inputFor,
  inputText,
  setInputText,
  setInputFor,
  redeeming,
  flashBonus,
  chipLocal,
  panelLocal,
  rewards,
  redeem,
  onChipPointerDown,
  onChipPointerMove,
  onChipPointerUp,
  onChipPointerCancel,
  t,
}: ReturnType<typeof useChannelPointsHudModel> & {
  channel: string;
  chip: OverlayRect;
  overlay: OverlayRect;
}) {
  return (
    <div
      className={catalogOpen ? "points-hud points-hud--open" : "points-hud"}
      style={{
        width: cssPx(overlay.width, scale),
        height: cssPx(overlay.height, scale),
        visibility: geometryConcealed || hostHidden ? "hidden" : "visible",
      }}
    >
      <button
        type="button"
        className={
          dragChip
            ? "points-hud__chip points-hud__chip--dragging"
            : "points-hud__chip"
        }
        style={{
          left: cssPx(chipLocal.x, scale),
          top: cssPx(chipLocal.y, scale),
          width: cssPx(chip.width, scale),
          height: cssPx(POINTS_HUD_CHIP_HEIGHT, scale),
        }}
        onPointerDown={onChipPointerDown}
        onPointerMove={onChipPointerMove}
        onPointerUp={onChipPointerUp}
        onPointerCancel={onChipPointerCancel}
      >
        <span className="points-hud__glyph" aria-hidden>
          ◆
        </span>
        <span className="points-hud__balance">
          {(snapshot?.balance ?? 0).toLocaleString()}
        </span>
        {showLogin ? (
          <span className="points-hud__login">{channel}</span>
        ) : null}
        {flashBonus ? <span className="points-hud__flash">+50</span> : null}
      </button>
      {panel ? (
        <div
          className="points-hud__catalog"
          style={{
            left: cssPx(panelLocal.x, scale),
            top: cssPx(panelLocal.y, scale),
            width: cssPx(panel.width, scale),
            height: cssPx(panel.height, scale),
          }}
        >
          <p className="points-hud__catalog-title">
            {t("pointsHudCatalogTitle", { channel })}
          </p>
          {error ? (
            <p className="points-hud__error" role="alert">
              {error}
            </p>
          ) : null}
          {rewards.length === 0 ? (
            <p className="muted points-hud__empty">{t("pointsHudEmpty")}</p>
          ) : (
            <ul className="points-hud__list">
              {rewards.map((reward) => (
                <li key={reward.id}>
                  <button
                    type="button"
                    className="points-hud__reward"
                    disabled={!reward.redeemable || redeeming === reward.id}
                    onClick={() => {
                      const prompt = reward.prompt?.trim();
                      if (reward.isUserInputRequired || prompt) {
                        setInputFor(reward.id);
                        setInputText("");
                        return;
                      }
                      void redeem(reward);
                    }}
                  >
                    {reward.imageUrl ? (
                      <img src={reward.imageUrl} alt="" />
                    ) : (
                      <span className="points-hud__glyph" aria-hidden>
                        ◆
                      </span>
                    )}
                    <span className="points-hud__reward-text">
                      <strong>{reward.title}</strong>
                      <small>
                        {reward.redeemable
                          ? t("pointsHudCost", { cost: reward.cost })
                          : reward.reason === "paused"
                            ? t("pointsHudPaused")
                            : reward.reason === "disabled"
                              ? t("pointsHudDisabled")
                              : reward.reason === "outOfStock"
                                ? t("pointsHudOutOfStock")
                                : reward.reason === "cooldown"
                                  ? t("pointsHudCooldown", {
                                      seconds: reward.cooldownSeconds,
                                    })
                                  : t("pointsHudNotEnough")}
                      </small>
                    </span>
                    <span className="points-hud__cost">
                      {reward.cost.toLocaleString()}
                    </span>
                  </button>
                  {inputFor === reward.id ? (
                    <form
                      className="points-hud__redeem"
                      onSubmit={(event) => {
                        event.preventDefault();
                        if (reward.isUserInputRequired && !inputText.trim()) return;
                        void redeem(
                          reward,
                          reward.isUserInputRequired
                            ? inputText.trim()
                            : undefined,
                        );
                      }}
                    >
                      {reward.prompt?.trim() ? (
                        <p className="points-hud__prompt">{reward.prompt.trim()}</p>
                      ) : null}
                      <div className="points-hud__input-row">
                        {reward.isUserInputRequired ? (
                          <input
                            value={inputText}
                            onChange={(event) => setInputText(event.target.value)}
                            placeholder={
                              reward.prompt?.trim() ||
                              t("pointsHudInputPlaceholder")
                            }
                            // Focus the reward prompt after the user selects it.
                            // react-doctor-disable-next-line react-doctor/no-autofocus
                            autoFocus
                          />
                        ) : null}
                        <button
                          type="submit"
                          disabled={
                            reward.isUserInputRequired && !inputText.trim()
                          }
                        >
                          {t("pointsHudRedeem")}
                        </button>
                      </div>
                    </form>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
        </div>
      ) : null}
    </div>
  );
}

