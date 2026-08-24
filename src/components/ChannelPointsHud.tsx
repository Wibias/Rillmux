import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { PhysicalPosition, PhysicalSize } from "@tauri-apps/api/dpi";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettingsStore } from "../lib/settings/store";
import { overlayRectMoved } from "../lib/streaming/pollOverlay";
import {
  POINTS_HUD_CATALOG_MAX_HEIGHT,
  POINTS_HUD_CATALOG_MAX_WIDTH,
  POINTS_HUD_CHIP_HEIGHT,
  POINTS_HUD_CHIP_MIN_WIDTH,
  POINTS_HUD_MOVE_SLOP,
  POINTS_HUD_OFFSET_EVENT,
  catalogRectForChip,
  chipOriginInOverlay,
  chipRectForPlayer,
  chipRectFromDrag,
  cssPx,
  hudDragSurfaceRect,
  isPointsHudOverlay,
  movementIsDrag,
  offsetFromChipRect,
  overlayRectForHud,
  physicalDeltaFromScreen,
  pointsHudChannelFromSearch,
  rewardUnavailableReason,
  sortCustomRewards,
  type ChannelPointsHudPlace,
  type OverlayRect,
} from "../lib/streaming/pointsHud";
import { invoke, isTauri } from "../lib/tauri";
import "./ChannelPointsHud.css";

export interface ChannelPointsReward {
  id: string;
  title: string;
  cost: number;
  imageUrl?: string | null;
  isPaused: boolean;
  inStock: boolean;
  isEnabled: boolean;
  isUserInputRequired: boolean;
  cooldownSeconds: number;
}

export interface ChannelPointsHudSnapshot {
  channelLogin: string;
  balance: number;
  bonusAvailable: boolean;
  bonusClaimed: boolean;
  rewards?: ChannelPointsReward[];
}

export function isPointsHudOverlayWindow() {
  return isPointsHudOverlay(window.location.search);
}

let overlayApplyChain: Promise<void> = Promise.resolve();
let overlayApplyLatest: OverlayRect | null = null;
let overlayApplied: OverlayRect | null = null;

function applyOverlayRect(rect: OverlayRect) {
  overlayApplyLatest = rect;
  overlayApplyChain = overlayApplyChain
    .then(flushOverlayRect)
    .catch(() => undefined);
}

async function flushOverlayRect() {
  while (overlayApplyLatest && isTauri()) {
    const rect = overlayApplyLatest;
    overlayApplyLatest = null;
    const win = getCurrentWindow();
    const position = new PhysicalPosition(
      Math.round(rect.x),
      Math.round(rect.y),
    );
    const width = Math.max(1, Math.round(rect.width));
    const height = Math.max(1, Math.round(rect.height));
    const size = new PhysicalSize(width, height);
    const sizeChanged =
      !overlayApplied ||
      Math.round(overlayApplied.width) !== width ||
      Math.round(overlayApplied.height) !== height;
    overlayApplied = rect;
    await invoke("overlay_place_hud", {
      label: win.label,
      x: Math.round(rect.x),
      y: Math.round(rect.y),
      width,
      height,
      force: true,
    }).catch(() => undefined);
    await win.setPosition(position).catch(() => undefined);
    if (!sizeChanged) continue;
    await win.setSize(size).catch(() => undefined);
    await invoke("overlay_fit_webview", { width, height }).catch(
      () => undefined,
    );
    // Transparent WebView2 often keeps the old child size on the first resize.
    await new Promise<void>((resolve) => {
      requestAnimationFrame(() => resolve());
    });
    await invoke("overlay_fit_webview", { width, height }).catch(
      () => undefined,
    );
    await win.setSize(size).catch(() => undefined);
  }
}

