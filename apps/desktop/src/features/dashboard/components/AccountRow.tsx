import { motion, useReducedMotion } from "motion/react";
import { copy } from "../../../styles/copy";
import { ProviderIcon } from "../../../components/ProviderIcon";
import { IconRefresh, IconTrash } from "../../../components/icons";
import { displaySource, formatRelative, formatValue } from "../../../lib/format";
import type { AccountViewModel } from "../../../types";

export function AccountRow({
  account,
  index,
  now,
  refreshing,
  deleting,
  onRefresh,
  onDelete,
}: {
  account: AccountViewModel;
  index: number;
  now: number;
  refreshing: boolean;
  deleting: boolean;
  onRefresh: () => void;
  onDelete: () => void;
}) {
  const reduce = useReducedMotion();
  const relative = formatRelative(account.lastUpdatedAt ?? account.snapshots[0]?.observedAt, now);
  const source = displaySource(account.source ?? account.snapshots[0]?.source);
  const hasSnapshots = account.snapshots.length > 0;

  return (
    <motion.li
      className={`account${deleting ? " account--busy" : ""}`}
      custom={{ index, reduce }}
      variants={{
        hidden: { opacity: 0, y: 10, scale: 0.99 },
        show: ({ index, reduce }: { index: number; reduce: boolean }) => ({
          opacity: 1,
          y: 0,
          scale: 1,
          transition: reduce
            ? { duration: 0 }
            : { type: "spring", bounce: 0, duration: 0.36, delay: Math.min(index * 0.045, 0.24) },
        }),
        hide: { opacity: 0, scale: 0.96, transition: { duration: 0.14, ease: [0.23, 1, 0.32, 1] as const } },
      }}
      initial={reduce ? false : "hidden"}
      animate="show"
      exit="hide"
      layout={!deleting}
    >
      <div className="account__head">
        <div className="account__identity">
          <ProviderIcon providerId={account.providerId} size={32} />
          <div className="account__names">
            <h3 className="account__label" title={account.label}>
              {account.label}
            </h3>
            <p className="account__provider">{account.providerName}</p>
          </div>
        </div>
        <div className="account__tools">
          <span className={`chip chip--${account.status}`}>
            {copy.health[account.status] ?? account.status}
          </span>
          <button
            className="btn btn--icon"
            type="button"
            onClick={onRefresh}
            disabled={refreshing || deleting}
            aria-label={copy.a11y.refreshAccount(account.label)}
          >
            <IconRefresh size={15} className={refreshing ? "is-spinning" : ""} />
          </button>
          <button
            className="btn btn--icon"
            type="button"
            onClick={onDelete}
            disabled={refreshing || deleting}
            aria-label={copy.a11y.removeAccount(account.label)}
          >
            <IconTrash size={15} />
          </button>
        </div>
      </div>
      {hasSnapshots && (
        <div className="quotas">
          {account.snapshots.slice(0, 3).map((snapshot) => (
            <div className="quota" key={snapshot.id}>
              <div className="quota__head">
                <span className="quota__name">{snapshot.metricName}</span>
                <span className="quota__unit">{copy.units[snapshot.unit] ?? snapshot.unit}</span>
              </div>
              <div className="quota__values">
                <span className="quota__value">
                  {formatValue(snapshot.remaining, snapshot)} / {formatValue(snapshot.limit, snapshot)}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}
      <div className="account__foot">
        <span>{relative}</span>
        <span>{source}</span>
      </div>
    </motion.li>
  );
}
