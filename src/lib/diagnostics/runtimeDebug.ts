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

const SENSITIVE_FIELD = /(token|cookie|authorization|payload|input|device|secret)/i;

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
    .filter(([key]) => !SENSITIVE_FIELD.test(key))
    .flatMap(([key, value]) => {
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

export function syncRuntimeDebugCategories(categories: DebugCategories): void {
  if (!isTauri()) return;
  void invoke("diagnostics_set_debug_categories", { categories }).catch(
    () => undefined,
  );
}
