import { describe, expect, it } from "vitest";
import { playerInstallGuide } from "./playerInstall";
import { MPV_SCOOP, MPV_WINGET } from "./mpv";

describe("playerInstallGuide", () => {
  it("returns the mpv commands already used in onboarding", () => {
    expect(playerInstallGuide("mpv")).toEqual({
      id: "mpv",
      name: "mpv",
      winget: MPV_WINGET,
      scoop: MPV_SCOOP,
      downloadUrl: "https://mpv.io/installation/",
    });
  });

  it("covers VLC, MPC-HC, and PotPlayer with winget ids and download pages", () => {
    expect(playerInstallGuide("vlc")).toMatchObject({
      winget: "winget install -e --id VideoLAN.VLC",
      downloadUrl: "https://www.videolan.org/vlc/",
    });
    expect(playerInstallGuide("mpc")).toMatchObject({
      winget: "winget install -e --id clsid2.mpc-hc",
      downloadUrl: "https://github.com/clsid2/mpc-hc/releases",
    });
    expect(playerInstallGuide("potplayer")).toMatchObject({
      winget: "winget install -e --id Daum.PotPlayer",
      downloadUrl: "https://potplayer.tv/",
    });
  });

  it("hides install help for a custom executable", () => {
    expect(playerInstallGuide("custom")).toBeNull();
  });
});
