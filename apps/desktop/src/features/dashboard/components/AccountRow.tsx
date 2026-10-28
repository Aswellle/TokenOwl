import { useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { copy } from "../../../styles/copy";
import { ProviderIcon } from "../../../components/ProviderIcon";
import { IconAlert, IconRefresh, IconTrash } from "../../../components/icons";
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
  onOpen,
  onRefresh,
  onDelete,
}: {
  account: AccountViewModel;
  index: number;
  now: number;
  refreshing: boolean;
  deleting: boolean;
  onOpen: () => void;
  onRefresh: () => void;
  onDelete: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
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
      {account.status === "stale" && hasSnapshots && (
        <Notice tone="warning">{copy.notices.accountStale}</Notice>
      )}

      {hasSnapshots ? (
        <ul className="quotas">
          {account.snapshots.map((quota) => (
            <QuotaMetric key={quota.id} quota={quota} now={now} />
          ))}
        </ul>
      ) : (
        <div className="notice notice--inline">
          <span className="notice__icon">
            <IconAlert size={14} />
          </span>
          <span>
            {account.status === "unavailable" ? copy.quota.emptyUnavailable : copy.quota.emptyBody}
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
    </motion.li>
  );
}
