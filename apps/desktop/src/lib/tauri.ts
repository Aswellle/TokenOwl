import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  AccountViewModel,
  DashboardBridge,
  GenericOpenAiAccountInput,
  NotificationPreferencesViewModel,
  NotificationViewModel,
  UpdateInfo,
} from "../types";

/**
 * The frontend only knows semantic actions. Rust owns persistence, credentials,
 * network access, and DTO-to-view-model conversion behind these commands.
 */
export const tauriBridge: DashboardBridge = {
  listAccounts: () => invoke<AccountViewModel[]>("accounts_list"),
  createAccount: (input: GenericOpenAiAccountInput) =>
    invoke<AccountViewModel>("accounts_create", { input }),
  deleteAccount: (accountId: string) => invoke<void>("accounts_delete", { accountId }),
  refresh: (accountId?: string) =>
    invoke<AccountViewModel[]>("accounts_refresh", accountId ? { accountId } : undefined),
  checkForUpdate: () => invoke<UpdateInfo | null>("updater_check"),
  installUpdate: () => invoke<void>("updater_install"),
  listNotifications: (limit?: number) =>
    invoke<NotificationViewModel[]>("notifications_list", limit ? { limit } : undefined),
  markNotificationsRead: (ids?: string[]) =>
    invoke<void>("notifications_mark_read", ids && ids.length > 0 ? { ids } : undefined),
  getNotificationPreferences: () =>
    invoke<NotificationPreferencesViewModel>("notification_preferences_get"),
  setNotificationPreferences: (prefs: NotificationPreferencesViewModel) =>
    invoke<void>("notification_preferences_set", { prefs }),
};

/** Window-shell actions that stay in Rust so capabilities stay narrow. */
export const shell = {
  openDashboard: () => invoke<void>("open_dashboard"),
  hideDashboard: () => invoke<void>("hide_dashboard"),
  minimizeDashboard: () => invoke<void>("minimize_dashboard"),
  showBubble: () => invoke<void>("show_bubble"),
  persistBubblePosition: () => invoke<void>("persist_bubble_position"),
};

/** True when running inside the Tauri webview (as opposed to a plain browser). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Which window this webview instance renders. The bubble window mounts the
 * compact BubbleView; everything else gets the full dashboard.
 */
export function windowMode(): "main" | "bubble" | "browser" {
  if (!isTauri()) return "browser";
  try {
    return getCurrentWindow().label === "bubble" ? "bubble" : "main";
  } catch {
    return "main";
  }
}
