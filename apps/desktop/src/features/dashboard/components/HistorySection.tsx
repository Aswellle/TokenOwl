import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { motion } from "motion/react";
import { Area, AreaChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { copy } from "../../../styles/copy";
import { IconRefresh } from "../../../components/icons";
import { ProviderIcon } from "../../../components/ProviderIcon";
import { formatCompact, quotaRatio, quotaWindowTag } from "../../../lib/format";
import type {
  AccountViewModel,
  DashboardBridge,
  HistoryDayAccountViewModel,
  HistoryDayBreakdownViewModel,
  HistoryDayViewModel,
  HistorySummaryViewModel,
} from "../../../types";

interface TrendPoint {
  label: string;
  dayStart: number;
  tokens: number | null;
}

interface TrendTooltipProps {
  active?: boolean;
  payload?: ReadonlyArray<{ payload: TrendPoint }>;
  breakdown: Map<number, HistoryDayAccountViewModel[]>;
  accounts: AccountViewModel[];
}

/** Hover details: day totals, per-provider split, and each one's quota snapshot. */
function TrendTooltip({ active, payload, breakdown, accounts }: TrendTooltipProps) {
  if (!active || !payload || payload.length === 0) return null;
  const point = payload[0].payload;
  const date = new Date(point.dayStart * 1000);
  const entries = breakdown.get(point.dayStart) ?? [];
  return (
    <div className="trend-tip">
      <p className="trend-tip__date">
        {date.getMonth() + 1}月{date.getDate()}日 · Tokens {formatCompact(point.tokens ?? 0)}
      </p>
      {entries.map((entry) => {
        const account = accounts.find((item) => item.id === entry.accountId);
        return (
          <div key={entry.accountId} className="trend-tip__row">
            {account && <ProviderIcon providerId={account.providerId} size={14} />}
            <span className="trend-tip__label">{entry.label}</span>
            <strong className="num">{formatCompact(entry.tokens)}</strong>
          </div>
        );
      })}
      {entries.length > 0 && (
        <div className="trend-tip__quotas">
          <span>{copy.history.quotaOverview}</span>
          {accounts
            .filter((account) => entries.some((entry) => entry.accountId === account.id))
            .map((account) => {
              const parts = account.snapshots
                .map((quota) => {
                  const tag = quotaWindowTag(quota);
                  const ratio = quotaRatio(quota);
                  return tag && ratio !== null ? `${tag} ${Math.round(ratio * 100)}%` : null;
                })
                .filter((part): part is string => part !== null);
              return (
                <div key={account.id} className="trend-tip__quota-row">
                  <span>{account.label}</span>
                  <span className="num">{parts.join(" · ") || copy.summary.unknown}</span>
                </div>
              );
            })}
        </div>
      )}
    </div>
  );
}

const RANGE_DAYS = 182;
const WEEKDAY_LABELS: Record<number, string> = { 0: "一", 2: "三", 4: "五" };

const fadeVariants = {
  hidden: { opacity: 0, y: -6 },
  show: { opacity: 1, y: 0, transition: { type: "spring" as const, bounce: 0, duration: 0.36 } },
};

function localMidnightSeconds(date: Date): number {
  return Math.floor(
    new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime() / 1000,
  );
}

function cellDate(dayStart: number): string {
  const date = new Date(dayStart * 1000);
  return `${date.getMonth() + 1}月${date.getDate()}日`;
}

function cellDetail(day: HistoryDayViewModel | undefined): string {
  if (!day || !day.hasData) return copy.history.noData;
  const tokens = `${formatCompact(day.tokens)} Tokens`;
  if (day.cost !== null && day.cost !== undefined && day.cost > 0) {
    const amount = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 }).format(day.cost);
    return `${tokens}，${day.currency ?? ""}${amount}`;
  }
  return tokens;
}

/** Quartile cut points of nonzero daily usage, for GitHub-style levels. */
function computeCuts(days: HistoryDayViewModel[]): number[] {
  const nonzero = days
    .filter((day) => day.hasData && day.tokens > 0)
    .map((day) => day.tokens)
    .sort((left, right) => left - right);
  if (nonzero.length < 4) return [];
  const pick = (fraction: number) =>
    nonzero[Math.min(nonzero.length - 1, Math.floor(fraction * nonzero.length))];
  return [pick(0.25), pick(0.5), pick(0.75)];
}

