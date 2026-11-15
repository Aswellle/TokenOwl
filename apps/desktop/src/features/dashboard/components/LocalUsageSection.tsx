import { useCallback, useEffect, useState } from "react";
import { copy } from "../../../styles/copy";
import { IconRefresh } from "../../../components/icons";
import { formatCompact } from "../../../lib/format";
import type {
  DashboardBridge,
  LocalUsageSourceView,
  LocalUsageSummary,
} from "../../../types";

/**
 * Local agent usage: a separate data domain from the remote quota pipeline.
 * Everything here is opt-in — a source scans only after the user has seen
 * what it reads and turned it on, and disabling a source deletes its stored
 * events. Totals are per token class; no cost is ever estimated.
 */
export function LocalUsageSection({
  bridge,
  reloadSignal,
}: {
  bridge: DashboardBridge;
  reloadSignal: number;
}) {
  const [sources, setSources] = useState<LocalUsageSourceView | null>(null);
  const [summary, setSummary] = useState<LocalUsageSummary | null>(null);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [sourceView, summaryView] = await Promise.all([
        bridge.localUsageSources(),
        bridge.localUsageSummary(30).catch(() => null),
      ]);
      setSources(sourceView);
      setSummary(summaryView);
      setError(null);
    } catch {
      setError(copy.localUsage.loadFailed);
    }
  }, [bridge]);

  useEffect(() => {
    void load();
  }, [load, reloadSignal]);

  const toggle = async (sourceId: string, enabled: boolean) => {
    try {
      await bridge.setLocalUsageSource(sourceId, enabled);
      await load();
    } catch {
      setError(copy.localUsage.toggleFailed);
    }
  };

  const scan = async () => {
    setScanning(true);
    try {
      await bridge.scanLocalUsage();
      await load();
      setError(null);
    } catch {
      setError(copy.localUsage.scanFailed);
    } finally {
      setScanning(false);
    }
  };

  const clearAll = async () => {
    try {
      await bridge.clearLocalUsage();
      await load();
    } catch {
      setError(copy.localUsage.clearFailed);
    }
  };

  return (
    <section className="section local-usage-section" aria-label={copy.localUsage.title}>
      <p className="section__empty">{copy.localUsage.intro}</p>
      {error && (
        <p className="form__error" role="alert">
          {error}
        </p>
      )}
      {sources === null ? (
        <div className="skeleton" aria-hidden="true">
          <div className="skeleton__line skeleton__line--short" />
          <div className="skeleton__line" />
        </div>
      ) : (
        <>
          <ul className="local-usage__sources">
            {sources.sources.map((source) => {
              const enabled = sources.enabled.includes(source.id);
              return (
                <li key={source.id} className="local-usage__source">
                  <label className="local-usage__toggle">
                    <input
                      type="checkbox"
                      checked={enabled}
                      onChange={(event) => void toggle(source.id, event.target.checked)}
                      aria-label={copy.localUsage.toggleFor(source.name)}
                    />
                    <span>
                      <strong>{source.name}</strong>
                      {source.verified ? null : (
                        <em className="local-usage__unverified">
                          {copy.localUsage.unverifiedParser}
                        </em>
                      )}
                    </span>
                  </label>
                  <p className="local-usage__detail">
                    {copy.localUsage.reads(source.directories.join("、"))}
                    <br />
                    {copy.localUsage.collects(source.collected.join("、"))}
                    <br />
                    {copy.localUsage.neverCollects(source.neverCollected.join("、"))}
                  </p>
                </li>
              );
            })}
          </ul>
          <div className="local-usage__actions">
            <button
              className="btn btn--quiet"
              type="button"
              onClick={() => void scan()}
              disabled={scanning || sources.enabled.length === 0}
            >
              <IconRefresh size={14} className={scanning ? "is-spinning" : undefined} />
              {scanning ? copy.localUsage.scanning : copy.localUsage.scan}
            </button>
            <button className="btn btn--quiet" type="button" onClick={() => void clearAll()}>
              {copy.localUsage.clear}
            </button>
          </div>
          {summary && summary.totals.eventCount > 0 ? (
            <dl className="local-usage__totals">
              <div>
                <dt>{copy.localUsage.input}</dt>
                <dd className="num">{formatCompact(summary.totals.inputTokens)}</dd>
              </div>
              <div>
                <dt>{copy.localUsage.output}</dt>
                <dd className="num">{formatCompact(summary.totals.outputTokens)}</dd>
              </div>
              <div>
                <dt>{copy.localUsage.cacheRead}</dt>
                <dd className="num">{formatCompact(summary.totals.cacheReadTokens)}</dd>
              </div>
              <div>
                <dt>{copy.localUsage.cacheWrite}</dt>
                <dd className="num">{formatCompact(summary.totals.cacheWriteTokens)}</dd>
              </div>
            </dl>
          ) : (
            <p className="section__empty">{copy.localUsage.empty}</p>
          )}
          <p className="history__note">{copy.localUsage.domainNote}</p>
        </>
      )}
    </section>
  );
}
