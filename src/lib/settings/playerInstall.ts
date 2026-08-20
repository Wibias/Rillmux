import { MPV_SCOOP, MPV_WINGET } from "./mpv";
import type { PlayerId } from "./types";

export interface PlayerInstallGuide {
  id: Exclude<PlayerId, "custom">;
  /** Proper name for the “How to install …” heading. */
  name: string;
  winget: string;
  scoop: string;
  downloadUrl: string;
}

const GUIDES: Record<Exclude<PlayerId, "custom">, PlayerInstallGuide> = {
  mpv: {
    id: "mpv",
    name: "mpv",
    winget: MPV_WINGET,
    scoop: MPV_SCOOP,
    downloadUrl: "https://mpv.io/installation/",
  },
  vlc: {
    id: "vlc",
    name: "VLC",
    winget: "winget install -e --id VideoLAN.VLC",
    scoop: "scoop install vlc",
    downloadUrl: "https://www.videolan.org/vlc/",
  },
  mpc: {
    id: "mpc",
    name: "MPC-HC",
    winget: "winget install -e --id clsid2.mpc-hc",
    scoop: "scoop install mpc-hc",
    downloadUrl: "https://github.com/clsid2/mpc-hc/releases",
  },
  potplayer: {
    id: "potplayer",
    name: "PotPlayer",
    winget: "winget install -e --id Daum.PotPlayer",
    scoop: "scoop install potplayer",
    downloadUrl: "https://potplayer.tv/",
  },
};

export function playerInstallGuide(id: PlayerId): PlayerInstallGuide | null {
  switch (id) {
    case "mpv":
    case "vlc":
    case "mpc":
    case "potplayer":
      return GUIDES[id];
    case "custom":
      return null;
    default: {
      const _never: never = id;
      return _never;
    }
  }
}