function levelFor(tokens: number, cuts: number[], max: number): number {
  if (tokens <= 0) return 0;
  if (cuts.length === 3) {
    if (tokens >= cuts[2]) return 4;
    if (tokens >= cuts[1]) return 3;
    if (tokens >= cuts[0]) return 2;
    return 1;
  }
  if (max > 0) return Math.max(1, Math.min(4, Math.ceil((tokens / max) * 4)));
  return 1;
}

interface Cell {
  dayStart: number;
  level: number;
  hasData: boolean;
}

export function HistorySection({
  bridge,
  accounts,
  reloadSignal,
}: {
  bridge: DashboardBridge;
  accounts: AccountViewModel[];
  reloadSignal: number;
}) {
  const [summary, setSummary] = useState<HistorySummaryViewModel | null>(null);
  const [breakdown, setBreakdown] = useState<Map<number, HistoryDayAccountViewModel[]>>(new Map());
  const [failed, setFailed] = useState(false);
  const cellRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const [focusIndex, setFocusIndex] = useState<number | null>(null);

  const load = useCallback(async () => {
    setFailed(false);
    try {
      const [loaded, days] = await Promise.all([
        bridge.historySummary(RANGE_DAYS),
        bridge.historyBreakdown(30).catch(() => [] as HistoryDayBreakdownViewModel[]),
      ]);
      setSummary(loaded);
      setBreakdown(new Map(days.map((day) => [day.dayStart, day.accounts])));
    } catch {
      // A failed read is a terminal, explained state: an endless skeleton
      // reads as "still loading" forever and hides the problem.
      setFailed(true);
    }
  }, [bridge]);

  useEffect(() => {
    void load();
  }, [load, reloadSignal]);

  const cuts = useMemo(() => computeCuts(summary?.days ?? []), [summary]);
  const maxTokens = useMemo(
    () => Math.max(0, ...(summary?.days ?? []).map((day) => day.tokens)),
    [summary],
  );

  const grid = useMemo(() => {
    const byDay = new Map<number, HistoryDayViewModel>();
    (summary?.days ?? []).forEach((day) => byDay.set(day.dayStart, day));
    const today = localMidnightSeconds(new Date());
    const startSeconds = today - (RANGE_DAYS - 1) * 86_400;
    const startWeekday = (new Date(startSeconds * 1000).getDay() + 6) % 7;
    const gridStart = startSeconds - startWeekday * 86_400;
    const weeks = Math.ceil((RANGE_DAYS + startWeekday) / 7);
    const cells: Array<Cell | null> = [];
    for (let week = 0; week < weeks; week++) {
      for (let weekday = 0; weekday < 7; weekday++) {
        const dayStart = gridStart + (week * 7 + weekday) * 86_400;
        if (dayStart > today) {
          cells.push(null);
          continue;
        }
        const day = byDay.get(dayStart);
        cells.push({
          dayStart,
          level: levelFor(day?.tokens ?? 0, cuts, maxTokens),
          hasData: day?.hasData ?? false,
        });
      }
    }
    const months: Array<{ week: number; label: string }> = [];
    let lastMonth = -1;
    for (let week = 0; week < weeks; week++) {
      const date = new Date((gridStart + week * 7 * 86_400) * 1000);
      if (date.getMonth() !== lastMonth) {
        lastMonth = date.getMonth();
        months.push({ week, label: `${date.getMonth() + 1}月` });
      }
    }
    const todayIndex = cells.findLastIndex((cell) => cell !== null);
    return { cells, months, weeks, todayIndex };
  }, [summary, cuts, maxTokens]);

  const last30 = useMemo(() => {
    const today = localMidnightSeconds(new Date());
    const byDay = new Map<number, HistoryDayViewModel>();
    (summary?.days ?? []).forEach((day) => byDay.set(day.dayStart, day));
    let tokens = 0;
    let cost = 0;
    let currency: string | null = null;
    let active = 0;
    for (let offset = 0; offset < 30; offset++) {
      const day = byDay.get(today - offset * 86_400);
      if (!day || !day.hasData) continue;
      tokens += day.tokens;
      if (day.cost !== null && day.cost !== undefined && day.cost > 0) {
        cost += day.cost;
        currency ??= day.currency ?? null;
      }
      if (day.tokens > 0 || (day.cost !== null && day.cost !== undefined && day.cost > 0)) {
        active += 1;
      }
    }
    return {
      tokens,
      cost: cost > 0 ? cost : null,
      currency,
      active,
    };
  }, [summary]);

  const moveFocus = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const deltas: Record<string, [number, number]> = {
      ArrowRight: [1, 0],
      ArrowLeft: [-1, 0],
      ArrowDown: [0, 1],
      ArrowUp: [0, -1],
    };
    const delta = deltas[event.key];
    if (!delta) return;
    event.preventDefault();
    const current = focusIndex ?? grid.todayIndex;
    if (current < 0) return;
    const week = Math.floor(current / 7) + delta[0];
    const weekday = (current % 7) + delta[1];
    if (weekday < 0 || weekday > 6) return;
    const next = week * 7 + weekday;
    const cell = grid.cells[next];
    if (week < 0 || week >= grid.weeks || !cell) return;
    setFocusIndex(next);
    cellRefs.current[next]?.focus();
  };

  const anyData = (summary?.days ?? []).some((day) => day.hasData);
  const costText =
    last30.cost !== null && last30.currency !== null
      ? `${last30.currency}${formatCompact(last30.cost)}`
      : last30.cost !== null
        ? formatCompact(last30.cost)
        : copy.summary.none;

  // 30-day trend series: unknown days stay null so the line shows real gaps.
  const trend = useMemo(() => {
    const byDay = new Map<number, HistoryDayViewModel>();
    (summary?.days ?? []).forEach((day) => byDay.set(day.dayStart, day));
    const today = localMidnightSeconds(new Date());
    const points: Array<TrendPoint> = [];
    for (let offset = 29; offset >= 0; offset--) {
      const dayStart = today - offset * 86_400;
      const day = byDay.get(dayStart);
      const date = new Date(dayStart * 1000);
      points.push({
        label: `${date.getMonth() + 1}/${date.getDate()}`,
        dayStart,
        tokens: day?.hasData ? day.tokens : null,
      });
    }
    return points;
  }, [summary]);
  const peak = Math.max(0, ...trend.map((point) => point.tokens ?? 0));

  return (
    <section className="section history-section" aria-label={copy.a11y.historyRegion}>

      {summary === null ? (
        failed ? (
          <div className="history__failure">
            <p className="section__empty">{copy.history.loadFailed}</p>
            <button className="btn btn--quiet" type="button" onClick={() => void load()}>
              <IconRefresh size={14} />
              {copy.actions.retry}
            </button>
          </div>
        ) : (
          <div className="skeleton" aria-hidden="true">
            <div className="skeleton__line skeleton__line--short" />
            <div className="skeleton__line" />
          </div>
        )
      ) : (
        <>
          {failed && <div className="notice notice--warning">{copy.history.loadFailed}</div>}
          <motion.div
            className="history__layout"
            variants={fadeVariants}
            initial="hidden"
            animate="show"
          >
          <div className="history__stats">
            <div className="history__stat">
              <span>{copy.history.tokens30}</span>
              <strong className="num">{formatCompact(last30.tokens)}</strong>
            </div>
            <div className="history__stat">
              <span>{copy.history.cost30}</span>
              <strong className="num">{costText}</strong>
            </div>
            <div className="history__stat">
              <span>{copy.history.activeDays}</span>
              <strong className="num">
                {last30.active}
                <em>/30</em>
              </strong>
            </div>
          </div>

          {anyData && peak > 0 && (
            <div className="history__trend">
              <div className="history__trend-head">
                <span>{copy.history.trendTitle}</span>
                <span className="num">
                  {copy.history.trendPeak}
                  {formatCompact(peak)}
                </span>
              </div>
              <div className="history__trend-chart">
                <ResponsiveContainer width="100%" height="100%">
                  <AreaChart data={trend} margin={{ top: 4, right: 4, bottom: 0, left: 4 }}>
                    <defs>
                      <linearGradient id="trend-fill" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="0%" stopColor="var(--brand)" stopOpacity={0.32} />
                        <stop offset="100%" stopColor="var(--brand)" stopOpacity={0.02} />
                      </linearGradient>
                    </defs>
                    <XAxis
                      dataKey="label"
                      tick={{ fontSize: 10, fill: "var(--ink-3)" }}
                      axisLine={false}
                      tickLine={false}
                      minTickGap={28}
                    />
                    <YAxis hide domain={[0, "dataMax"]} />
                    <Tooltip
                      cursor={{ stroke: "var(--hairline-strong)" }}
                      content={(props) => {
                        const typed = props as {
                          active?: boolean;
                          payload?: ReadonlyArray<{ payload: TrendPoint }>;
                        };
                        return (
                          <TrendTooltip
                            active={typed.active}
                            payload={typed.payload}
                            breakdown={breakdown}
                            accounts={accounts}
                          />
                        );
                      }}
                    />
                    <Area
                      type="monotone"
                      dataKey="tokens"
                      stroke="var(--brand)"
                      strokeWidth={1.8}
                      fill="url(#trend-fill)"
                      connectNulls={false}
                      dot={false}
                      activeDot={{ r: 3, fill: "var(--brand)" }}
                    />
                  </AreaChart>
                </ResponsiveContainer>
              </div>
            </div>
          )}

          {anyData ? (
            <>
              <div
                className="heatmap__scroll"
                role="group"
                aria-label={copy.a11y.heatmap}
                tabIndex={0}
                onKeyDown={moveFocus}
              >
                <div
                  className="heatmap__grid"
                  style={{
                    gridTemplateColumns: `22px repeat(${grid.weeks}, minmax(12px, 1fr))`,
                    gridTemplateRows: "auto repeat(7, minmax(12px, 1fr))",
                  }}
                >
                  {grid.months.map((month) => (
                    <span
                      key={`${month.week}-${month.label}`}
                      className="heatmap__month"
                      style={{ gridColumn: month.week + 2, gridRow: 1 }}
                    >
                      {month.label}
                    </span>
                  ))}
                  {Object.entries(WEEKDAY_LABELS).map(([row, label]) => (
                    <span
                      key={row}
                      className="heatmap__weekday"
                      style={{ gridColumn: 1, gridRow: Number(row) + 2 }}
                    >
                      {label}
                    </span>
                  ))}
                  {grid.cells.map((cell, index) =>
                    cell === null ? null : (
                      <button
                        key={cell.dayStart}
                        ref={(node) => {
                          cellRefs.current[index] = node;
                        }}
                        type="button"
                        tabIndex={focusIndex === index ? 0 : -1}
                        className={`heat-cell heat-cell--${cell.level}${cell.hasData ? "" : " heat-cell--empty"}`}
                        style={{ gridColumn: Math.floor(index / 7) + 2, gridRow: (index % 7) + 2 }}
                        title={copy.a11y.dayCell(cellDate(cell.dayStart), cellDetail(
                          summary.days.find((day) => day.dayStart === cell.dayStart),
                        ))}
                        onFocus={() => setFocusIndex(index)}
                      >
                        <span className="sr-only">
                          {copy.a11y.dayCell(cellDate(cell.dayStart), cellDetail(
                            summary.days.find((day) => day.dayStart === cell.dayStart),
                          ))}
                        </span>
                      </button>
                    ),
                  )}
                </div>
              </div>
              <div className="heatmap__legend">
                <span>{copy.history.legendLess}</span>
                {[0, 1, 2, 3, 4].map((level) => (
                  <span key={level} className={`heat-cell heat-cell--${level} heat-swatch`} />
                ))}
                <span>{copy.history.legendMore}</span>
                <span className="heatmap__hint">{copy.a11y.heatmapHint}</span>
              </div>
            </>
          ) : (
            <p className="section__empty">{copy.history.empty}</p>
          )}
          <p className="history__note">{copy.history.intro}</p>
          </motion.div>
        </>
      )}
    </section>
  );
}
