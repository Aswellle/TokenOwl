import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import { motion } from "motion/react";
import { copy } from "../../../styles/copy";
import { formatRelative } from "../../../lib/format";
import type {
  DashboardBridge,
  NotificationKind,
  NotificationPreferencesViewModel,
  NotificationViewModel,
} from "../../../types";

const springPanel = { type: "spring", bounce: 0, duration: 0.36 } as const;

const fadeVariants = {
  hidden: { opacity: 0, y: -6 },
  show: { opacity: 1, y: 0, transition: springPanel },
};

/**
 * A module-scoped relative-time clock: one 30s interval no matter how many
 * labels subscribe, and a tick re-renders only the time labels through
 * `useSyncExternalStore` — never the notification list around them.
 */
const clockListeners = new Set<() => void>();
let clockNow = Date.now();
let clockTimer: number | null = null;

function subscribeClock(listener: () => void): () => void {
  clockListeners.add(listener);
  if (clockTimer === null) {
    clockTimer = window.setInterval(() => {
      clockNow = Date.now();
      clockListeners.forEach((fn) => fn());
    }, 30_000);
  }
  return () => {
    clockListeners.delete(listener);
    if (clockListeners.size === 0 && clockTimer !== null) {
      window.clearInterval(clockTimer);
      clockTimer = null;
    }
  };
}

function useRelativeNow(): number {
  return useSyncExternalStore(
    subscribeClock,
    () => clockNow,
    () => clockNow,
  );
}

function RelativeTime({ at }: { at: number }) {
  const now = useRelativeNow();
  return (
    <time className="notif__time" dateTime={new Date(at).toISOString()}>
      {formatRelative(at, now) ?? ""}
    </time>
  );
}

function severityClass(severity: NotificationViewModel["severity"]): string {
  return `notif__severity notif__severity--${severity.toLowerCase()}`;
}

function isUnread(item: NotificationViewModel): boolean {
  return item.readAt === null || item.readAt === undefined;
}

/** Human label for a notification kind, falling back to the raw value. */
function kindLabel(item: NotificationViewModel): string {
  return copy.notifications.kindLabels[item.kind] ?? item.kind;
}

/** `minutes since midnight` → `HH:MM` for a time input. */
function minuteText(minutes: number): string {
  const clamped = Math.min(1_439, Math.max(0, Math.round(minutes)));
  const hours = Math.floor(clamped / 60);
  return `${String(hours).padStart(2, "0")}:${String(clamped % 60).padStart(2, "0")}`;
}

/** `HH:MM` → minutes since midnight; unparseable input keeps the old value. */
function parseMinuteText(text: string, fallback: number): number {
  const match = /^(\d{1,2}):(\d{2})$/.exec(text.trim());
  if (!match) return fallback;
  const hours = Number(match[1]);
  const minutes = Number(match[2]);
  if (hours > 23 || minutes > 59) return fallback;
  return hours * 60 + minutes;
}

function NotificationItem({
  item,
  onMarkRead,
}: {
  item: NotificationViewModel;
  onMarkRead: (id: string) => void;
}) {
  const unread = isUnread(item);
  return (
    <li className={`notif${unread ? " notif--unread" : " notif--read"}`}>
      <span className="notif__dot" aria-hidden="true" />
      <div className="notif__body">
        <p className="notif__head">
          <strong className="notif__title">{item.title}</strong>
          {/* The kind chip only adds information when the title does not
              already convey it: "额度紧张 额度紧张" is noise, and so is
              "额度即将重置 即将重置". A window alert titled "5 小时窗口额度不足"
              keeps its "窗口额度不足" tag because the title says something
              narrower than the kind. */}
          {!item.title.includes(kindLabel(item)) && (
            <span className="notif__kind">{kindLabel(item)}</span>
          )}
          <span className={severityClass(item.severity)}>
            {copy.notifications.severity[item.severity] ?? item.severity}
          </span>
          <RelativeTime at={item.createdAt} />
        </p>
        <p className="notif__message">{item.message}</p>
      </div>
      {unread && (
        <button
          className="btn btn--quiet notif__read"
          type="button"
          onClick={() => onMarkRead(item.id)}
          aria-label={copy.a11y.markNotificationRead(item.title)}
        >
          {copy.notifications.markRead}
        </button>
      )}
    </li>
  );
}

