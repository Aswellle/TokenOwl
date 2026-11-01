import { useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { copy } from "../../../styles/copy";
import { ProviderIcon } from "../../../components/ProviderIcon";
import { IconAlert, IconKey, IconRefresh, IconTrash } from "../../../components/icons";
import { StatusChip } from "./StatusChip";
import { formatRelative, displaySource } from "../../../lib/format";
import { QuotaMetric } from "./QuotaMetric";
import { Notice } from "./Notice";
import type { AccountViewModel } from "../../../types";

const springPanel = { type: "spring", bounce: 0, duration: 0.42 } as const;

const springItem = { type: "spring", bounce: 0, duration: 0.36 } as const;

const listVariants = {
  hidden: { opacity: 0, y: 10, scale: 0.99 },
  show: ({ index, reduce }: { index: number; reduce: boolean }) => ({
    opacity: 1,
    y: 0,
    scale: 1,
    transition: reduce
      ? { duration: 0 }
      : { ...springItem, delay: Math.min(index * 0.045, 0.24) },
  }),
  hide: { opacity: 0, scale: 0.96, transition: { duration: 0.14, ease: [0.23, 1, 0.32, 1] as const } },
};

export function AccountRow({
  account,
  index,
  now,
  refreshing,
  deleting,
  updatingKey,
  onOpen,
  onRefresh,
  onDelete,
  onUpdateKey,
}: {
  account: AccountViewModel;
  index: number;
  now: number;
  refreshing: boolean;
  deleting: boolean;
  updatingKey: boolean;
  onOpen: () => void;
  onRefresh: () => void;
  onDelete: () => void;
  onUpdateKey: (apiKey: string) => Promise<boolean>;
}) {
  const [confirming, setConfirming] = useState(false);
  const [keyDialog, setKeyDialog] = useState(false);
  const keyInputRef = useRef<HTMLInputElement>(null);
  const reduce = useReducedMotion();
  const relative = formatRelative(account.lastUpdatedAt ?? account.snapshots[0]?.observedAt, now);
  const source = displaySource(account.source ?? account.snapshots[0]?.source);
  const hasSnapshots = account.snapshots.length > 0;
  const freshness = copy.freshness[account.freshness] ?? copy.freshness.unknown;

  return (
    <motion.li
      className={`account${deleting ? " account--busy" : ""}`}
      custom={{ index, reduce }}
      variants={listVariants}
      initial={reduce ? false : "hidden"}
      animate="show"
      exit="hide"
      layout={!confirming}
    >
      <div className="account__head">
        {/* The identity is the row's navigation affordance: it looks like the
            card's clickable face, so it must actually open the provider
            view — the tools on the right stay separate actions. */}
        <button
          className="account__identity"
          type="button"
          onClick={onOpen}
          title={copy.a11y.selectProvider(account.label)}
        >
          <ProviderIcon providerId={account.providerId} size={32} />
          <span className="account__names">
            <span className="account__label">{account.label}</span>
            <span className="account__provider">{account.providerName}</span>
          </span>
        </button>
        <div className="account__tools">
          <StatusChip status={account.status} />
          <button
            className="btn btn--icon"
            type="button"
            onClick={onRefresh}
            disabled={refreshing || deleting}
            aria-label={copy.a11y.refreshAccount(account.label)}
            title={copy.actions.refresh}
          >
            <IconRefresh size={18} className={refreshing ? "is-spinning" : undefined} />
          </button>
          <button
            className="btn btn--icon"
            type="button"
            onClick={() => setConfirming(true)}
            disabled={refreshing || deleting}
            aria-label={copy.a11y.removeAccount(account.label)}
            title={copy.actions.remove}
          >
            <IconTrash size={18} />
          </button>
        </div>
      </div>

      {account.status === "error" && hasSnapshots && (
        <Notice tone="danger">{copy.notices.accountError}</Notice>
      )}
      {account.status === "error" && !hasSnapshots && (
        <Notice tone="danger">{copy.notices.accountErrorEmpty}</Notice>
      )}
      {account.status === "stale" && hasSnapshots && (
        <Notice tone="warning">{copy.notices.accountStale}</Notice>
      )}

      {account.status === "error" && (
        <div className="account__recovery">
          <button
            className="btn btn--quiet"
            type="button"
            onClick={() => setKeyDialog(true)}
            disabled={updatingKey || refreshing || deleting}
            aria-label={copy.a11y.updateKeyFor(account.label)}
            title={copy.actions.updateKey}
          >
            <IconKey size={14} />
            {updatingKey ? copy.actions.updatingKey : copy.actions.updateKey}
          </button>
        </div>
      )}

      {hasSnapshots ? (
        <ul className="quotas">
          {account.snapshots.map((quota) => (
            <QuotaMetric key={quota.id} quota={quota} now={now} />
          ))}
        </ul>
      ) : account.status === "error" ? null : (
        <div className="notice notice--inline">
          <span className="notice__icon">
            <IconAlert size={14} />
          </span>
          <span>
            {account.status === "unavailable"
              ? // An L1/L2 provider never publishes key-level quota; naming
                // that beats a "no recognizable fields" failure reading.
                account.capabilityLevel === "L1" || account.capabilityLevel === "L2"
                ? copy.quota.noQuotaCapability
                : copy.quota.emptyUnavailable
              : copy.quota.emptyBody}
          </span>
        </div>
      )}

      <div className="account__foot">
        <span>
          {freshness}
          {relative ? ` · ${copy.time.updated(relative)}` : ` · ${copy.freshness.neverSynced}`}
          {source ? ` · ${source}` : ""}
        </span>
        <button className="btn btn--quiet" type="button" onClick={onRefresh} disabled={refreshing || deleting}>
          <IconRefresh size={14} className={refreshing ? "is-spinning" : undefined} />
          {refreshing ? copy.actions.refreshing : copy.actions.refresh}
        </button>
      </div>

      <AnimatePresence initial={false}>
        {confirming && (
          <motion.div
            className="confirm"
            role="alertdialog"
            aria-label={copy.actions.confirmRemove}
            initial={{ opacity: 0, y: -4, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1, transition: springPanel }}
            exit={{ opacity: 0, scale: 0.98, transition: { duration: 0.14 } }}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.stopPropagation();
                setConfirming(false);
              }
            }}
          >
            <p className="confirm__text">{copy.removeConfirm(account.label)}</p>
            <div className="confirm__actions">
              {/* Focus lands on the non-destructive action first: a stray
                  Enter must cancel the removal, not confirm it. */}
              <button
                className="btn btn--quiet"
                type="button"
                autoFocus
                onClick={() => setConfirming(false)}
              >
                {copy.actions.cancel}
              </button>
              <button
                className="btn btn--danger"
                type="button"
                onClick={() => {
                  setConfirming(false);
                  onDelete();
                }}
              >
                {copy.actions.confirmRemove}
              </button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence initial={false}>
        {keyDialog && (
          <motion.div
            className="confirm key-confirm"
            role="alertdialog"
            aria-label={copy.keyUpdate.title}
            initial={{ opacity: 0, y: -4, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1, transition: springPanel }}
            exit={{ opacity: 0, scale: 0.98, transition: { duration: 0.14 } }}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.stopPropagation();
                setKeyDialog(false);
              }
            }}
          >
            <div className="key-confirm__body">
              <p className="key-confirm__title">{copy.keyUpdate.title}</p>
              <p className="key-confirm__hint">{copy.keyUpdate.hint}</p>
              {/* Uncontrolled like the setup form's key field: the secret
                  never lands in React state. type=password keeps it masked. */}
              <input
                ref={keyInputRef}
                className="key-confirm__input"
                type="password"
                autoComplete="off"
                placeholder={copy.keyUpdate.placeholder}
                aria-label={copy.keyUpdate.placeholder}
              />
            </div>
            <div className="confirm__actions">
              <button
                className="btn btn--quiet"
                type="button"
                autoFocus
                onClick={() => setKeyDialog(false)}
              >
                {copy.actions.cancel}
              </button>
              <button
                className="btn btn--primary"
                type="button"
                disabled={updatingKey}
                onClick={() => {
                  const input = keyInputRef.current;
                  if (!input || !input.value.trim()) return;
                  void onUpdateKey(input.value).then((saved) => {
                    if (saved) {
                      input.value = "";
                      setKeyDialog(false);
                    }
                  });
                }}
              >
                {updatingKey ? copy.actions.updatingKey : copy.keyUpdate.confirm}
              </button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.li>
  );
}
