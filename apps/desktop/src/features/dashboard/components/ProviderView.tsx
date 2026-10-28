import { useEffect, useState } from "react";
import { motion } from "motion/react";
import { Area, AreaChart, ResponsiveContainer, Tooltip, YAxis } from "recharts";
import { copy } from "../../../styles/copy";
import { ProviderIcon } from "../../../components/ProviderIcon";
import { IconRefresh } from "../../../components/icons";
import {
  burnExhaustionHint as burnHint,
  formatBurnRate,
  formatCompact,
  quotaRatio,
  quotaWindowTag,
} from "../../../lib/format";
import { QuotaMetric } from "./QuotaMetric";
import { LEVEL_CLASS } from "./ProviderMatrix";
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

interface ProviderTrendPoint {
  label: string;
  tokens: number | null;
  dayStart: number;
}

/**
 * Hover details for the provider trend: that day's consumption plus the
 * account's current quota windows, so one hover answers "how much and how
 * much is left" without leaving the chart.
 */
function ProviderTrendTooltip({
  active,
  payload,
  account,
}: {
  active?: boolean;
  payload?: ReadonlyArray<{ payload: ProviderTrendPoint }>;
  account: AccountViewModel;
}) {
  if (!active || !payload || payload.length === 0) return null;
  const point = payload[0].payload;
  const date = new Date(point.dayStart * 1000);
  const quotas = account.snapshots
    .map((quota) => {
      const tag = quotaWindowTag(quota);
      const ratio = quotaRatio(quota);
      return tag && ratio !== null ? { tag, ratio } : null;
    })
    .filter((entry): entry is { tag: string; ratio: number } => entry !== null);
  return (
    <div className="trend-tip">
      <p className="trend-tip__date">
        {date.getMonth() + 1}月{date.getDate()}日 · {copy.usage.tokens}{" "}
        {point.tokens === null ? copy.summary.unknown : formatCompact(point.tokens)}
      </p>
      {quotas.length > 0 && (
        <div className="trend-tip__quotas">
          <span>{copy.history.quotaOverview}</span>
          {quotas.map((entry) => (
            <div key={entry.tag} className="trend-tip__quota-row">
              <span>{entry.tag}</span>
              <span className="num">{Math.round(entry.ratio * 100)}%</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
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
  const [trend, setTrend] = useState<
    Array<{ label: string; tokens: number | null; dayStart: number }>
  >([]);
  // The capability chip reflects the provider's measured level from the
  // embedded matrix instead of a hardcoded value; unknown ids show nothing.
  const [capabilityLevel, setCapabilityLevel] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    setUsage(null);
    setCapabilityLevel(null);
    bridge
      .usageWindows(account.id)
      .then((value) => {
        if (active) setUsage(value);
      })
      .catch(() => null);
    const prefix = account.providerId.split(":")[0].trim().toLowerCase();
    bridge
      .providerMatrix()
      .then((rows) => {
        if (!active) return;
        const row = rows.find((item) => item.id === prefix);
        setCapabilityLevel(row && row.implemented && row.level ? row.level : null);
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
        const points: Array<{ label: string; tokens: number | null; dayStart: number }> = [];
        for (let offset = 29; offset >= 0; offset--) {
          const dayStart = todayStart - offset * 86_400;
          const day = byDay.get(dayStart);
          const date = new Date(dayStart * 1000);
          points.push({
            label: `${date.getMonth() + 1}/${date.getDate()}`,
            tokens: day?.hasData ? day.tokens : null,
            dayStart,
          });
        }
        setTrend(points);
      })
      .catch(() => null);
    return () => {
      active = false;
    };
  }, [bridge, account.id, account.providerId, account.lastUpdatedAt]);

  const range: RangeUsageViewModel | null = usage ? usage[windowKey] : null;
  // While the usage query is in flight the cards stay neutral ("—"); showing
  // the empty-window copy here would present loading as "no records".
  const tokensText =
    usage === null ? copy.summary.none : range?.hasData ? formatCompact(range.tokens) : copy.usage.noData;
  const burnRate: BurnRateViewModel | undefined = usage?.burnRate;

  return (
    <section className="pview" aria-label={account.label}>
      <div className="pview__head">
        <ProviderIcon providerId={account.providerId} size={40} />
        <div className="pview__names">
          <p className="pview__provider">{account.providerName}</p>
        </div>
        <StatusChip status={account.status} />
        {capabilityLevel && (
          <span
            className={`cap-chip ${LEVEL_CLASS[capabilityLevel] ?? "cap-chip--l1"}`}
            title={copy.a11y.capabilityChip(capabilityLevel)}
          >
            {capabilityLevel} · {copy.matrix.level[capabilityLevel] ?? capabilityLevel}
          </span>
        )}
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
            <dd className="num">{usage === null ? copy.summary.none : formatCost(range)}</dd>
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
          <div className="pview__trend">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={trend} margin={{ top: 2, right: 2, bottom: 0, left: 2 }}>
                <Tooltip
                  cursor={{ stroke: "var(--hairline-strong)", strokeWidth: 1 }}
                  content={(props) => (
                    <ProviderTrendTooltip
                      active={props.active}
                      payload={props.payload as ReadonlyArray<{ payload: ProviderTrendPoint }> | undefined}
                      account={account}
                    />
                  )}
                />
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