/**
 * Number fields commit on blur so typing a value does not spam the backend;
 * checkboxes commit immediately.
 */
function NotificationSettings({
  prefs,
  onChange,
}: {
  prefs: NotificationPreferencesViewModel;
  onChange: (next: NotificationPreferencesViewModel) => Promise<void>;
}) {
  const [thresholdText, setThresholdText] = useState(() => String(prefs.lowQuotaThreshold));
  const [balanceText, setBalanceText] = useState(() =>
    prefs.balanceThreshold === null || prefs.balanceThreshold === undefined
      ? ""
      : String(prefs.balanceThreshold),
  );
  const [resetMinutes, setResetMinutes] = useState(() =>
    String(Math.round(prefs.resetSoonWindow / 60)),
  );
  const [horizonMinutes, setHorizonMinutes] = useState(() =>
    String(Math.round(prefs.projectedExhaustionHorizon / 60)),
  );
  const [quietStart, setQuietStart] = useState(() => minuteText(prefs.quietHoursStart));
  const [quietEnd, setQuietEnd] = useState(() => minuteText(prefs.quietHoursEnd));
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Per-field validation feedback. Focusing a field and leaving it unchanged
  // must never save, and an out-of-range entry is named instead of being
  // silently replaced by the stored value.
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});

  useEffect(() => {
    setThresholdText(String(prefs.lowQuotaThreshold));
    setBalanceText(
      prefs.balanceThreshold === null || prefs.balanceThreshold === undefined
        ? ""
        : String(prefs.balanceThreshold),
    );
    setResetMinutes(String(Math.round(prefs.resetSoonWindow / 60)));
    setHorizonMinutes(String(Math.round(prefs.projectedExhaustionHorizon / 60)));
    setQuietStart(minuteText(prefs.quietHoursStart));
    setQuietEnd(minuteText(prefs.quietHoursEnd));
  }, [prefs]);

  useEffect(() => {
    if (!saved) return;
    const timer = window.setTimeout(() => setSaved(false), 2_000);
    return () => window.clearTimeout(timer);
  }, [saved]);

  const commit = async (next: NotificationPreferencesViewModel) => {
    try {
      await onChange(next);
      setError(null);
      setSaved(true);
      return true;
    } catch {
      setError(copy.notifications.saveFailed);
      return false;
    }
  };

  /** Puts every draft back to the persisted value. */
  const resetDrafts = () => {
    setThresholdText(String(prefs.lowQuotaThreshold));
    setBalanceText(
      prefs.balanceThreshold === null || prefs.balanceThreshold === undefined
        ? ""
        : String(prefs.balanceThreshold),
    );
    setResetMinutes(String(Math.round(prefs.resetSoonWindow / 60)));
    setHorizonMinutes(String(Math.round(prefs.projectedExhaustionHorizon / 60)));
    setQuietStart(minuteText(prefs.quietHoursStart));
    setQuietEnd(minuteText(prefs.quietHoursEnd));
  };

  /**
   * Commits the drafts on blur, but only when something actually changed.
   *
   * Focusing a field and tabbing away used to write the stored value back to
   * the backend and flash "已保存", which reads as an edit the user never
   * made. An unchanged draft is now a no-op, an invalid one is reported in
   * place, and a changed one carries exactly the fields that changed.
   */
  const commitNumbers = () => {
    const errors: Record<string, string> = {};
    const parseNumber = (text: string): number | null => {
      const trimmed = text.trim();
      if (trimmed === "") return null;
      const value = Number(trimmed);
      return Number.isFinite(value) ? value : null;
    };

    const threshold = parseNumber(thresholdText);
    if (threshold === null || threshold < 1 || threshold > 99) {
      errors.threshold = copy.notifications.invalidThreshold;
    }
    const balance = parseNumber(balanceText);
    if (balanceText.trim() !== "" && (balance === null || balance < 0)) {
      errors.balance = copy.notifications.invalidBalance;
    }
    const reset = parseNumber(resetMinutes);
    if (reset === null || reset < 5 || reset > 1_440) {
      errors.reset = copy.notifications.invalidResetWindow;
    }
    const horizon = parseNumber(horizonMinutes);
    if (horizon === null || horizon < 0 || horizon > 1_440) {
      errors.horizon = copy.notifications.invalidHorizon;
    }

    if (Object.keys(errors).length > 0) {
      setFieldErrors(errors);
      return;
    }
    setFieldErrors({});

    const next: NotificationPreferencesViewModel = {
      ...prefs,
      lowQuotaThreshold: threshold as number,
      balanceThreshold: balance,
      resetSoonWindow: Math.round((reset as number) * 60),
      projectedExhaustionHorizon: Math.round((horizon as number) * 60),
      quietHoursStart: parseMinuteText(quietStart, prefs.quietHoursStart),
      quietHoursEnd: parseMinuteText(quietEnd, prefs.quietHoursEnd),
    };

    // Deep comparison against what is already stored: an untouched field (or
    // a cleared optional field that was already empty) is not an edit.
    const unchanged =
      next.lowQuotaThreshold === prefs.lowQuotaThreshold &&
      (next.balanceThreshold ?? null) === (prefs.balanceThreshold ?? null) &&
      next.resetSoonWindow === prefs.resetSoonWindow &&
      next.projectedExhaustionHorizon === prefs.projectedExhaustionHorizon &&
      next.quietHoursStart === prefs.quietHoursStart &&
      next.quietHoursEnd === prefs.quietHoursEnd;
    if (unchanged) {
      // Normalize whatever was typed back to the stored spelling (for
      // example "20.0" → "20") without touching the backend.
      resetDrafts();
      return;
    }

    // On success the persisted values flow back through `prefs`, and the
    // effect above re-syncs every draft. Resetting here from this closure
    // would use the pre-save snapshot and visibly revert the field the user
    // just edited (the IPC round-trip is slower than the optimistic render).
    void commit(next);
  };

  // The collapsed disclosure still answers the two questions a user has:
  // is alerting on, and will it actually pop up now? Quiet hours suppress
  // popups whenever start and end differ (the same instant means off).
  const quietActive = prefs.quietHoursStart !== prefs.quietHoursEnd;
  const stateSummary = !prefs.enabled
    ? copy.notifications.settingsSummaryDisabled
    : quietActive
      ? `${copy.notifications.settingsSummaryEnabled} · ${copy.notifications.settingsSummaryQuiet(
          minuteText(prefs.quietHoursStart),
          minuteText(prefs.quietHoursEnd),
        )}`
      : copy.notifications.settingsSummaryEnabled;

  return (
    <details className="notif-settings">
      {/* Native disclosure: the summary keeps its platform semantics while the
          chevron and the state summary give the collapsed row a visible
          affordance and hint at what is inside. */}
      <summary>
        <span className="notif-settings__chev" aria-hidden="true" />
        <strong className="notif-settings__title">{copy.notifications.settingsTitle}</strong>
        <span className="notif-settings__state">{stateSummary}</span>
      </summary>
      <div className="notif-settings__body">
        {/* Master switch: the only control that stays live while alerting is
            off, so it sits outside the fieldset that mutes the rest. */}
        <label className="setting-row setting-row--toggle">
          <span className="setting-row__text">
            <strong>{copy.notifications.enabled}</strong>
            <small>{copy.notifications.enabledHelp}</small>
          </span>
          <input
            type="checkbox"
            checked={prefs.enabled}
            onChange={(event) => void commit({ ...prefs, enabled: event.currentTarget.checked })}
          />
        </label>
        <fieldset className="notif-settings__fields" disabled={!prefs.enabled}>
          <div className="settings-group">
            <h3 className="settings-group__title">{copy.notifications.groupDelivery}</h3>
            <p className="settings-group__intro">{copy.notifications.groupDeliveryIntro}</p>
            <label className="setting-row setting-row--toggle">
              <span className="setting-row__text">
                <strong>{copy.notifications.silentMode}</strong>
                <small>{copy.notifications.silentModeHelp}</small>
              </span>
              <input
                type="checkbox"
                checked={prefs.silentMode}
                onChange={(event) =>
                  void commit({ ...prefs, silentMode: event.currentTarget.checked })
                }
              />
            </label>
            <label className="setting-row setting-row--toggle">
              <span className="setting-row__text">
                <strong>{copy.notifications.criticalOnly}</strong>
                {/* The threshold is configurable, so the example names the
                    current value instead of a frozen number. */}
                <small>{copy.notifications.criticalOnlyHelp(prefs.lowQuotaThreshold)}</small>
              </span>
              <input
                type="checkbox"
                checked={prefs.criticalOnly}
                onChange={(event) =>
                  void commit({ ...prefs, criticalOnly: event.currentTarget.checked })
                }
              />
            </label>
          </div>

          <div className="settings-group">
            <h3 className="settings-group__title">{copy.notifications.groupConditions}</h3>
            <p className="settings-group__intro">{copy.notifications.groupConditionsIntro}</p>
            <label className="setting-row">
              <span className="setting-row__text">
                <strong>{copy.notifications.lowQuotaThreshold}</strong>
                <small>{copy.notifications.lowQuotaHelp}</small>
                {fieldErrors.threshold && (
                  <small className="setting-row__error" role="alert">
                    {fieldErrors.threshold}
                  </small>
                )}
              </span>
              <span className="setting-row__control">
                <input
                  type="number"
                  min={1}
                  max={99}
                  value={thresholdText}
                  onChange={(event) => setThresholdText(event.currentTarget.value)}
                  onBlur={commitNumbers}
                  aria-label={`${copy.notifications.lowQuotaThreshold}（${copy.notifications.lowQuotaSuffix}）`}
                  aria-invalid={fieldErrors.threshold ? true : undefined}
                />
                <em>{copy.notifications.lowQuotaSuffix}</em>
              </span>
            </label>
            <label className="setting-row">
              <span className="setting-row__text">
                <strong>{copy.notifications.balanceThreshold}</strong>
                <small>{copy.notifications.balanceThresholdHelp}</small>
                {fieldErrors.balance && (
                  <small className="setting-row__error" role="alert">
                    {fieldErrors.balance}
                  </small>
                )}
              </span>
              <span className="setting-row__control">
                <input
                  type="number"
                  min={0}
                  value={balanceText}
                  placeholder="—"
                  onChange={(event) => setBalanceText(event.currentTarget.value)}
                  onBlur={commitNumbers}
                  aria-label={copy.notifications.balanceThreshold}
                  aria-invalid={fieldErrors.balance ? true : undefined}
                />
              </span>
            </label>
          </div>

          <div className="settings-group">
            <h3 className="settings-group__title">{copy.notifications.groupTiming}</h3>
            <p className="settings-group__intro">{copy.notifications.groupTimingIntro}</p>
            <label className="setting-row">
              <span className="setting-row__text">
                <strong>{copy.notifications.resetSoonWindow}</strong>
                <small>{copy.notifications.resetSoonHelp}</small>
                {fieldErrors.reset && (
                  <small className="setting-row__error" role="alert">
                    {fieldErrors.reset}
                  </small>
                )}
              </span>
              <span className="setting-row__control">
                <input
                  type="number"
                  min={5}
                  max={1440}
                  value={resetMinutes}
                  onChange={(event) => setResetMinutes(event.currentTarget.value)}
                  onBlur={commitNumbers}
                  aria-label={`${copy.notifications.resetSoonWindow}（${copy.notifications.resetSoonSuffix}）`}
                  aria-invalid={fieldErrors.reset ? true : undefined}
                />
                <em>{copy.notifications.resetSoonSuffix}</em>
              </span>
            </label>
            <label className="setting-row">
              <span className="setting-row__text">
                <strong>{copy.notifications.projectedHorizon}</strong>
                <small>{copy.notifications.projectedHorizonHelp}</small>
                {fieldErrors.horizon && (
                  <small className="setting-row__error" role="alert">
                    {fieldErrors.horizon}
                  </small>
                )}
              </span>
              <span className="setting-row__control">
                <input
                  type="number"
                  min={0}
                  max={1440}
                  value={horizonMinutes}
                  onChange={(event) => setHorizonMinutes(event.currentTarget.value)}
                  onBlur={commitNumbers}
                  aria-label={`${copy.notifications.projectedHorizon}（${copy.notifications.projectedHorizonSuffix}）`}
                  aria-invalid={fieldErrors.horizon ? true : undefined}
                />
                <em>{copy.notifications.projectedHorizonSuffix}</em>
              </span>
            </label>
          </div>

          <div className="settings-group">
            <h3 className="settings-group__title">{copy.notifications.groupQuiet}</h3>
            <p className="settings-group__intro">{copy.notifications.groupQuietIntro}</p>
            <label className="setting-row">
              <span className="setting-row__text">
                <strong>{copy.notifications.quietHours}</strong>
                <small>{copy.notifications.quietHoursHelp}</small>
              </span>
              <span className="setting-row__control notif-range">
                <input
                  type="time"
                  value={quietStart}
                  onChange={(event) => setQuietStart(event.currentTarget.value)}
                  onBlur={commitNumbers}
                  aria-label={`${copy.notifications.quietHours} ${copy.notifications.quietStart}`}
                />
                <em aria-hidden="true">–</em>
                <input
                  type="time"
                  value={quietEnd}
                  onChange={(event) => setQuietEnd(event.currentTarget.value)}
                  onBlur={commitNumbers}
                  aria-label={`${copy.notifications.quietHours} ${copy.notifications.quietEnd}`}
                />
              </span>
            </label>
          </div>
        </fieldset>
        {error && (
          <p className="notif-settings__error" role="alert">
            {error}
          </p>
        )}
        {!error && saved && (
          <p className="notif-settings__saved" role="status">
            {copy.notifications.saved}
          </p>
        )}
      </div>
    </details>
  );
}

