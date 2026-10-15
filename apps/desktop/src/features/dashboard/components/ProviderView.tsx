import { useEffect, useState } from "react";
import { motion } from "motion/react";
import { Area, AreaChart, ResponsiveContainer, YAxis } from "recharts";
import { copy } from "../../../styles/copy";
import { ProviderIcon } from "../../../components/ProviderIcon";
import { IconRefresh } from "../../../components/icons";
import { burnExhaustionHint as burnHint, formatBurnRate, formatCompact } from "../../../lib/format";
import { QuotaMetric } from "./QuotaMetric";
import { StatusChip } from "./StatusChip";
import type {
  AccountViewModel,
  BurnRateViewModel,
  DashboardBridge,
  RangeUsageViewModel,
  UsageWindowsViewModel,
} from "../../../types";

type WindowKey = "hours2" | "hours24" | "days30";

const WINDOW_KEYS: WindowKey[] = ["hours2", "hours24", "days30"];

const springPanel = { type: "spring" as const, bounce: 0, duration: 0.36 };

const fadeVariants = {
  hidden: { opacity: 0, y: -6 },
  show: { opacity: 1, y: 0, transition: springPanel },
};

function formatCost(range: RangeUsageViewModel | null): string {
  if (!range || range.cost === null || range.cost === undefined || range.cost <= 0) {
    return copy.summary.none;
  }
  const amount = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 }).format(range.cost);
  return `${range.currency ?? ""}${amount}`;
}

/**
 * One provider's dedicated view: header card, trailing-window usage stats
 * (2h / 24h / 30 days) and the provider's quota metric cards.
 */
export function ProviderView({
  account,
  now,
  refreshing,
  bridge,
  onRefresh,
}: {
  account: AccountViewModel;
  now: number;
  refreshing: boolean;
  bridge: DashboardBridge;
  onRefresh: () => void;
}) {
  const [windowKey, setWindowKey] = useState<WindowKey>("hours24");
  const [usage, setUsage] = useState<UsageWindowsViewModel | null>(null);
  const [trend, setTrend] = useState<Array<{ label: string; tokens: number | null }>>([]);

  useEffect(() => {
    let active = true;
    setUsage(null);
    bridge
      .usageWindows(account.id)
      .then((value) => {
        if (active) setUsage(value);
      })
      .catch(() => null);
    bridge
      .historySummary(30, account.id)
      .then((summary) => {
        if (!active) return;
        const byDay = new Map(summary.days.map((day) => [day.dayStart, day]));
        const today = new Date();
        const todayStart = Math.floor(
          new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime() / 1000,
        );
        const points: Array<{ label: string; tokens: number | null }> = [];
        for (let offset = 29; offset >= 0; offset--) {
          const dayStart = todayStart - offset * 86_400;
          const day = byDay.get(dayStart);
          const date = new Date(dayStart * 1000);
          points.push({
            label: `${date.getMonth() + 1}/${date.getDate()}`,
            tokens: day?.hasData ? day.tokens : null,
          });
        }
        setTrend(points);
      })
      .catch(() => null);
    return () => {
      active = false;
    };
  }, [bridge, account.id, account.lastUpdatedAt]);

  const range: RangeUsageViewModel | null = usage ? usage[windowKey] : null;
  const tokensText = range?.hasData ? formatCompact(range.tokens) : copy.usage.noData;
  const burnRate: BurnRateViewModel | undefined = usage?.burnRate;

  return (
    <section className="pview" aria-label={account.label}>
      <div className="pview__head">
        <ProviderIcon providerId={account.providerId} size={40} />
        <div className="pview__names">
          <h2 className="pview__title">{account.label}</h2>
          <p className="pview__provider">{account.providerName}</p>
        </div>
        <StatusChip status={account.status} />
        <span className="cap-chip cap-chip--l1" title={copy.a11y.capabilityChip("L1")}>
          {copy.matrix.level.L1}
        </span>
        <button
          className="btn btn--icon"
          type="button"
          onClick={onRefresh}
          disabled={refreshing}
          aria-label={copy.a11y.refreshAccount(account.label)}
          title={copy.actions.refresh}
        >
          <IconRefresh size={18} className={refreshing ? "is-spinning" : undefined} />
        </button>
      </div>

      <motion.div
        className="pview__usage"
        variants={fadeVariants}
        initial="hidden"
        animate="show"
        aria-label={copy.usage.title}
      >
        <div className="seg" role="group" aria-label={copy.a11y.usageTabs}>
          {WINDOW_KEYS.map((key) => (
            <button
              key={key}
              type="button"
              className={`seg__btn${windowKey === key ? " is-active" : ""}`}
              aria-pressed={windowKey === key}
              onClick={() => setWindowKey(key)}
            >
              {windowKey === key && (
                <motion.span
                  className="seg__pill"
                  layoutId="usage-pill"
                  transition={{ type: "spring", bounce: 0, duration: 0.32 }}
                />
              )}
              <span className="seg__label">{copy.usage[key]}</span>
            </button>
          ))}
        </div>
        <dl className="pview__numbers">
          <div className="pview__number">
            <dt>{copy.usage.tokens}</dt>
            <dd className="num">{tokensText}</dd>
          </div>
          <div className="pview__number pview__number--muted">
            <dt>{copy.usage.cost}</dt>
            <dd className="num">{formatCost(range)}</dd>
          </div>
          {burnRate && (
            <div className="pview__number">
              <dt>{copy.usage.burnRate}</dt>
              <dd className="num">{formatBurnRate(burnRate)}</dd>
            </div>
          )}
        </dl>
        <p className="pview__hint">{burnRate ? burnHint(burnRate) : copy.usage.growing}</p>
        {trend.some((point) => (point.tokens ?? 0) > 0) && (
          <div className="pview__trend" aria-hidden="true">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={trend} margin={{ top: 2, right: 2, bottom: 0, left: 2 }}>
                <defs>
                  <linearGradient id="pview-trend-fill" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor="var(--brand)" stopOpacity={0.3} />
                    <stop offset="100%" stopColor="var(--brand)" stopOpacity={0.02} />
                  </linearGradient>
                </defs>
                <YAxis hide domain={[0, "dataMax"]} />
                <Area
                  type="monotone"
                  dataKey="tokens"
                  stroke="var(--brand)"
                  strokeWidth={1.6}
                  fill="url(#pview-trend-fill)"
                  connectNulls={false}
                  dot={false}
                />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        )}
      </motion.div>

      {account.snapshots.length > 0 ? (
        <ul className="quotas">
          {account.snapshots.map((quota) => (
            <QuotaMetric key={quota.id} quota={quota} now={now} />
          ))}
        </ul>
      ) : (
        <div className="notice notice--inline">
          <span>
            {account.status === "unavailable" ? copy.quota.emptyUnavailable : copy.quota.emptyBody}
          </span>
        </div>
      )}
    </section>
  );
}
