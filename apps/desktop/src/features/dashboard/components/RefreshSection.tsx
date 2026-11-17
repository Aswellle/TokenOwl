import { useCallback, useEffect, useRef, useState } from "react";
import { copy } from "../../../styles/copy";
import type { DashboardBridge, RefreshSettings } from "../../../types";

const CADENCES = [0, 1, 5, 10, 15, 30];

/**
 * Refresh cadence and the opt-in adaptive policy. Both are local
 * preferences: the backend validates the cadence again, and the adaptive
 * mode only moves between half and twice each provider's declared interval.
 */
export function RefreshSection({ bridge }: { bridge: DashboardBridge }) {
  const [settings, setSettings] = useState<RefreshSettings | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(false);
  const [saved, setSaved] = useState(false);
  const savedTimer = useRef<number | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (savedTimer.current !== null) window.clearTimeout(savedTimer.current);
    };
  }, []);

  const load = useCallback(() => {
    bridge
      .getRefreshSettings()
      .then((value) => setSettings(value))
      .catch(() => setError(true));
  }, [bridge]);

  useEffect(() => {
    load();
  }, [load]);

  const apply = async (next: RefreshSettings) => {
    if (!settings || saving) return;
    const previous = settings;
    setSettings(next);
    setSaving(true);
    setError(false);
    try {
      await bridge.setRefreshSettings(next);
      if (!mounted.current) return;
      setSaved(true);
      if (savedTimer.current !== null) window.clearTimeout(savedTimer.current);
      savedTimer.current = window.setTimeout(() => setSaved(false), 4_000);
    } catch {
      if (!mounted.current) return;
      setSettings(previous);
      setError(true);
    } finally {
      if (mounted.current) setSaving(false);
    }
  };

  return (
    <section className="section settings-card refresh-settings" aria-labelledby="refresh-settings-title">
      <div className="settings-card__head">
        <h2 className="settings-card__title" id="refresh-settings-title">
          {copy.refreshSettings.title}
        </h2>
        <p className="settings-card__intro">{copy.refreshSettings.intro}</p>
      </div>
      {settings === null ? (
        error ? (
          <p className="form__error" role="alert">
            {copy.refreshSettings.loadFailed}
          </p>
        ) : (
          <div className="skeleton" aria-hidden="true">
            <div className="skeleton__line skeleton__line--short" />
            <div className="skeleton__line" />
          </div>
        )
      ) : (
        <>
          <div className="setting-row">
            <div className="setting-row__text">
              <label className="setting-row__label" htmlFor="refresh-cadence">
                {copy.refreshSettings.cadence}
              </label>
              <small>{copy.refreshSettings.cadenceHelp}</small>
            </div>
            <span className="setting-row__control">
              <select
                className="field__input"
                id="refresh-cadence"
                value={settings.cadenceMinutes}
                disabled={saving}
                aria-label={copy.refreshSettings.cadence}
                onChange={(event) =>
                  void apply({
                    ...settings,
                    cadenceMinutes: Number(event.currentTarget.value),
                  })
                }
              >
                {CADENCES.map((minutes) => (
                  <option key={minutes} value={minutes}>
                    {minutes === 0
                      ? copy.refreshSettings.auto
                      : copy.refreshSettings.minutes(minutes)}
                  </option>
                ))}
              </select>
            </span>
          </div>
          <label className="setting-row setting-row--toggle">
            <span className="setting-row__text">
              <strong>{copy.refreshSettings.adaptive}</strong>
              <small>{copy.refreshSettings.adaptiveHelp}</small>
            </span>
            <input
              type="checkbox"
              checked={settings.adaptive}
              disabled={saving}
              aria-label={copy.refreshSettings.adaptive}
              onChange={(event) =>
                void apply({ ...settings, adaptive: event.currentTarget.checked })
              }
            />
          </label>
          {error && (
            <p className="form__error" role="alert">
              {copy.refreshSettings.saveFailed}
            </p>
          )}
          {saved && (
            <p className="history__note" role="status">
              {copy.refreshSettings.saved}
            </p>
          )}
        </>
      )}
    </section>
  );
}