export function NotificationSection({
  bridge,
  reloadSignal,
}: {
  bridge: DashboardBridge;
  reloadSignal: number;
}) {
  const [items, setItems] = useState<NotificationViewModel[] | null>(null);
  const [prefs, setPrefs] = useState<NotificationPreferencesViewModel | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const [prefsFailed, setPrefsFailed] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  // Type filter: the log is the only place a user can audit *what kind* of
  // alerting their setup produces, and after a few weeks it holds every kind.
  const [kindFilter, setKindFilter] = useState<NotificationKind | "all">("all");

  const load = useCallback(async () => {
    // The list and the preferences degrade independently: a preferences
    // failure must not hide the notification history.
    const results = await Promise.allSettled([
      bridge.listNotifications(50),
      bridge.getNotificationPreferences(),
    ]);
    const [list, loadedPrefs] = results;
    if (list.status === "fulfilled") {
      setItems(list.value);
      setLoadFailed(false);
    } else {
      setItems((current) => current ?? []);
      setLoadFailed(true);
    }
    if (loadedPrefs.status === "fulfilled") {
      setPrefs(loadedPrefs.value);
      setPrefsFailed(false);
    } else {
      setPrefsFailed(true);
    }
  }, [bridge]);

  useEffect(() => {
    void load();
  }, [load, reloadSignal]);

  const unread = (items ?? []).filter(isUnread).length;
  // Only offer the kinds actually present, so the control never lists a filter
  // that would always come back empty.
  const presentKinds = Array.from(new Set((items ?? []).map((item) => item.kind)));
  const visibleItems =
    kindFilter === "all"
      ? items
      : (items ?? []).filter((item) => item.kind === kindFilter);

  const handleMarkRead = async (id: string) => {
    setActionError(null);
    try {
      await bridge.markNotificationsRead([id]);
      void load();
    } catch {
      setActionError(copy.notifications.markReadFailed);
    }
  };

  const handleMarkAllRead = async () => {
    setActionError(null);
    try {
      await bridge.markNotificationsRead();
      void load();
    } catch {
      setActionError(copy.notifications.markReadFailed);
    }
  };

  return (
    <section className="section notif-section" aria-label={copy.a11y.notificationsRegion}>
      <div className="section__head">
        <div className="section__titlewrap">
          {unread > 0 && <span className="notif__badge num">{copy.notifications.unreadSuffix(unread)}</span>}
        </div>
        <div className="section__actions">
          {presentKinds.length > 1 && (
            <label className="notif-filter">
              <span className="sr-only">{copy.notifications.kindFilter}</span>
              <select
                value={kindFilter}
                onChange={(event) =>
                  setKindFilter(event.currentTarget.value as NotificationKind | "all")
                }
                aria-label={copy.notifications.kindFilter}
              >
                <option value="all">{copy.notifications.kindAll}</option>
                {presentKinds.map((kind) => (
                  <option key={kind} value={kind}>
                    {copy.notifications.kindLabels[kind] ?? kind}
                  </option>
                ))}
              </select>
            </label>
          )}
          {unread > 0 && (
            <button
              className="btn btn--quiet"
              type="button"
              onClick={() => void handleMarkAllRead()}
              aria-label={copy.a11y.markAllNotificationsRead}
            >
              {copy.notifications.markAllRead}
            </button>
          )}
        </div>
      </div>

      {actionError && (
        <p className="notif-section__error" role="alert">
          {actionError}
        </p>
      )}
      {loadFailed && (items?.length ?? 0) > 0 && (
        <div className="notice notice--warning">{copy.notifications.loadFailed}</div>
      )}
      {prefsFailed && !prefs && (
        <div className="notice notice--warning">{copy.notifications.preferencesLoadFailed}</div>
      )}

      {loadFailed && (items?.length ?? 0) === 0 ? (
        <div className="notif__failure">
          <p className="section__empty">{copy.notifications.loadFailed}</p>
          <button className="btn btn--quiet" type="button" onClick={() => void load()}>
            {copy.actions.retry}
          </button>
        </div>
      ) : items === null ? (
        <div className="skeleton" aria-hidden="true">
          <div className="skeleton__line skeleton__line--short" />
          <div className="skeleton__line" />
        </div>
      ) : items.length === 0 ? (
        <motion.p
          key="notifications-empty"
          className="section__empty"
          variants={fadeVariants}
          initial="hidden"
          animate="show"
        >
          {copy.notifications.empty}
        </motion.p>
      ) : visibleItems && visibleItems.length === 0 ? (
        <motion.p
          key="notifications-filtered-empty"
          className="section__empty"
          variants={fadeVariants}
          initial="hidden"
          animate="show"
        >
          {copy.notifications.filteredEmpty}
        </motion.p>
      ) : (
        <ul className="notifs">
          {(visibleItems ?? []).map((item) => (
            <NotificationItem
              key={item.id}
              item={item}
              onMarkRead={(id) => void handleMarkRead(id)}
            />
          ))}
        </ul>
      )}

      {prefs && (
        <NotificationSettings
          prefs={prefs}
          onChange={async (next) => {
            // Optimistic, but honest: a rejected save rolls the form back to
            // the persisted values so the UI never shows an unsaved state as
            // if it were saved.
            const previous = prefs;
            setPrefs(next);
            try {
              await bridge.setNotificationPreferences(next);
            } catch (error) {
              setPrefs(previous);
              throw error;
            }
          }}
        />
      )}
    </section>
  );
}
