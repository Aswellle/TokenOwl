import { copy } from "../../../styles/copy";
import { IconClock } from "../../../components/icons";
import {
  displaySource,
  formatCountdown,
  formatValue,
  quotaRatio,
  ratioTone,
} from "../../../lib/format";
import type { QuotaViewModel } from "../../../types";

function QuotaValues({ quota }: { quota: QuotaViewModel }) {
  const hasValue =
    (quota.remaining !== null && quota.remaining !== undefined) ||
    (quota.limit !== null && quota.limit !== undefined) ||
    (quota.used !== null && quota.used !== undefined);
  if (!hasValue) return null;
  return (
    <dl className="quota__values">
      {quota.remaining !== null && quota.remaining !== undefined && (
        <div className="quota__value">
          <dt>{copy.quota.remaining}</dt>
          <dd>{formatValue(quota.remaining, quota)}</dd>
        </div>
      )}
      {quota.limit !== null && quota.limit !== undefined && (
        <div className="quota__value quota__value--muted">
          <dt>{copy.quota.limit}</dt>
          <dd>{formatValue(quota.limit, quota)}</dd>
        </div>
      )}
      {quota.used !== null && quota.used !== undefined && (
        <div className="quota__value quota__value--muted">
          <dt>{copy.quota.used}</dt>
          <dd>{formatValue(quota.used, quota)}</dd>
        </div>
      )}
    </dl>
  );
}

/** One quota metric card: name, values, intensity bar, reset countdown. */
export function QuotaMetric({ quota, now }: { quota: QuotaViewModel; now: number }) {
  const ratio = quotaRatio(quota);
  const tone = ratioTone(ratio);
  const percent = ratio === null ? null : `${Math.round(ratio * 100)}%`;
  const resetText = formatCountdown(quota.window?.resetAt ?? quota.window?.endsAt, now);
  const source = displaySource(quota.source);
  const unit = quota.unit && quota.unit !== "unknown" ? (copy.units[quota.unit] ?? quota.unit) : null;
  // Endpoint kind ids ("quota"/"usage"/"balance") localize through copy.metrics;
  // provider-supplied metric names pass through untouched.
  const metricName = copy.metrics[quota.metricName] ?? quota.metricName;
  const windowLabel = quota.window?.type
    ? (copy.windowTypes[quota.window.type] ?? quota.window.type)
    : null;
  const state = quota.status ?? "available";
  return (
    <li className="quota">
      <div className="quota__head">
        <h4 className="quota__name">
          {metricName}
          {unit && <span className="quota__unit">{unit}</span>}
        </h4>
        {windowLabel && <span className="quota__window">{windowLabel}</span>}
      </div>
      <QuotaValues quota={quota} />
      {ratio !== null && tone && (
        <div
          className={`bar bar--${tone}`}
          role="img"
          aria-label={copy.a11y.quotaBar(metricName, percent ?? "")}
        >
          <span className="bar__fill" style={{ transform: `scaleX(${ratio})` }} />
        </div>
      )}
      <div className="quota__foot">
        {resetText && (
          <span className="quota__reset">
            <IconClock size={14} />
            {resetText}
          </span>
        )}
        {ratio !== null && <span className="bar__value num">{percent}</span>}
        {source && <span>{source}</span>}
        {state === "partial" && <span className="quota__state">{copy.quota.partial}</span>}
        {state === "unavailable" && <span className="quota__state">{copy.quota.notReported}</span>}
        {state === "error" && (
          <span className="quota__state quota__state--error">{copy.quota.readError}</span>
        )}
      </div>
    </li>
  );
}
