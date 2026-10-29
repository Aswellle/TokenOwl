import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion, useMotionValueEvent, useReducedMotion, useSpring } from "motion/react";
import { listen } from "@tauri-apps/api/event";
import "../../styles/App.css";
import { copy } from "../../styles/copy";
import {
  IconAlert,
  IconBubble,
  IconCheck,
  IconCircleUser,
  IconClock,
  IconClose,
  IconDownload,
  IconGauge,
  IconGear,
  IconGitHub,
  IconGrid,
  IconMinus,
  IconOwl,
  IconPlus,
  IconRefresh,
  IconShield,
} from "../../components/icons";
import { StatusChip } from "./components/StatusChip";
import { NotificationSection } from "./components/NotificationSection";
import { HistorySection } from "./components/HistorySection";
import { ProviderView } from "./components/ProviderView";
import { ProviderMatrix } from "./components/ProviderMatrix";
import { SetupForm } from "./components/SetupForm";
import { AccountRow } from "./components/AccountRow";
import { Notice } from "./components/Notice";
import { useAccounts } from "./hooks/useAccounts";
import { useUpdater } from "./hooks/useUpdater";
import { ProviderIcon } from "../../components/ProviderIcon";
import {
  accountResetAt,
  formatResetShort,
  overallHealth,
  pressure,
  ratioTone,
  timestamp,
} from "../../lib/format";
import { isTauri, shell, tauriBridge } from "../../lib/tauri";
import { clipCircle, cornerRadius, onMorphToBubble, useMorphFromBubble } from "../../lib/morph";
import type { MorphAnchor } from "../../lib/morph";
import type {
  DashboardBridge,
} from "../../types";

type FilterId = "all" | "attention";
type ViewId = "overview" | "provider" | "history" | "notifications" | "settings";

const VIEW_ORDER: ViewId[] = ["overview", "provider", "history", "notifications", "settings"];
/** Provider icons the rail can show before the rest live in the overview. */
const RAIL_MAX = 5;

/* ---------- motion vocabulary (critically damped, Apple-style) ---------- */

const springPanel = { type: "spring", bounce: 0, duration: 0.42 } as const;


const fadeVariants = {
  hidden: { opacity: 0, y: -6 },
  show: { opacity: 1, y: 0, transition: springPanel },
  hide: { opacity: 0, y: -4, transition: { duration: 0.16, ease: [0.23, 1, 0.32, 1] as const } },
};

/* ---------- primitives ---------- */

function AnimatedNumber({ value, format }: { value: number; format?: (value: number) => string }) {
  const reduce = useReducedMotion();
  const [display, setDisplay] = useState(value);
  const spring = useSpring(value, { bounce: 0, duration: 0.55 });

  useEffect(() => {
    if (reduce) spring.jump(value);
    else spring.set(value);
  }, [spring, value, reduce]);
  useMotionValueEvent(spring, "change", (latest) => setDisplay(latest as number));

  return <>{format ? format(display) : String(Math.round(display))}</>;
}



function TrafficLights() {
  return (
    <div className="lights">
      <button
        className="light light--close"
        type="button"
        aria-label={copy.titlebar.close}
        onClick={() => void shell.hideDashboard().catch(() => null)}
      >
        <IconClose size={9} strokeWidth={2.2} />
      </button>
      <button
        className="light light--min"
        type="button"
        aria-label={copy.titlebar.minimize}
        onClick={() => void shell.minimizeDashboard().catch(() => null)}
      >
        <IconMinus size={9} strokeWidth={2.2} />
      </button>
    </div>
  );
}

