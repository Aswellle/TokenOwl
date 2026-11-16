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
    <section className="section refresh-settings" aria-labelledby="refresh-settings-title">
      <div className="section__titlewrap">
        <h2 className="section__title" id="refresh-settings-title">
          {copy.refreshSettings.title}
        </h2>
      </div>
      <p className="section__intro">{copy.refreshSettings.intro}</p>
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
          <div className="refresh-settings__row">
            <label className="field__label" htmlFor="refresh-cadence">
              {copy.refreshSettings.cadence}
            </label>
            <select
              className="field__input"
              id="refresh-cadence"
              value={settings.cadenceMinutes}
              disabled={saving}
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
          </div>
          <label className="refresh-settings__toggle">
            <input
              type="checkbox"
              checked={settings.adaptive}
              disabled={saving}
              onChange={(event) =>
                void apply({ ...settings, adaptive: event.currentTarget.checked })
              }
            />
            <span>
              <strong>{copy.refreshSettings.adaptive}</strong>
              <em className="refresh-settings__help">{copy.refreshSettings.adaptiveHelp}</em>
            </span>
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
