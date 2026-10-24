import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { copy } from "../../../styles/copy";
import type { DashboardBridge, UpdateInfo } from "../../../types";
import type { NoticeSetter } from "./useAccounts";

export type ActionState = "idle" | "checking" | "available" | "current";

/**
 * Updater surface state: manual check/install plus the passive
 * `app:update-available` event the Rust side emits when the background
 * probe finds a newer release. Notices surface through the shared notice
 * banner, so the setter is injected by the shell.
 */
export function useUpdater(bridge: DashboardBridge, setNotice: NoticeSetter) {
  const [updateState, setUpdateState] = useState<ActionState>("idle");
  const [pendingUpdate, setPendingUpdate] = useState<UpdateInfo | null>(null);

  useEffect(() => {
    let active = true;
    const unlisten = listen<UpdateInfo>("app:update-available", (event) => {
      if (!active) return;
      setUpdateState("available");
      setPendingUpdate(event.payload);
    }).catch(() => null);
    return () => {
      active = false;
      void unlisten.then((dispose) => dispose?.());
    };
  }, []);

  const handleUpdateCheck = useCallback(async () => {
    setUpdateState("checking");
    try {
      const update = await bridge.checkForUpdate();
      if (!update) {
        setUpdateState("current");
        setPendingUpdate(null);
        setNotice({ text: copy.update.current, tone: "accent" });
        return;
      }
      setUpdateState("available");
      setPendingUpdate(update);
      setNotice({ text: copy.update.available(update.version), tone: "accent" });
    } catch {
      setUpdateState("idle");
      setNotice({ text: copy.update.unavailable, tone: "warning" });
    }
  }, [bridge, setNotice]);

  const handleInstallUpdate = useCallback(async () => {
    try {
      await bridge.installUpdate();
      setNotice({ text: copy.update.installed, tone: "accent" });
    } catch {
      setUpdateState("idle");
      setNotice({ text: copy.update.unavailable, tone: "warning" });
    }
  }, [bridge, setNotice]);

  return { updateState, pendingUpdate, setPendingUpdate, handleUpdateCheck, handleInstallUpdate };
}
