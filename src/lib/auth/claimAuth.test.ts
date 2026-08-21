import { describe, expect, it } from "vitest";
import {
  applyBonusClaimsChipClick,
  type BonusClaimsUiState,
} from "./claimAuth";

describe("bonus claims chip", () => {
  it("opens the panel without disconnecting a connected session", () => {
    const state: BonusClaimsUiState = {
      expanded: false,
      status: { configured: true, login: "wibias", userId: "1" },
      device: { userCode: "ABCD" },
    };

    const next = applyBonusClaimsChipClick(state);

    expect(next.expanded).toBe(true);
    expect(next.status).toEqual(state.status);
    expect(next.device).toEqual(state.device);
  });

  it("opens the panel without starting device login when disconnected", () => {
    const state: BonusClaimsUiState = {
      expanded: false,
      status: { configured: false, login: null, userId: null },
      device: null,
    };

    const next = applyBonusClaimsChipClick(state);

    expect(next.expanded).toBe(true);
    expect(next.device).toBeNull();
    expect(next.status?.configured).toBe(false);
  });
});
