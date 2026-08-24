//! Z-order policy for Chatterino popups vs dock grips / player windows.

/// Whether a Chatterino top-level window that is not the main chat should be
/// treated as a usercard/menu overlay. Size cuts out tooltips.
#[cfg(test)]
pub fn chatterino_extra_window_is_overlay(
    visible: bool,
    is_main: bool,
    width: i32,
    height: i32,
) -> bool {
    visible && !is_main && width >= 48 && height >= 48
}

/// `chatterino.exe` regardless of install path.
pub fn image_path_looks_like_chatterino(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = name
        .trim_end_matches(".exe")
        .trim_end_matches(".EXE")
        .trim_end_matches(".Exe");
    stem.eq_ignore_ascii_case("chatterino")
}

/// Restack is fine if overlays are raised after grips.
#[cfg(test)]
pub fn dock_should_restack_players_for_overlay(_overlay_open: bool) -> bool {
    true
}

/// Raise Chatterino usercards/menus above dock grips instead of hiding them.
#[cfg(test)]
pub fn dock_should_raise_overlay_above_dock(overlay_open: bool) -> bool {
    overlay_open
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usercard_sized_owned_window_is_an_overlay() {
        assert!(chatterino_extra_window_is_overlay(true, false, 360, 420));
    }

    #[test]
    fn main_chat_is_not_an_overlay() {
        assert!(!chatterino_extra_window_is_overlay(true, true, 400, 800));
    }

    #[test]
    fn tiny_tooltip_is_not_an_overlay() {
        assert!(!chatterino_extra_window_is_overlay(true, false, 40, 20));
    }

    #[test]
    fn compact_usercard_is_an_overlay() {
        assert!(chatterino_extra_window_is_overlay(true, false, 80, 80));
    }

    #[test]
    fn hidden_window_is_not_an_overlay() {
        assert!(!chatterino_extra_window_is_overlay(false, false, 360, 420));
    }

    #[test]
    fn overlay_raises_above_dock_without_burying_players() {
        assert!(dock_should_restack_players_for_overlay(true));
        assert!(dock_should_raise_overlay_above_dock(true));
        assert!(!dock_should_raise_overlay_above_dock(false));
    }

    #[test]
    fn chatterino_exe_path_is_detected() {
        assert!(image_path_looks_like_chatterino(
            r"C:\Program Files\Chatterino\chatterino.exe"
        ));
        assert!(image_path_looks_like_chatterino("Chatterino.EXE"));
        assert!(!image_path_looks_like_chatterino(r"C:\mpv\mpv.exe"));
    }
}
