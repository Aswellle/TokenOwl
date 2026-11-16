import { useEffect, useRef, useState } from "react";
import { AlertDialog } from "../../../components/AlertDialog";
import { copy } from "../../../styles/copy";
import { IconRefresh } from "../../../components/icons";
import type {
  AccountConfigExportResult,
  AccountImportEntryViewModel,
  AccountImportPreview,
  AccountViewModel,
  DashboardBridge,
} from "../../../types";

type RowState =
  | { status: "importing" }
  | { status: "imported" }
  | { status: "unverified" }
  | { status: "failed"; message?: string };

interface PendingLan {
  entry: AccountImportEntryViewModel;
  apiKey: string;
  token: string;
}

function apiFormatOf(entry: AccountImportEntryViewModel): "openai_compatible" | "anthropic" | "gemini" {
  if (entry.apiFormat === "anthropic") return "anthropic";
  if (entry.apiFormat === "gemini") return "gemini";
  return "openai_compatible";
}

/**
 * Secret-free account backup. Export selects account configs only; import
 * previews a pasted document, then each row requires a fresh API key and goes
 * through the normal create path (first-refresh verification and LAN consent
 * included). Nothing is created by the preview itself.
 */
export function BackupSection({
  bridge,
  accounts,
  onImported,
}: {
  bridge: DashboardBridge;
  accounts: AccountViewModel[];
  onImported: (account: AccountViewModel) => void;
}) {
  const [selected, setSelected] = useState<Set<string>>(
    () => new Set(accounts.map((account) => account.id)),
  );
  const [exporting, setExporting] = useState(false);
  const [exportResult, setExportResult] = useState<AccountConfigExportResult | null>(null);
  const [exportFailed, setExportFailed] = useState(false);
  const [payload, setPayload] = useState("");
  const [previewing, setPreviewing] = useState(false);
  const [preview, setPreview] = useState<AccountImportPreview | null>(null);
  const [previewFailed, setPreviewFailed] = useState(false);
  const [keys, setKeys] = useState<Record<number, string>>({});
  const [rows, setRows] = useState<Record<number, RowState>>({});
  const [lanPending, setLanPending] = useState<PendingLan | null>(null);
  const lanTrigger = useRef<HTMLButtonElement | null>(null);
  const importButtons = useRef(new Map<number, HTMLButtonElement>());
  const initialized = useRef(false);

  // Select every account on the first non-empty load (the parent starts with
  // an empty list while the first IPC read is in flight), then only drop
  // selections for accounts that disappeared. A newly imported account is not
  // forced into the next export.
  useEffect(() => {
    setSelected((current) => {
      const existing = new Set(accounts.map((account) => account.id));
      if (!initialized.current && accounts.length > 0) {
        initialized.current = true;
        return existing;
      }
      const next = new Set([...current].filter((id) => existing.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [accounts]);

  const toggle = (id: string, checked: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      if (checked) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  const exportConfig = async () => {
    setExporting(true);
    setExportFailed(false);
    setExportResult(null);
    try {
      const result = await bridge.exportAccountConfig(
        accounts.filter((account) => selected.has(account.id)).map((account) => account.id),
      );
      setExportResult(result);
    } catch {
      setExportFailed(true);
    } finally {
      setExporting(false);
    }
  };

  const checkPayload = async () => {
    setPreviewing(true);
    setPreviewFailed(false);
    setPreview(null);
    setRows({});
    setKeys({});
    try {
      const view = await bridge.previewAccountImport(payload);
      setPreview(view);
    } catch {
      setPreviewFailed(true);
    } finally {
      setPreviewing(false);
    }
  };

  const createEntry = async (
    entry: AccountImportEntryViewModel,
    apiKey: string,
    lanConsentToken?: string,
  ) => {
    setRows((current) => ({ ...current, [entry.index]: { status: "importing" } }));
    try {
      const account = await bridge.createAccount({
        providerId: entry.providerId,
        label: entry.label,
        baseUrl: entry.baseUrl,
        quotaEndpoint: entry.quotaEndpoint ?? undefined,
        usageEndpoint: entry.usageEndpoint ?? undefined,
        balanceEndpoint: entry.balanceEndpoint ?? undefined,
        apiFormat: apiFormatOf(entry),
        apiKey,
        lanConsentToken,
      });
      onImported(account);
      setKeys((current) => {
        const next = { ...current };
        delete next[entry.index];
        return next;
      });
      setRows((current) => ({
        ...current,
        [entry.index]: {
          status: account.diagnostics?.status === "healthy" ? "imported" : "unverified",
        },
      }));
    } catch {
      setRows((current) => ({ ...current, [entry.index]: { status: "failed" } }));
    }
  };

  const importEntry = async (entry: AccountImportEntryViewModel) => {
    const apiKey = (keys[entry.index] ?? "").trim();
    if (!apiKey) {
      setRows((current) => ({
        ...current,
        [entry.index]: { status: "failed", message: copy.backup.missingKey },
      }));
      return;
    }
    setRows((current) => ({ ...current, [entry.index]: { status: "importing" } }));
    let token: string | null = null;
    try {
      token = await bridge.lanConsentBegin(entry.baseUrl);
    } catch {
      setRows((current) => ({ ...current, [entry.index]: { status: "failed" } }));
      return;
    }
    if (token) {
      setLanPending({ entry, apiKey, token });
      setRows((current) => {
        const next = { ...current };
        delete next[entry.index];
        return next;
      });
      return;
    }
    await createEntry(entry, apiKey);
  };

  return (
    <section className="section backup-section" aria-labelledby="backup-title">
      <div className="section__titlewrap">
        <h2 className="section__title" id="backup-title">
          {copy.backup.title}
        </h2>
      </div>
      <p className="section__intro">{copy.backup.intro}</p>

      <div className="backup__block">
        <h3 className="backup__heading">{copy.backup.exportTitle}</h3>
        {accounts.length === 0 ? (
          <p className="history__note">{copy.backup.noAccounts}</p>
        ) : (
          <>
            <div className="backup__select-actions">
              <button
                className="btn btn--quiet"
                type="button"
                onClick={() => setSelected(new Set(accounts.map((account) => account.id)))}
              >
                {copy.backup.selectAll}
              </button>
              <button
                className="btn btn--quiet"
                type="button"
                onClick={() => setSelected(new Set())}
              >
                {copy.backup.selectNone}
              </button>
            </div>
            <ul className="backup__accounts">
              {accounts.map((account) => (
                <li key={account.id}>
                  <label>
                    <input
                      type="checkbox"
                      checked={selected.has(account.id)}
                      onChange={(event) => toggle(account.id, event.currentTarget.checked)}
                    />
                    <span>
                      <strong>{account.label}</strong> · {account.providerName}
                    </span>
                  </label>
                </li>
              ))}
            </ul>
            <button
              className="btn btn--quiet"
              type="button"
              onClick={() => void exportConfig()}
              disabled={exporting || selected.size === 0}
            >
              <IconRefresh size={14} className={exporting ? "is-spinning" : undefined} />
              {exporting ? copy.backup.exporting : copy.backup.export}
            </button>
            {exportFailed && (
              <p className="form__error" role="alert">
                {copy.backup.exportFailed}
              </p>
            )}
            {exportResult && (
              <p className="history__note" role="status">
                {copy.backup.exported(exportResult.path, exportResult.accounts)}
              </p>
            )}
          </>
        )}
      </div>

      <div className="backup__block">
        <h3 className="backup__heading">{copy.backup.importTitle}</h3>
        <p className="history__note">{copy.backup.importHint}</p>
        <div className="field">
          <label className="field__label" htmlFor="backup-payload">
            {copy.backup.payloadLabel}
          </label>
          <textarea
            className="field__input mono backup__payload"
            id="backup-payload"
            rows={7}
            value={payload}
            spellCheck={false}
            onChange={(event) => {
              setPayload(event.currentTarget.value);
              setPreview(null);
              setPreviewFailed(false);
            }}
          />
        </div>
        <button
          className="btn btn--quiet"
          type="button"
          onClick={() => void checkPayload()}
          disabled={previewing || payload.trim().length === 0}
        >
          {previewing ? copy.backup.previewing : copy.backup.preview}
        </button>
        {previewFailed && (
          <p className="form__error" role="alert">
            {copy.backup.previewFailed}
          </p>
        )}
        {preview && (
          <div className="backup__preview">
            <p className="history__note" role="status">
              {copy.backup.previewSummary(
                preview.schemaVersion,
                preview.entries.length,
                new Date(preview.exportedAt * 1000).toLocaleString("zh-CN"),
              )}
            </p>
            {preview.entries.length === 0 && (
              <p className="history__note">{copy.backup.previewEmpty}</p>
            )}
            <ul className="backup__entries">
              {preview.entries.map((entry) => {
                const row = rows[entry.index];
                return (
                  <li key={entry.index} className="backup__entry">
                    <div className="backup__entry-head">
                      <span>
                        <strong>{entry.label}</strong> · {entry.providerName}
                      </span>
                      {entry.alreadyExists && (
                        <span className="cap-chip cap-chip--l3">{copy.backup.duplicate}</span>
                      )}
                      {!entry.importable && (
                        <span className="cap-chip cap-chip--l1">{copy.backup.unavailable}</span>
                      )}
                    </div>
                    <p className="backup__entry-detail mono">
                      {entry.providerId} · {entry.baseUrl}
                    </p>
                    {!entry.importable && entry.reason && (
                      <p className="form__error">{entry.reason}</p>
                    )}
                    {entry.importable && row?.status !== "imported" && row?.status !== "unverified" && (
                      <div className="backup__entry-import">
                        <label className="sr-only" htmlFor={`backup-key-${entry.index}`}>
                          {copy.backup.keyLabel(entry.label)}
                        </label>
                        <input
                          className="field__input secret-input"
                          id={`backup-key-${entry.index}`}
                          type="text"
                          autoComplete="off"
                          placeholder={copy.form.apiKeyPlaceholder}
                          spellCheck={false}
                          value={keys[entry.index] ?? ""}
                          onChange={(event) => {
                            const value = event.currentTarget.value;
                            setKeys((current) => ({ ...current, [entry.index]: value }));
                          }}
                        />
                        <button
                          className="btn btn--quiet"
                          type="button"
                          disabled={row?.status === "importing"}
                          ref={(node) => {
                            if (node) importButtons.current.set(entry.index, node);
                            else importButtons.current.delete(entry.index);
                          }}
                          onClick={() => {
                            lanTrigger.current = importButtons.current.get(entry.index) ?? null;
                            void importEntry(entry);
                          }}
                        >
                          {row?.status === "importing"
                            ? copy.backup.importing
                            : copy.backup.importEntry}
                        </button>
                      </div>
                    )}
                    {row?.status === "imported" && (
                      <p className="history__note" role="status">
                        {copy.backup.imported}
                      </p>
                    )}
                    {row?.status === "unverified" && (
                      <p className="form__error" role="status">
                        {copy.backup.unverified}
                      </p>
                    )}
                    {row?.status === "failed" && (
                      <p className="form__error" role="alert">
                        {row.message ?? copy.backup.importFailed}
                      </p>
                    )}
                  </li>
                );
              })}
            </ul>
          </div>
        )}
      </div>

      {lanPending && (
        <AlertDialog
          className="lan-consent"
          title={copy.backup.lanTitle}
          body={copy.backup.lanBody}
          confirmLabel={copy.backup.lanConfirm}
          cancelLabel={copy.actions.cancel}
          returnFocusTo={lanTrigger}
          onCancel={() => setLanPending(null)}
          onConfirm={() => {
            const pending = lanPending;
            setLanPending(null);
            void createEntry(pending.entry, pending.apiKey, pending.token);
          }}
        />
      )}
    </section>
  );
}