function Segmented({
  value,
  onChange,
  options,
}: {
  value: FilterId;
  onChange: (id: FilterId) => void;
  options: Array<{ id: FilterId; label: string; count: number }>;
}) {
  return (
    <div className="seg" role="group" aria-label={copy.a11y.filterAccounts}>
      {options.map((option) => (
        <button
          key={option.id}
          className={`seg__btn${value === option.id ? " is-active" : ""}`}
          type="button"
          aria-pressed={value === option.id}
          onClick={() => onChange(option.id)}
        >
          {value === option.id && (
            <motion.span
              className="seg__pill"
              layoutId="seg-pill"
              transition={{ type: "spring", bounce: 0, duration: 0.32 }}
            />
          )}
          <span className="seg__label">
            {option.label}
            <span className="seg__count num">{option.count}</span>
          </span>
        </button>
      ))}
    </div>
  );
}

function PressureRing({ ratio, tone }: { ratio: number | null; tone: "healthy" | "attention" | "critical" }) {
  const reduce = useReducedMotion();
  const radius = 15;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference * (1 - (ratio ?? 0));
  return (
    <span className={`ring ring--${tone}`} aria-hidden="true">
      <svg viewBox="0 0 36 36">
        <circle className="ring__track" cx="18" cy="18" r={radius} />
        <motion.circle
          className="ring__value"
          cx="18"
          cy="18"
          r={radius}
          strokeDasharray={circumference}
          animate={{ strokeDashoffset: offset }}
          transition={reduce ? { duration: 0 } : springPanel}
        />
      </svg>
    </span>
  );
}

function EmptyState({
  offline,
  onAdd,
  onRetry,
}: {
  offline: boolean;
  onAdd: () => void;
  onRetry: () => void;
}) {
  return (
    <section className="empty" aria-live="polite">
      <span className={`empty__mark${offline ? " empty__mark--offline" : ""}`} aria-hidden="true">
        <IconOwl size={40} strokeWidth={1.4} />
      </span>
      <h3 className="empty__title">
        {offline ? copy.notices.offlineTitle : copy.quota.firstRunTitle}
      </h3>
      <p className="empty__body">{offline ? copy.notices.offlineBody : copy.form.intro}</p>
      <div className="empty__actions">
        {offline ? (
          <button className="btn btn--primary" type="button" onClick={onRetry}>
            <IconRefresh size={15} />
            {copy.actions.retry}
          </button>
        ) : (
          <button className="btn btn--primary" type="button" onClick={onAdd}>
            <IconPlus size={15} />
            {copy.actions.addAccount}
          </button>
        )}
      </div>
    </section>
  );
}

/* ---------- app ---------- */

export interface AppProps {
  bridge?: DashboardBridge;
}

