import { useSettingsStore } from "../settings/store";
import type { DebugCategories } from "../settings/types";
import { invoke, isTauri } from "../tauri";

export type RuntimeDebugCategory =
  | "windows"
  | "points-credit"
  | "points-claim"
  | "rewards"
  | "polls"
  | "raids";

const CATEGORY_SETTING: Record<RuntimeDebugCategory, keyof DebugCategories> = {
  windows: "windows",
  "points-credit": "pointsCredit",
  "points-claim": "pointsClaim",
  rewards: "rewards",
  polls: "polls",
  raids: "raids",
};

// Free-form errors may embed URLs, auth material, or upstream payload fragments.
// Keep runtime diagnostics to explicitly structured correlation fields only.
const SENSITIVE_FIELD =
  /(token|cookie|authorization|payload|input|device|secret|reason|error|message)/i;
const IDENTIFIER_FIELD = /^(claimId|claim_id)$/;
const HASH_FIELD = /^(queryHash|persistedQueryHash|query_hash|persisted_query_hash)$/;

function redactIdentifier(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length <= 10) return "***";
  return `${trimmed.slice(0, 6)}…${trimmed.slice(-4)}`;
}

function redactHash(value: string): string {
  const prefix = value.trim().slice(0, 8);
  return prefix ? `${prefix}…` : "***";
}

function fieldValue(value: unknown): string | null {
  if (typeof value === "string") {
    return value.replace(/[\r\n\t]/g, " ").slice(0, 240);
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return null;
}

export function formatDebugFields(fields: Record<string, unknown>): string {
  return Object.entries(fields)
    .flatMap(([key, value]) => {
      if (SENSITIVE_FIELD.test(key)) return [];
      if (typeof value === "string" && IDENTIFIER_FIELD.test(key)) {
        return [`${key}=${redactIdentifier(value)}`];
      }
      if (typeof value === "string" && HASH_FIELD.test(key)) {
        return [`${key}=${redactHash(value)}`];
      }
      const formatted = fieldValue(value);
      return formatted === null ? [] : [`${key}=${formatted}`];
    })
    .join(" ");
}

export function runtimeDebugEnabled(category: RuntimeDebugCategory): boolean {
  const settings = useSettingsStore.getState().settings;
  return (
    settings.gui.debugMode &&
    settings.gui.debugCategories[CATEGORY_SETTING[category]]
  );
}

export function debugRuntimeEvent(
  category: RuntimeDebugCategory,
  event: string,
  fields: Record<string, unknown> = {},
): void {
  if (!isTauri() || !runtimeDebugEnabled(category)) return;
  void invoke("diagnostics_log_event", {
    category,
    event,
    fields: formatDebugFields(fields),
  }).catch(() => undefined);
}
