// Placement and re-placement policy predicates.

pub fn overlay_rects_overlap(a: OverlayRect, b: OverlayRect) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

fn overlay_rect_differs(a: OverlayRect, b: OverlayRect, slop: i32) -> bool {
    (a.x - b.x).abs() > slop
        || (a.y - b.y).abs() > slop
        || (a.width - b.width).abs() > slop
        || (a.height - b.height).abs() > slop
}

/// Move the HUD when forced, when the chip jumped (monitor switch), or when
/// it still covers the caption keepout. Do not skip a monitor jump just
/// because the chip is not overlapping the (new) caption strip.
pub fn hud_overlay_should_apply(
    force: bool,
    current: Option<OverlayRect>,
    desired: OverlayRect,
    keepout: Option<OverlayRect>,
    slop: i32,
) -> bool {
    if force {
        return true;
    }
    let Some(current) = current else {
        return true;
    };
    if overlay_rect_differs(current, desired, slop) {
        return true;
    }
    keepout.is_some_and(|k| overlay_rects_overlap(current, k))
}

/// Channel login from a `points-hud-<login>` webview label.
pub fn points_hud_channel_from_label(label: &str) -> Option<&str> {
    let channel = label.strip_prefix("points-hud-")?;
    if channel.is_empty() {
        None
    } else {
        Some(channel)
    }
}

/// `SetWindowPos` insert-after so the HUD sits immediately above the player.
/// `player_prev` is `GW_HWNDPREV` of the player (0 if none). `None` means the
/// HUD is already there — do not raise it over other apps.
pub fn hud_z_insert_after(hud: isize, player_prev: isize) -> Option<isize> {
    if player_prev == 0 {
        Some(0)
    } else if player_prev == hud {
        None
    } else {
        Some(player_prev)
    }
}

/// Tauri owns overlay webviews by the creator (main). Clearing
/// GWLP_HWNDPARENT orphans the chip. Never re-own it to mpv either — that
/// made the Rillmux main window impossible to activate while a stream ran.
#[cfg(test)]
pub fn hud_needs_detach_owner(_owner: isize) -> bool {
    false
}

#[cfg(test)]
pub fn hud_needs_reown(_current_owner: isize, _player: isize) -> bool {
    false
}

/// Qt restores window-layout.json after a monitor change, often after 800ms.
pub fn chatterino_place_retry_ms() -> &'static [u64] {
    &[0, 80, 200, 400, 800, 1600, 3000]
}

/// Visible-frame slop. 64px hid the ~27px left-gap after DWM expand.
pub fn chatterino_place_slop_px() -> i32 {
    12
}

/// Keep retrying place when the dock still has a Chatterino process but no HWND
/// (Qt recreates the window on monitor switch and drops the SetProp tag).
pub fn chatterino_hwnd_lost_needs_retry(found_hwnd: bool, have_dock_pid: bool) -> bool {
    !found_hwnd && have_dock_pid
}

/// Watchdog / place retry: missing HWND, unreadable frame, or visible drift.
pub fn chatterino_watchdog_should_place(
    found_hwnd: bool,
    expect_chat: bool,
    visible: Option<OverlayRect>,
    target: OverlayRect,
    slop: i32,
) -> bool {
    if chatterino_hwnd_lost_needs_retry(found_hwnd, expect_chat) {
        return true;
    }
    match visible {
        Some(got) => overlay_rect_drifted(got, target, slop),
        None => found_hwnd,
    }
}

pub fn overlay_rect_drifted(current: OverlayRect, target: OverlayRect, slop: i32) -> bool {
    (current.x - target.x).abs() > slop
        || (current.y - target.y).abs() > slop
        || (current.width - target.width).abs() > slop
        || (current.height - target.height).abs() > slop
}
