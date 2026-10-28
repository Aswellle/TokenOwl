import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { copy } from "../../../styles/copy";
import type {
  AccountViewModel,
  DashboardBridge,
  GenericOpenAiAccountInput,
} from "../../../types";

export type LoadPhase = "loading" | "ready" | "offline";

export interface NoticeState {
  text: string;
  tone?: "warning" | "danger" | "accent";
}

/** Merges a single-account refresh answer into the list without truncating
 * the accounts the answer did not mention. */
function mergeAccounts(
  current: AccountViewModel[],
  updates: AccountViewModel[],
): AccountViewModel[] {
  const byId = new Map(current.map((account) => [account.id, account]));
  updates.forEach((account) => byId.set(account.id, account));
  return Array.from(byId.values());
}

/**
 * Account data and every mutation against it: loading, refresh, creation,
 * removal. Extracted from the dashboard shell so React state transitions
 * and IPC error handling live in one testable place; the shell keeps only
 * view state. Behavior is intentionally identical to the inline code it
 * replaced.
 */
export function useAccounts(bridge: DashboardBridge) {
  const loadAccountsRequestId = useRef(0);
  const [accounts, setAccounts] = useState<AccountViewModel[]>([]);
  const [phase, setPhase] = useState<LoadPhase>("loading");
  const [refreshingId, setRefreshingId] = useState<string | "all" | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [formSubmitting, setFormSubmitting] = useState(false);
  const [notice, setNotice] = useState<NoticeState | null>(null);

  const loadAccounts = async () => {
    const requestId = ++loadAccountsRequestId.current;
    setPhase("loading");
    try {
      const result = await bridge.listAccounts();
      if (requestId !== loadAccountsRequestId.current) return;
      setAccounts(result);
      setPhase("ready");
      setNotice(null);
    } catch {
      if (requestId !== loadAccountsRequestId.current) return;
      setPhase("offline");
      setNotice({ text: copy.notices.loadFailed, tone: "warning" });
    }
  };

  // First mount: load once. Unlike an explicit reload, a successful first
  // load does not clear a notice that may have raced onto the screen.
  useEffect(() => {
    let active = true;
    bridge
      .listAccounts()
      .then((loaded) => {
        if (!active) return;
        setAccounts(loaded);
        setPhase("ready");
      })
      .catch(() => {
        if (!active) return;
        setPhase("offline");
        setNotice({ text: copy.notices.loadFailed, tone: "warning" });
      });
    return () => {
      active = false;
    };
  }, [bridge]);

  const handleRefresh = useCallback(
    async (accountId?: string) => {
      setRefreshingId(accountId ?? "all");
      setNotice(null);
      try {
        const updated = await bridge.refresh(accountId);
        setAccounts((current) => (accountId ? mergeAccounts(current, updated) : updated));
        setPhase("ready");
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        setAccounts((current) =>
          current.map((account) =>
            accountId && account.id !== accountId
              ? account
              : {
                  ...account,
                  status: "error",
                  errorMessage: message || copy.notices.refreshFailedOne,
                },
          ),
        );
        setNotice({ text: message || copy.notices.refreshFailed, tone: "warning" });
      } finally {
        setRefreshingId(null);
      }
    },
    [bridge],
  );

  const handleCreate = useCallback(
    async (
      input: Omit<GenericOpenAiAccountInput, "apiKey"> & { apiKeyInput: HTMLInputElement },
    ): Promise<boolean> => {
      if (!input.label || !input.baseUrl) {
        input.apiKeyInput.value = "";
        setFormError(copy.form.missingAccount);
        return false;
      }
      const secret = input.apiKeyInput.value;
      if (!secret.trim()) {
        setFormError(copy.form.missingKey);
        return false;
      }
      setFormSubmitting(true);
      setFormError(null);
      try {
        const account = await bridge.createAccount({
          providerId: input.providerId,
          label: input.label,
          baseUrl: input.baseUrl,
          quotaEndpoint: input.quotaEndpoint,
          usageEndpoint: input.usageEndpoint,
          balanceEndpoint: input.balanceEndpoint,
          apiFormat: input.apiFormat,
          lanConsentToken: input.lanConsentToken,
          apiKey: secret,
        });
        setAccounts((current) => [...current.filter((item) => item.id !== account.id), account]);
        setPhase("ready");
        setNotice(null);
        // A fresh form for the next account; failures keep every field so the
        // user can correct and retry without retyping the secret.
        input.apiKeyInput.value = "";
        return true;
      } catch {
        setFormError(copy.form.failed);
        return false;
      } finally {
        setFormSubmitting(false);
      }
    },
    [bridge],
  );

  const handleDelete = useCallback(
    async (accountId: string, onRemoved?: () => void) => {
      setDeletingId(accountId);
      try {
        await bridge.deleteAccount(accountId);
        setAccounts((current) => current.filter((account) => account.id !== accountId));
        onRemoved?.();
      } catch {
        setNotice({ text: copy.notices.deleteFailed, tone: "danger" });
      } finally {
        setDeletingId(null);
      }
    },
    [bridge],
  );

  return {
    accounts,
    phase,
    notice,
    refreshingId,
    deletingId,
    formError,
    formSubmitting,
    loadAccounts,
    handleRefresh,
    handleCreate,
    handleDelete,
    setNotice,
  };
}

export type AccountsApi = ReturnType<typeof useAccounts>;
export type NoticeSetter = Dispatch<SetStateAction<NoticeState | null>>;
