export interface ChannelPointsClaimAuthStatus {
  configured: boolean;
  login?: string | null;
  userId?: string | null;
}

export interface BonusClaimsDevice {
  userCode: string;
}

export interface BonusClaimsUiState {
  expanded: boolean;
  status: ChannelPointsClaimAuthStatus | null;
  device: BonusClaimsDevice | null;
}

/** Compact chip click only toggles the panel — never login or disconnect. */
export function applyBonusClaimsChipClick(
  state: BonusClaimsUiState,
): BonusClaimsUiState {
  return { ...state, expanded: !state.expanded };
}