export function App({ bridge = tauriBridge }: AppProps = {}) {
  const viewRef = useRef<ViewId>("overview");
  const [view, setView] = useState<ViewId>("overview");
  const [direction, setDirection] = useState(1);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filter, setFilter] = useState<FilterId>("all");
  const {
    accounts,
    phase,
    notice,
    refreshingId,
    deletingId,
    formError,
    formSubmitting,
    loadAccounts,
    handleRefresh,
    handleCreate,
    handleDelete,
    setNotice,
  } = useAccounts(bridge);
  const { updateState, banner, dismissBanner, handleUpdateCheck, handleInstallUpdate } =
    useUpdater(bridge, setNotice);
  const [now, setNow] = useState(() => Date.now());
  const [notificationTick, setNotificationTick] = useState(0);
  const [historyTick, setHistoryTick] = useState(0);
  const reduce = useReducedMotion();
  const overallStatus = overallHealth(accounts, phase);

  // Expansion origin, reported by Rust when the dashboard was opened from the
  // bubble. Null means "opened some other way" and the window just appears.
  const anchor = useMorphFromBubble();
  // Collapse target, reported when the bubble takes over; the dashboard
  // shrinks into that circle and only then hides (the reverse handoff).
  const [collapseAnchor, setCollapseAnchor] = useState<MorphAnchor | null>(null);
  // True once the expand reveal has landed: the clip circle then yields to a
  // full-cover one, so later window resizes are never masked by a stale
  // fixed-radius circle.
  const [revealDone, setRevealDone] = useState(false);
  const revealRadius = useMemo(() => {
    if (!anchor) return 0;
    const width = window.innerWidth || 760;
    const height = window.innerHeight || 720;
    // +2% so the final circle clears the corners even if the window is
    // resized between the anchor being measured and the animation running.
    return cornerRadius(anchor.x, anchor.y, width, height) * 1.02;
  }, [anchor]);
  // Full-cover clip for the idle state: always a circle (so the collapse can
  // interpolate from it), recomputed on resize because a stale one would cut
  // the corners of a grown window.
  const [viewport, setViewport] = useState(() => ({
    w: window.innerWidth || 760,
    h: window.innerHeight || 720,
  }));
  useEffect(() => {
    const onResize = () => setViewport({ w: window.innerWidth, h: window.innerHeight });
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  const idleClip = clipCircle(
    { x: viewport.w / 2, y: viewport.h / 2 },
    cornerRadius(viewport.w / 2, viewport.h / 2, viewport.w, viewport.h),
  );

  useEffect(() => {
    if (!isTauri()) return;
    return onMorphToBubble((incoming) => setCollapseAnchor(incoming));
  }, []);

  // A fresh expand anchor always restarts the reveal. The bubble is topmost,
  // so it would sit over the growing panel for the whole animation; the
  // reveal covers the bubble's spot well before the tween ends, so the
  // handoff hides it early (finish is idempotent).
  useEffect(() => {
    if (!anchor) return;
    setRevealDone(false);
    const timer = window.setTimeout(() => {
      void shell.finishBubbleHandoff().catch(() => null);
    }, 140);
    return () => window.clearTimeout(timer);
  }, [anchor]);

  // Mirror handoff for the collapse: once the shrinking circle is near the
  // bubble's size, the panel's remaining tail is invisible under the bubble
  // (topmost, already on screen), so the panel hides before the tween ends.
  useEffect(() => {
    if (!collapseAnchor) return;
    const timer = window.setTimeout(() => {
      void shell.finishDashboardHandoff().catch(() => null);
    }, 230);
    return () => window.clearTimeout(timer);
  }, [collapseAnchor]);

  const goView = useCallback((next: ViewId) => {
    const from = VIEW_ORDER.indexOf(viewRef.current);
    const to = VIEW_ORDER.indexOf(next);
    setDirection(to >= from ? 1 : -1);
    viewRef.current = next;
    setView(next);
  }, []);

  // Escape walks back from a provider's detail view like a desktop app's
  // Back. Dialogs stopPropagation, so closing them never also navigates.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && viewRef.current === "provider") {
        goView("overview");
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [goView]);

  const openProvider = (accountId: string) => {
    setSelectedId(accountId);
    goView("provider");
  };

  useEffect(() => {
    let active = true;
    const unlisten = listen("quota:updated", () => {
      if (!active) return;
      setHistoryTick((tick) => tick + 1);
      void loadAccounts();
    }).catch(() => null);
    return () => {
      active = false;
      void unlisten.then((dispose) => dispose?.());
    };
  }, [bridge]);

  useEffect(() => {
    let active = true;
    const unlisten = listen("notification:enqueue", () => {
      if (active) setNotificationTick((tick) => tick + 1);
    }).catch(() => null);
    return () => {
      active = false;
      void unlisten.then((dispose) => dispose?.());
    };
  }, []);

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, []);

  const summary = useMemo(() => {
    const healthy = accounts.filter((account) => account.status === "healthy").length;
    const resetAt = accounts
      .map(accountResetAt)
      .map(timestamp)
      .filter((value): value is number => value !== null)
      .sort((a, b) => a - b)[0];
    const ratios = accounts
      .map((account) => pressure(account.snapshots))
      .filter((value): value is number => value !== null);
    const lastSync = accounts
      .map((account) => timestamp(account.lastUpdatedAt))
      .filter((value): value is number => value !== null)
      .sort((a, b) => b - a)[0];
    return {
      healthy,
      resetAt,
      lastSync,
      pressure: ratios.length > 0 ? Math.min(...ratios) : null,
    };
  }, [accounts]);

  const pressureTone = ratioTone(summary.pressure) ?? "healthy";
  const attentionCount = accounts.filter(
    (account) => account.status !== "healthy" && account.status !== "syncing",
  ).length;
  const railAccounts = accounts.slice(0, RAIL_MAX);
  const selected = accounts.find((account) => account.id === selectedId) ?? null;
  const visibleAccounts =
    filter === "all" ? accounts : accounts.filter((account) => account.status !== "healthy" && account.status !== "syncing");


  const viewVariants = {
    hidden: (dir: number) =>
      reduce ? { opacity: 0 } : { opacity: 0, x: dir * 28 },
    show: { opacity: 1, x: 0, transition: springPanel },
    hide: (dir: number) =>
      reduce
        ? { opacity: 0, transition: { duration: 0.12 } }
        : { opacity: 0, x: dir * -20, transition: { duration: 0.18, ease: [0.23, 1, 0.32, 1] as const } },
  };

  const dockItems: Array<{ id: ViewId; label: string; icon: typeof IconGauge }> = [
    { id: "overview", label: copy.views.overview, icon: IconGauge },
    { id: "history", label: copy.views.history, icon: IconClock },
    { id: "notifications", label: copy.views.notifications, icon: IconAlert },
    { id: "settings", label: copy.views.settings, icon: IconGear },
  ];

  return (
    <motion.div
      className="window"
      // The desktop window grows out of the bubble's circle; the bubble is
      // still on screen underneath until the early handoff lands, so the two
      // surfaces overlap and there is never a blank frame between them.
      // Keyframes (from → to) make both directions replay on every anchor:
      // expand always grows out of the bubble's small circle, collapse always
      // pulls the full panel back into it — a plain animate target would
      // interpolate from whatever was last painted and skip the path.
      initial={reduce || !anchor ? false : { clipPath: clipCircle(anchor, anchor.diameter / 2), opacity: 0.35 }}
      animate={
        collapseAnchor
          ? {
              clipPath: [idleClip, clipCircle(collapseAnchor, collapseAnchor.diameter / 2)],
              opacity: [1, 0.35],
            }
          : anchor && !revealDone
            ? {
                clipPath: [clipCircle(anchor, anchor.diameter / 2), clipCircle(anchor, revealRadius)],
                opacity: [0.35, 1],
              }
            : { clipPath: idleClip, opacity: 1 }
      }
      transition={
        collapseAnchor
          ? reduce
            ? { duration: 0.12 }
            : { duration: 0.32, ease: [0.32, 0.72, 0, 1] }
          : anchor && !revealDone
            ? reduce
              ? { duration: 0 }
              : { duration: 0.32, ease: [0.32, 0.72, 0, 1] }
            : { duration: 0 }
      }
      onAnimationComplete={() => {
        if (!isTauri()) return;
        if (collapseAnchor) {
          setCollapseAnchor(null);
          void shell.finishDashboardHandoff().catch(() => null);
          return;
        }
        if (anchor && !revealDone) {
          setRevealDone(true);
          void shell.finishBubbleHandoff().catch(() => null);
        }
      }}
    >
      <header className="titlebar" data-tauri-drag-region>
        <TrafficLights />
        <div className="titlebar__brand">
          <span className="titlebar__owl" aria-hidden="true">
            <IconOwl size={19} />
          </span>
          <span className="titlebar__name">{copy.brand}</span>
          <h1 className="titlebar__title" aria-live="polite">
            {view === "provider" && selected ? selected.label : copy.views[view]}
          </h1>
        </div>
        <div className="titlebar__spacer" data-tauri-drag-region />
        <div className="titlebar__actions">
          <StatusChip status={overallStatus} />
          <button
            className="pill"
            type="button"
            onClick={() => {
              if (!isTauri()) {
                // Plain-browser preview: morph to the bubble in this document.
                window.dispatchEvent(new CustomEvent("tokenowl:show-bubble"));
                return;
              }
              void shell.showBubble().catch(() => null);
            }}
            title={copy.titlebar.bubbleHint}
          >
            <IconBubble size={17} />
            {copy.titlebar.bubble}
          </button>
        </div>
      </header>

      {accounts.length > 0 && (
        <nav className="rail" aria-label={copy.a11y.providerRail}>
          <div className="rail__track">
            {railAccounts.map((account) => (
              <button
                key={account.id}
                type="button"
                className={`rail__item${view === "provider" && selectedId === account.id ? " is-active" : ""}`}
                aria-pressed={view === "provider" && selectedId === account.id}
                aria-label={copy.a11y.selectProvider(`${account.label}（${copy.health[account.status] ?? account.status}）`)}
                title={`${account.label} · ${copy.health[account.status] ?? account.status}`}
                onClick={() => openProvider(account.id)}
              >
                <ProviderIcon providerId={account.providerId} size={28} />
                <span className={`rail__dot rail__dot--${account.status}`} aria-hidden="true" />
              </button>
            ))}
            {accounts.length > RAIL_MAX && (
              <button
                type="button"
                className="rail__hint"
                aria-label={copy.rail.maxHint}
                title={copy.rail.maxHint}
                onClick={() => goView("overview")}
              >
                +{accounts.length - RAIL_MAX}
              </button>
            )}
            <button
              type="button"
              className={`rail__item rail__item--all${view === "overview" ? " is-active" : ""}`}
              aria-pressed={view === "overview"}
              aria-label={copy.rail.allProviders}
              title={copy.rail.allProviders}
              onClick={() => goView("overview")}
            >
              <IconGrid size={17} />
            </button>
          </div>
        </nav>
      )}

      <div className="content">
        <AnimatePresence initial={false}>
          {notice && !(phase === "offline" && accounts.length === 0) && (
            <Notice key="notice" tone={notice.tone}>
              {notice.text}
            </Notice>
          )}
        </AnimatePresence>

        <AnimatePresence initial={false} mode="wait" custom={direction}>
          <motion.div
            key={view + (view === "provider" ? selectedId ?? "" : "")}
            className="view"
            variants={viewVariants}
            custom={direction}
            initial="hidden"
            animate="show"
            exit="hide"
          >
            {view === "overview" && (
              <>
                <section className="stats" aria-label={copy.a11y.summaryRegion}>
                  <div className="stat">
                    <span className="stat__icon stat__icon--pressure">
                      <IconGauge size={15} />
                    </span>
                    <span className="stat__label">{copy.summary.pressure}</span>
                    <span className="stat__main">
                      <PressureRing ratio={summary.pressure} tone={pressureTone} />
                      <strong className="stat__value num">
                        {summary.pressure !== null ? (
                          <AnimatedNumber
                            value={summary.pressure}
                            format={(value) => `${Math.round(value * 100)}%`}
                          />
                        ) : (
                          copy.summary.none
                        )}
                      </strong>
                    </span>
                    <span className="stat__hint">
                      {summary.pressure !== null ? copy.summary.knownWindow : copy.summary.waitingData}
                    </span>
                  </div>
                  <div className="stat">
                    <span className="stat__icon">
                      <IconCircleUser size={15} />
                    </span>
                    <span className="stat__label">{copy.summary.accounts}</span>
                    <strong className="stat__value num">
                      {accounts.length > 0 ? <AnimatedNumber value={accounts.length} /> : copy.summary.none}
                    </strong>
                    <span className="stat__hint">
                      {accounts.length === 0 ? copy.summary.notConnected : copy.summary.configuredLocally}
                    </span>
                  </div>
                  <div className="stat">
                    <span className="stat__icon stat__icon--ok">
                      <IconCheck size={15} />
                    </span>
                    <span className="stat__label">{copy.summary.healthy}</span>
                    <strong className="stat__value num">
                      {accounts.length > 0 ? <AnimatedNumber value={summary.healthy} /> : copy.summary.none}
                    </strong>
                    <span className="stat__hint">
                      {accounts.length > 0
                        ? copy.summary.needsAttention(attentionCount)
                        : copy.summary.waitingData}
                    </span>
                  </div>
                  <div className="stat">
                    <span className="stat__icon stat__icon--clock">
                      <IconClock size={15} />
                    </span>
                    <span className="stat__label">{copy.summary.nextReset}</span>
                    <strong className="stat__value">
                      {summary.resetAt ? (formatResetShort(summary.resetAt, now) ?? copy.summary.none) : copy.summary.none}
                    </strong>
                    <span className="stat__hint">
                      {summary.resetAt ? copy.summary.knownWindow : copy.summary.unknown}
                    </span>
                  </div>
                </section>

                <section className="section" aria-label={copy.a11y.accountsRegion}>
                <div className="section__head section__head--end">
                  <div className="section__actions">
                    {accounts.length > 0 && (
                      <>
                        <Segmented
                          value={filter}
                          onChange={setFilter}
                          options={[
                            { id: "all", label: copy.filter.all, count: accounts.length },
                            { id: "attention", label: copy.filter.attention, count: attentionCount },
                          ]}
                        />
                        <button
                          className="btn btn--icon"
                          type="button"
                          onClick={() => void handleRefresh()}
                          disabled={refreshingId !== null}
                          aria-label={copy.actions.refreshAll}
                          title={refreshingId === "all" ? copy.actions.refreshing : copy.actions.refreshAll}
                        >
                          <IconRefresh size={16} className={refreshingId === "all" ? "is-spinning" : undefined} />
                        </button>
                      </>
                    )}
                  </div>
                </div>

                {phase === "loading" && accounts.length === 0 ? (
                  <div aria-label={copy.a11y.loadingAccounts}>
                    {[0, 1].map((row) => (
                      <div className="skeleton" key={row}>
                        <div className="skeleton__line skeleton__line--short" />
                        <div className="skeleton__line" />
                        <div className="skeleton__line" />
                      </div>
                    ))}
                  </div>
                ) : accounts.length === 0 ? (
                  <EmptyState
                    offline={phase === "offline"}
                    onAdd={() => goView("settings")}
                    onRetry={() => void loadAccounts()}
                  />
                ) : visibleAccounts.length === 0 ? (
                  <motion.p key="filtered-empty" className="section__empty" variants={fadeVariants} initial="hidden" animate="show">
                    {copy.filter.attentionEmpty}
                  </motion.p>
                ) : (
                  <ul className="accounts">
                    <AnimatePresence initial={false} mode="popLayout">
                      {visibleAccounts.map((account, index) => (
                        <AccountRow
                          key={account.id}
                          account={account}
                          index={index}
                          now={now}
                          refreshing={refreshingId === account.id || refreshingId === "all"}
                          deleting={deletingId === account.id}
                          onOpen={() => openProvider(account.id)}
                          onRefresh={() => void handleRefresh(account.id)}
                          onDelete={() =>
                            void handleDelete(account.id, () => {
                              if (selectedId === account.id) goView("overview");
                            })
                          }
                        />
                      ))}
                    </AnimatePresence>
                  </ul>
                )}
                </section>
              </>
            )}

            {view === "provider" && selected && (
              <ProviderView
                account={selected}
                now={now}
                refreshing={refreshingId === selected.id || refreshingId === "all"}
                bridge={bridge}
                onBack={() => goView("overview")}
                onRefresh={() => void handleRefresh(selected.id)}
              />
            )}

            {view === "history" && (
              <HistorySection bridge={bridge} accounts={accounts} reloadSignal={historyTick} />
            )}

            {view === "notifications" && (
              <NotificationSection bridge={bridge} reloadSignal={notificationTick} />
            )}

            {view === "settings" && (
              <section className="section" aria-label={copy.views.settings}>
                <SetupForm
                  bridge={bridge}
                  submitting={formSubmitting}
                  error={formError}
                  onCancel={() => goView("overview")}
                  onSubmit={handleCreate}
                />
                <ProviderMatrix bridge={bridge} />
                {/* The check lives at the bottom of a long page, so the result
                    renders here — right above the button that produced it —
                    and never at the top, where it would land off-screen. */}
                {banner?.kind === "available" && (
                  <motion.div
                    className="notice notice--accent"
                    role="status"
                    variants={fadeVariants}
                    initial="hidden"
                    animate="show"
                    exit="hide"
                  >
                    <span className="notice__icon">
                      <IconDownload size={14} />
                    </span>
                    <span>{copy.update.available(banner.version)}</span>
                    <span className="notice__actions">
                      <button
                        className="btn btn--accent-outline"
                        type="button"
                        onClick={() => void handleInstallUpdate()}
                      >
                        {copy.actions.downloadUpdate}
                      </button>
                      <button
                        className="btn btn--quiet"
                        type="button"
                        aria-label={copy.actions.dismissBanner}
                        title={copy.actions.dismissBanner}
                        onClick={dismissBanner}
                      >
                        <IconClose size={14} />
                      </button>
                    </span>
                  </motion.div>
                )}
                {banner?.kind === "current" && (
                  <motion.div
                    variants={fadeVariants}
                    initial="hidden"
                    animate="show"
                    exit="hide"
                  >
                    <Notice>
                      {copy.update.currentDetail}
                      <span className="notice__actions">
                        <button
                          className="btn btn--quiet"
                          type="button"
                          aria-label={copy.actions.dismissBanner}
                          title={copy.actions.dismissBanner}
                          onClick={dismissBanner}
                        >
                          <IconClose size={14} />
                        </button>
                      </span>
                    </Notice>
                  </motion.div>
                )}
                {banner?.kind === "failed" && (
                  <motion.div
                    variants={fadeVariants}
                    initial="hidden"
                    animate="show"
                    exit="hide"
                  >
                    <Notice tone="warning">
                      {copy.update.unavailable}
                      <span className="notice__actions">
                        <button
                          className="btn btn--quiet"
                          type="button"
                          aria-label={copy.actions.dismissBanner}
                          title={copy.actions.dismissBanner}
                          onClick={dismissBanner}
                        >
                          <IconClose size={14} />
                        </button>
                      </span>
                    </Notice>
                  </motion.div>
                )}
                <div className="settings__about">
                  <button className="btn btn--quiet" type="button" onClick={() => void handleUpdateCheck()} disabled={updateState === "checking"}>
                    <IconRefresh size={14} className={updateState === "checking" ? "is-spinning" : undefined} />
                    {updateState === "checking" ? copy.actions.checkingUpdate : copy.actions.checkUpdate}
                  </button>
                  <button
                    className="btn btn--quiet"
                    type="button"
                    onClick={() => void shell.openProjectPage().catch(() => null)}
                  >
                    <IconGitHub size={15} />
                    GitHub
                  </button>
                  <span className="settings__note">
                    <IconShield size={14} />
                    {copy.notices.localOnly}
                  </span>
                </div>
              </section>
            )}
          </motion.div>
        </AnimatePresence>
      </div>

      <nav className="dock" aria-label={copy.a11y.dockRegion}>
        {dockItems.map((item) => (
          <button
            key={item.id}
            type="button"
            className={`dock__item${view === item.id ? " is-active" : ""}`}
            aria-pressed={view === item.id}
            aria-label={item.label}
            onClick={() => goView(item.id)}
          >
            {view === item.id && (
              <motion.span
                className="dock__pill"
                layoutId="dock-pill"
                transition={{ type: "spring", bounce: 0, duration: 0.32 }}
              />
            )}
            <item.icon size={19} />
            <span className="dock__label">{item.label}</span>
          </button>
        ))}
        <span className="dock__spacer" aria-hidden="true" />
        <button
          type="button"
          className="dock__item dock__item--icon"
          aria-label={copy.a11y.openGithub}
          title={copy.a11y.openGithub}
          onClick={() => void shell.openProjectPage().catch(() => null)}
        >
          <IconGitHub size={19} />
        </button>
      </nav>
    </motion.div>
  );
}

export default App;
