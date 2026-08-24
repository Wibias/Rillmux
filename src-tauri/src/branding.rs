//! Display name, binary identity, and leftover names from the archived fork.

pub const KEYRING_SERVICE: &str = "rillmux";
pub const KEYRING_SERVICE_LEGACY: &str = "streamlink-twitch-gui";
pub const PLAYER_WINDOW_PREFIX: &str = "rillmux";
pub const PLAYER_WINDOW_PREFIX_LEGACY: &str = "stgui";
pub const APP_IDENTIFIER_LEGACY: &str = "com.wibias.streamlinktwitchgui";
/// `%APPDATA%\<this>` for logs/crashes — a folder name, not the bundle id.
pub const APP_DATA_FOLDER: &str = "Rillmux";
/// First debug-folder name; migrated into `APP_DATA_FOLDER`.
pub const APP_DATA_FOLDER_PACKAGE: &str = "com.wibias.rillmux";
