import { useCallback, useEffect, useState } from "react";
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
  now,
  onMarkRead,
}: {
  item: NotificationViewModel;
  now: number;
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
          <time className="notif__time" dateTime={new Date(item.createdAt).toISOString()}>
            {formatRelative(item.createdAt, now) ?? ""}
          </time>
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
    } catch {
      setError(copy.notifications.saveFailed);
    }
  };

  const commitNumbers = () => {
    const threshold = Number(thresholdText);
    const balance = balanceText.trim() === "" ? null : Number(balanceText);
    const minutes = Number(resetMinutes);
    const horizon = Number(horizonMinutes);
    void commit({
      ...prefs,
      lowQuotaThreshold: Number.isFinite(threshold) ? threshold : prefs.lowQuotaThreshold,
      balanceThreshold:
        balance === null || !Number.isFinite(balance) || balance < 0
          ? null
          : balance,
      resetSoonWindow: Number.isFinite(minutes) && minutes > 0
        ? Math.round(minutes * 60)
        : prefs.resetSoonWindow,
      projectedExhaustionHorizon:
        Number.isFinite(horizon) && horizon >= 0
          ? Math.round(horizon * 60)
          : prefs.projectedExhaustionHorizon,
      quietHoursStart: parseMinuteText(quietStart, prefs.quietHoursStart),
      quietHoursEnd: parseMinuteText(quietEnd, prefs.quietHoursEnd),
    });
  };

  return (
    <details className="notif-settings">
      <summary>{copy.notifications.settingsTitle}</summary>
      <div className="notif-settings__body">
        <label className="notif-check">
          <input
            type="checkbox"
            checked={prefs.enabled}
            onChange={(event) => void commit({ ...prefs, enabled: event.currentTarget.checked })}
          />
          <span>
            <strong>{copy.notifications.enabled}</strong>
            <small>{copy.notifications.enabledHelp}</small>
          </span>
        </label>
        <fieldset className="notif-settings__fields" disabled={!prefs.enabled}>
          <label className="notif-check">
            <input
              type="checkbox"
              checked={prefs.silentMode}
              onChange={(event) =>
                void commit({ ...prefs, silentMode: event.currentTarget.checked })
              }
            />
            <span>
              <strong>{copy.notifications.silentMode}</strong>
              <small>{copy.notifications.silentModeHelp}</small>
            </span>
          </label>
          <label className="notif-check">
            <input
              type="checkbox"
              checked={prefs.criticalOnly}
              onChange={(event) =>
                void commit({ ...prefs, criticalOnly: event.currentTarget.checked })
              }
            />
            <span>
              <strong>{copy.notifications.criticalOnly}</strong>
              <small>{copy.notifications.criticalOnlyHelp}</small>
            </span>
          </label>
          <div className="notif-numbers">
            <label className="notif-number">
              <span>{copy.notifications.lowQuotaThreshold}</span>
              <span className="notif-number__input">
                <input
                  type="number"
                  min={1}
                  max={99}
                  value={thresholdText}
                  onChange={(event) => setThresholdText(event.currentTarget.value)}
                  onBlur={commitNumbers}
                  aria-label={`${copy.notifications.lowQuotaThreshold}（${copy.notifications.lowQuotaSuffix}）`}
                />
                <em>{copy.notifications.lowQuotaSuffix}</em>
              </span>
            </label>
            <label className="notif-number">
              <span>{copy.notifications.balanceThreshold}</span>
              <span className="notif-number__input">
                <input
                  type="number"
                  min={0}
                  value={balanceText}
                  placeholder="—"
                  onChange={(event) => setBalanceText(event.currentTarget.value)}
                  onBlur={commitNumbers}
                  aria-label={copy.notifications.balanceThreshold}
                />
              </span>
              <small>{copy.notifications.balanceThresholdHelp}</small>
            </label>
            <label className="notif-number">
              <span>{copy.notifications.resetSoonWindow}</span>
              <span className="notif-number__input">
                <input
                  type="number"
                  min={5}
                  max={1440}
                  value={resetMinutes}
                  onChange={(event) => setResetMinutes(event.currentTarget.value)}
                  onBlur={commitNumbers}
                  aria-label={`${copy.notifications.resetSoonWindow}（${copy.notifications.resetSoonSuffix}）`}
                />
                <em>{copy.notifications.resetSoonSuffix}</em>
              </span>
            </label>
            <label className="notif-number">
              <span>{copy.notifications.projectedHorizon}</span>
              <span className="notif-number__input">
                <input
                  type="number"
                  min={0}
                  max={1440}
                  value={horizonMinutes}
                  onChange={(event) => setHorizonMinutes(event.currentTarget.value)}
                  onBlur={commitNumbers}
                  aria-label={`${copy.notifications.projectedHorizon}（${copy.notifications.projectedHorizonSuffix}）`}
                />
                <em>{copy.notifications.projectedHorizonSuffix}</em>
              </span>
              <small>{copy.notifications.projectedHorizonHelp}</small>
            </label>
            <label className="notif-number">
              <span>{copy.notifications.quietHours}</span>
              <span className="notif-number__input notif-number__input--time">
                <input
                  type="time"
                  value={quietStart}
                  onChange={(event) => setQuietStart(event.currentTarget.value)}
                  onBlur={commitNumbers}
                  aria-label={`${copy.notifications.quietHours} ${copy.notifications.quietStart}`}
                />
                <em>–</em>
                <input
                  type="time"
                  value={quietEnd}
                  onChange={(event) => setQuietEnd(event.currentTarget.value)}
                  onBlur={commitNumbers}
                  aria-label={`${copy.notifications.quietHours} ${copy.notifications.quietEnd}`}
                />
              </span>
              <small>{copy.notifications.quietHoursHelp}</small>
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
  const [now, setNow] = useState(() => Date.now());
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
    } else {
      setItems((current) => current ?? []);
    }
    if (loadedPrefs.status === "fulfilled") {
      setPrefs(loadedPrefs.value);
    }
  }, [bridge]);

  useEffect(() => {
    void load();
  }, [load, reloadSignal]);

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, []);

  const unread = (items ?? []).filter(isUnread).length;
  // Only offer the kinds actually present, so the control never lists a filter
  // that would always come back empty.
  const presentKinds = Array.from(new Set((items ?? []).map((item) => item.kind)));
  const visibleItems =
    kindFilter === "all"
      ? items
      : (items ?? []).filter((item) => item.kind === kindFilter);

  const handleMarkRead = async (id: string) => {
    await bridge.markNotificationsRead([id]);
    void load();
  };

  const handleMarkAllRead = async () => {
    await bridge.markNotificationsRead();
    void load();
  };

  return (
    <section className="section notif-section" aria-label={copy.a11y.notificationsRegion}>
      <div className="section__head">
        <div className="section__titlewrap">
          {items && items.length > 0 && (
            <span className="section__count num">{items.length}</span>
          )}
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

      {items === null ? (
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
          {copy.notifications.empty}
        </motion.p>
      ) : (
        <ul className="notifs">
          {(visibleItems ?? []).map((item) => (
            <NotificationItem
              key={item.id}
              item={item}
              now={now}
              onMarkRead={(id) => void handleMarkRead(id)}
            />
          ))}
        </ul>
      )}

      {prefs && (
        <NotificationSettings
          prefs={prefs}
          onChange={async (next) => {
            setPrefs(next);
            await bridge.setNotificationPreferences(next);
          }}
        />
      )}
    </section>
  );
}