export function ChannelPointsHud() {
  const { t } = useTranslation("common");
  const channel = pointsHudChannelFromSearch(window.location.search);
  const offset = useSettingsStore(
    (state) => state.settings.streaming.channelPointsHudOffset,
  );
  const setSettings = useSettingsStore((state) => state.setSettings);
  const showLogin =
    new URLSearchParams(window.location.search).get("showLogin") === "1";

  const [host, setHost] = useState<OverlayRect | null>(null);
  const [captionAvoid, setCaptionAvoid] = useState<OverlayRect | null>(null);
  const [scale, setScale] = useState(window.devicePixelRatio || 1);
  const [dragChip, setDragChip] = useState<OverlayRect | null>(null);
  const [snapshot, setSnapshot] = useState<ChannelPointsHudSnapshot | null>(
    null,
  );
  const [catalogOpen, setCatalogOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [inputFor, setInputFor] = useState<string | null>(null);
  const [inputText, setInputText] = useState("");
  const [redeeming, setRedeeming] = useState<string | null>(null);
  const [flashBonus, setFlashBonus] = useState(false);
  const dragRef = useRef<{
    pointerId: number;
    startScreenX: number;
    startScreenY: number;
    origin: OverlayRect;
    dragged: boolean;
  } | null>(null);
  const draggingRef = useRef(false);
  const pendingDragChipRef = useRef<OverlayRect | null>(null);
  const dragChipRafRef = useRef<number | null>(null);
  const lastOverlayRef = useRef<OverlayRect | null>(null);
  const lastClaimedRef = useRef(false);

  const chip = useMemo(() => {
    if (dragChip) return dragChip;
    if (!host) return null;
    return chipRectForPlayer(
      host,
      offset,
      POINTS_HUD_CHIP_MIN_WIDTH,
      captionAvoid,
    );
  }, [dragChip, host, offset, captionAvoid]);

  const panel = useMemo(() => {
    if (!host || !chip || !catalogOpen) return null;
    return catalogRectForChip(
      host,
      chip,
      POINTS_HUD_CATALOG_MAX_WIDTH,
      POINTS_HUD_CATALOG_MAX_HEIGHT,
    );
  }, [host, chip, catalogOpen]);

  const dragSurface = useMemo(() => {
    if (!dragChip || !host) return null;
    return hudDragSurfaceRect(host, captionAvoid, {
      width: dragChip.width,
      height: dragChip.height,
    });
  }, [Boolean(dragChip), host, captionAvoid, dragChip?.width, dragChip?.height]);

  const overlay = useMemo(() => {
    if (dragSurface) return dragSurface;
    if (!chip) return null;
    return overlayRectForHud(chip, panel);
  }, [dragSurface, chip, panel]);

  const chipLocal =
    overlay && chip ? chipOriginInOverlay(overlay, chip) : { x: 0, y: 0 };
  const panelLocal =
    overlay && panel ? chipOriginInOverlay(overlay, panel) : { x: 0, y: 0 };

  useEffect(() => {
    if (!overlay) return;
    if (
      lastOverlayRef.current &&
      !overlayRectMoved(
        lastOverlayRef.current,
        overlay,
        POINTS_HUD_MOVE_SLOP,
      )
    ) {
      return;
    }
    lastOverlayRef.current = overlay;
    applyOverlayRect(overlay);
  }, [overlay]);

  const interactive = chip != null;
  useEffect(() => {
    if (!isTauri()) return;
    void getCurrentWindow()
      .setIgnoreCursorEvents(!interactive)
      .catch(() => undefined);
  }, [interactive]);

  useEffect(() => {
    if (!channel || !isTauri()) return;
    let active = true;
    const tick = async () => {
      if (draggingRef.current) return;
      const next = await invoke<ChannelPointsHudPlace | null>(
        "channel_points_hud_place",
        { channelLogin: channel },
      ).catch(() => null);
      const nextScale = await getCurrentWindow()
        .scaleFactor()
        .catch(() => window.devicePixelRatio || 1);
      if (!active || draggingRef.current) return;
      setScale(nextScale);
      setHost(next?.player ?? null);
      setCaptionAvoid(next?.captionAvoid ?? null);
      if (!next?.player) setCatalogOpen(false);
    };
    void tick();
    const timer = window.setInterval(() => void tick(), 250);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [channel]);

  useEffect(() => {
    if (!channel || !isTauri()) return;
    let active = true;
    const load = async (useCache: boolean) => {
      try {
        let next = useCache
          ? await invoke<ChannelPointsHudSnapshot | null>(
              "channel_points_cached",
              { channelLogin: channel },
            )
          : await invoke<ChannelPointsHudSnapshot>("channel_points_refresh", {
              channelLogin: channel,
              includePoll: false,
            });
        if (!next && useCache) {
          next = await invoke<ChannelPointsHudSnapshot>(
            "channel_points_refresh",
            { channelLogin: channel, includePoll: false },
          );
        }
        if (!active || !next) return;
        if (next.bonusClaimed && !lastClaimedRef.current) {
          setFlashBonus(true);
          window.setTimeout(() => setFlashBonus(false), 2000);
        }
        lastClaimedRef.current = next.bonusClaimed;
        setSnapshot(next);
        setError(null);
      } catch (reason) {
        if (!active) return;
        setError(reason instanceof Error ? reason.message : String(reason));
      }
    };
    void load(true);
    const timer = window.setInterval(() => void load(false), 15_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [channel]);

  const persistChip = useCallback(
    (chipRect: OverlayRect, player: OverlayRect) => {
      const streaming = useSettingsStore.getState().settings.streaming;
      const nextOffset = offsetFromChipRect(player, chipRect, captionAvoid);
      setSettings({
        streaming: {
          ...streaming,
          channelPointsHudOffset: nextOffset,
        },
      });
      if (isTauri()) {
        void emit(POINTS_HUD_OFFSET_EVENT, nextOffset);
      }
    },
    [setSettings, captionAvoid],
  );

  function cancelDragChipRaf() {
    if (dragChipRafRef.current != null) {
      window.cancelAnimationFrame(dragChipRafRef.current);
      dragChipRafRef.current = null;
    }
    pendingDragChipRef.current = null;
  }

  function endChipDrag() {
    draggingRef.current = false;
    cancelDragChipRaf();
    dragRef.current = null;
  }

  function onChipPointerDown(event: React.PointerEvent<HTMLButtonElement>) {
    if (!chip || event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = {
      pointerId: event.pointerId,
      startScreenX: event.screenX,
      startScreenY: event.screenY,
      origin: chip,
      dragged: false,
    };
  }

  function onChipPointerMove(event: React.PointerEvent<HTMLButtonElement>) {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId || !host) return;
    if (!event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.setPointerCapture(event.pointerId);
    }
    const cssDelta = physicalDeltaFromScreen(
      drag.startScreenX,
      drag.startScreenY,
      event.screenX,
      event.screenY,
      1,
    );
    if (!drag.dragged && !movementIsDrag(cssDelta.dx, cssDelta.dy)) return;
    const physical = physicalDeltaFromScreen(
      drag.startScreenX,
      drag.startScreenY,
      event.screenX,
      event.screenY,
      window.devicePixelRatio || 1,
    );
    const next = chipRectFromDrag(
      host,
      drag.origin,
      physical.dx,
      physical.dy,
      captionAvoid,
    );
    pendingDragChipRef.current = next;
    if (!drag.dragged) {
      drag.dragged = true;
      draggingRef.current = true;
      setCatalogOpen(false);
      setInputFor(null);
      setDragChip(next);
      return;
    }
    if (dragChipRafRef.current != null) return;
    dragChipRafRef.current = window.requestAnimationFrame(() => {
      dragChipRafRef.current = null;
      const pending = pendingDragChipRef.current;
      if (pending) setDragChip(pending);
    });
  }

  function onChipPointerUp(event: React.PointerEvent<HTMLButtonElement>) {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const dropped = pendingDragChipRef.current ?? dragChip;
    const dragged = drag.dragged;
    endChipDrag();
    if (dragged && dropped && host) {
      persistChip(dropped, host);
      setDragChip(null);
      return;
    }
    setDragChip(null);
    setCatalogOpen((open) => !open);
    setInputFor(null);
    setError(null);
  }

  function onChipPointerCancel(event: React.PointerEvent<HTMLButtonElement>) {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    endChipDrag();
    setDragChip(null);
  }

  const rewards = useMemo(() => {
    const list = (snapshot?.rewards ?? []).map((reward) => {
      const reason = rewardUnavailableReason({
        paused: reward.isPaused,
        enabled: reward.isEnabled,
        inStock: reward.inStock,
        cooldownSeconds: reward.cooldownSeconds,
        cost: reward.cost,
        balance: snapshot?.balance ?? 0,
      });
      return { ...reward, redeemable: reason === null, reason };
    });
    return sortCustomRewards(list);
  }, [snapshot]);

  async function redeem(reward: ChannelPointsReward, text?: string) {
    if (!channel) return;
    setRedeeming(reward.id);
    setError(null);
    try {
      const next = await invoke<ChannelPointsHudSnapshot>(
        "channel_points_redeem_reward",
        {
          channelLogin: channel,
          rewardId: reward.id,
          text: text || null,
        },
      );
      setSnapshot(next);
      setInputFor(null);
      setInputText("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setRedeeming(null);
    }
  }

  if (!channel || !chip || !overlay || !host) {
    return <div className="points-hud points-hud--empty" />;
  }

  return (
    <div
      className={catalogOpen ? "points-hud points-hud--open" : "points-hud"}
      style={{
        width: cssPx(overlay.width, scale),
        height: cssPx(overlay.height, scale),
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
        {showLogin ? <span className="points-hud__login">{channel}</span> : null}
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
                      if (reward.isUserInputRequired) {
                        setInputFor(reward.id);
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
                      className="points-hud__input-row"
                      onSubmit={(event) => {
                        event.preventDefault();
                        if (!inputText.trim()) return;
                        void redeem(reward, inputText.trim());
                      }}
                    >
                      <input
                        value={inputText}
                        onChange={(event) => setInputText(event.target.value)}
                        placeholder={t("pointsHudInputPlaceholder")}
                        autoFocus
                      />
                      <button type="submit" disabled={!inputText.trim()}>
                        {t("pointsHudRedeem")}
                      </button>
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
