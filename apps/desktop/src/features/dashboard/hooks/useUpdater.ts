import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { copy } from "../../../styles/copy";
import type { DashboardBridge, UpdateInfo } from "../../../types";
import type { NoticeSetter } from "./useAccounts";

export type ActionState = "idle" | "checking" | "available" | "current";

/**
 * The outcome of an update check, rendered as a banner at the top of the
 * page that hosts the check button. Both outcomes — a newer release and
 * "already current" — land here so the result is visible immediately where
 * the user pressed the button, each with its own dismiss control.
 */
export type UpdateBanner =
  | { kind: "available"; version: string }
  | { kind: "current" }
  | { kind: "failed" };

/**
 * Updater surface state: manual check/install plus the passive
 * `app:update-available` event the Rust side emits when the background
 * probe finds a newer release. Check results surface through the banner;
 * install outcomes stay on the shared notice.
 */
export function useUpdater(bridge: DashboardBridge, setNotice: NoticeSetter) {
  const [updateState, setUpdateState] = useState<ActionState>("idle");
  const [pendingUpdate, setPendingUpdate] = useState<UpdateInfo | null>(null);
  const [banner, setBanner] = useState<UpdateBanner | null>(null);

  useEffect(() => {
    let active = true;
    const unlisten = listen<UpdateInfo>("app:update-available", (event) => {
      if (!active) return;
      setUpdateState("available");
      setPendingUpdate(event.payload);
      // A background find is announced through the same banner as a manual
      // check, plus the global notice for users on other pages.
      setBanner({ kind: "available", version: event.payload.version });
      setNotice({ text: copy.update.available(event.payload.version), tone: "accent" });
    }).catch(() => null);
    return () => {
      active = false;
      void unlisten.then((dispose) => dispose?.());
    };
  }, [setNotice]);

  const handleUpdateCheck = useCallback(async () => {
    setUpdateState("checking");
    setBanner(null);
    try {
      const update = await bridge.checkForUpdate();
      if (!update) {
        setUpdateState("current");
        setPendingUpdate(null);
        setBanner({ kind: "current" });
        return;
      }
      setUpdateState("available");
      setPendingUpdate(update);
      setBanner({ kind: "available", version: update.version });
    } catch {
      setUpdateState("idle");
      setBanner({ kind: "failed" });
    }
  }, [bridge]);

  const handleInstallUpdate = useCallback(async () => {
    try {
      await bridge.installUpdate();
      setBanner(null);
      setNotice({ text: copy.update.installed, tone: "accent" });
    } catch {
      setNotice({ text: copy.update.unavailable, tone: "warning" });
    }
  }, [bridge, setNotice]);

  return {
    updateState,
    pendingUpdate,
    setPendingUpdate,
    banner,
    dismissBanner: () => setBanner(null),
    handleUpdateCheck,
    handleInstallUpdate,
  };
}
