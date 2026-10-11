import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
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
  IconKey,
  IconMinus,
  IconOwl,
  IconPlus,
  IconRefresh,
  IconShield,
  IconTrash,
} from "../../components/icons";
import { StatusChip } from "./components/StatusChip";
import { NotificationSection } from "./components/NotificationSection";
import { HistorySection } from "./components/HistorySection";
import { ProviderView } from "./components/ProviderView";
import { ProviderMatrix } from "./components/ProviderMatrix";
import { QuotaMetric } from "./components/QuotaMetric";
import { ProviderIcon } from "../../components/ProviderIcon";
import {
  accountResetAt,
  displaySource,
  formatRelative,
  formatResetShort,
  overallHealth,
  pressure,
  ratioTone,
  timestamp,
} from "../../lib/format";
import { shell, tauriBridge } from "../../lib/tauri";
import type {
  AccountViewModel,
  DashboardBridge,
  GenericOpenAiAccountInput,
  ManifestProfileViewModel,
  UpdateInfo,
} from "../../types";

type LoadPhase = "loading" | "ready" | "offline";
type ActionState = "idle" | "checking" | "available" | "current";
type FilterId = "all" | "attention";
type ViewId = "overview" | "provider" | "history" | "notifications" | "settings";

const VIEW_ORDER: ViewId[] = ["overview", "provider", "history", "notifications", "settings"];
/** Provider icons the rail can show before the rest live in the overview. */
const RAIL_MAX = 5;

/* ---------- motion vocabulary (critically damped, Apple-style) ---------- */

const springPanel = { type: "spring", bounce: 0, duration: 0.42 } as const;
const springItem = { type: "spring", bounce: 0, duration: 0.36 } as const;

const listVariants = {
  hidden: { opacity: 0, y: 10, scale: 0.99 },
  show: ({ index, reduce }: { index: number; reduce: boolean }) => ({
    opacity: 1,
    y: 0,
    scale: 1,
    transition: reduce
      ? { duration: 0 }
      : { ...springItem, delay: Math.min(index * 0.045, 0.24) },
  }),
  hide: { opacity: 0, scale: 0.96, transition: { duration: 0.14, ease: [0.23, 1, 0.32, 1] as const } },
};

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


function Notice({
  tone,
  icon,
  children,
}: {
  tone?: "warning" | "danger" | "accent";
  icon?: boolean;
  children: ReactNode;
}) {
  const Icon = tone === "danger" || tone === "warning" ? IconAlert : IconShield;
  return (
    <motion.p
      className={`notice${tone ? ` notice--${tone}` : ""}`}
      role="status"
      variants={fadeVariants}
      initial="hidden"
      animate="show"
      exit="hide"
    >
      {icon !== false && (
        <span className="notice__icon">
          <Icon size={14} />
        </span>
      )}
      <span>{children}</span>
    </motion.p>
  );
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

function AccountRow({
  account,
  index,
  now,
  refreshing,
  deleting,
  onRefresh,
  onDelete,
}: {
  account: AccountViewModel;
  index: number;
  now: number;
  refreshing: boolean;
  deleting: boolean;
  onRefresh: () => void;
  onDelete: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const reduce = useReducedMotion();
  const relative = formatRelative(account.lastUpdatedAt ?? account.snapshots[0]?.observedAt, now);
  const source = displaySource(account.source ?? account.snapshots[0]?.source);
  const hasSnapshots = account.snapshots.length > 0;
  const freshness = copy.freshness[account.freshness] ?? copy.freshness.unknown;

  return (
    <motion.li
      className={`account${deleting ? " account--busy" : ""}`}
      custom={{ index, reduce }}
      variants={listVariants}
      initial={reduce ? false : "hidden"}
      animate="show"
      exit="hide"
      layout={!confirming}
    >
      <div className="account__head">
        <div className="account__identity">
          <ProviderIcon providerId={account.providerId} size={32} />
          <div className="account__names">
            <h3 className="account__label" title={account.label}>
              {account.label}
            </h3>
            <p className="account__provider">{account.providerName}</p>
          </div>
        </div>
        <div className="account__tools">
          <StatusChip status={account.status} />
          <button
            className="btn btn--icon"
            type="button"
            onClick={onRefresh}
            disabled={refreshing || deleting}
            aria-label={copy.a11y.refreshAccount(account.label)}
            title={copy.actions.refresh}
          >
            <IconRefresh size={18} className={refreshing ? "is-spinning" : undefined} />
          </button>
          <button
            className="btn btn--icon"
            type="button"
            onClick={() => setConfirming(true)}
            disabled={refreshing || deleting}
            aria-label={copy.a11y.removeAccount(account.label)}
            title={copy.actions.remove}
          >
            <IconTrash size={18} />
          </button>
        </div>
      </div>

      {account.status === "error" && hasSnapshots && (
        <Notice tone="danger">{copy.notices.accountError}</Notice>
      )}
      {account.status === "stale" && hasSnapshots && (
        <Notice tone="warning">{copy.notices.accountStale}</Notice>
      )}

      {hasSnapshots ? (
        <ul className="quotas">
          {account.snapshots.map((quota) => (
            <QuotaMetric key={quota.id} quota={quota} now={now} />
          ))}
        </ul>
      ) : (
        <div className="notice notice--inline">
          <span className="notice__icon">
            <IconAlert size={14} />
          </span>
          <span>
            {account.status === "unavailable" ? copy.quota.emptyUnavailable : copy.quota.emptyBody}
          </span>
        </div>
      )}

      <div className="account__foot">
        <span>
          {freshness}
          {relative ? ` · ${copy.time.updated(relative)}` : ` · ${copy.freshness.neverSynced}`}
          {source ? ` · ${source}` : ""}
        </span>
        <button className="btn btn--quiet" type="button" onClick={onRefresh} disabled={refreshing || deleting}>
          <IconRefresh size={14} className={refreshing ? "is-spinning" : undefined} />
          {refreshing ? copy.actions.refreshing : copy.actions.refresh}
        </button>
      </div>

      <AnimatePresence initial={false}>
        {confirming && (
          <motion.div
            className="confirm"
            role="alertdialog"
            aria-label={copy.actions.confirmRemove}
            initial={{ opacity: 0, y: -4, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1, transition: springPanel }}
            exit={{ opacity: 0, scale: 0.98, transition: { duration: 0.14 } }}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.stopPropagation();
                setConfirming(false);
              }
            }}
          >
            <p className="confirm__text">{copy.removeConfirm(account.label)}</p>
            <div className="confirm__actions">
              <button className="btn btn--quiet" type="button" onClick={() => setConfirming(false)}>
                {copy.actions.cancel}
              </button>
              <button
                className="btn btn--danger"
                type="button"
                autoFocus
                onClick={() => {
                  setConfirming(false);
                  onDelete();
                }}
              >
                {copy.actions.confirmRemove}
              </button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.li>
  );
}

function SetupForm({
  bridge,
  submitting,
  error,
  onCancel,
  onSubmit,
}: {
  bridge: DashboardBridge;
  submitting: boolean;
  error: string | null;
  onCancel: () => void;
  onSubmit: (input: Omit<GenericOpenAiAccountInput, "apiKey"> & { apiKeyInput: HTMLInputElement }) => Promise<void>;
}) {
  const [profiles, setProfiles] = useState<ManifestProfileViewModel[]>([]);
  const [profileId, setProfileId] = useState("custom");
  const [label, setLabel] = useState("");
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  const [quotaEndpoint, setQuotaEndpoint] = useState("");
  const [usageEndpoint, setUsageEndpoint] = useState("");
  const [balanceEndpoint, setBalanceEndpoint] = useState("");
  const apiKeyRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let active = true;
    bridge
      .manifestProfiles()
      .then((list) => {
        if (active) setProfiles(list);
      })
      .catch(() => null);
    return () => {
      active = false;
    };
  }, [bridge]);

  const profile = profiles.find((item) => item.id === profileId) ?? null;
  const isCustom = profile === null;
  const effectiveBaseUrl = profile ? profile.baseUrl : baseUrl;
  const endpointChips = profile
    ? Object.entries(profile.endpoints)
        .filter(([, path]) => path !== null && path !== undefined)
        .map(([kind]) => copy.form.endpointLabels[kind] ?? kind)
    : [];

  const submit = async (event: { preventDefault(): void }) => {
    event.preventDefault();
    const apiKeyInput = apiKeyRef.current;
    if (!apiKeyInput || !apiKeyInput.value.trim()) return;
    await onSubmit({
      providerId: profile ? profile.id : "generic.openai_compatible",
      label: label.trim(),
      baseUrl: (profile?.customBaseUrl ? baseUrl : effectiveBaseUrl).trim(),
      quotaEndpoint: profile ? undefined : quotaEndpoint.trim() || undefined,
      usageEndpoint: profile ? undefined : usageEndpoint.trim() || undefined,
      balanceEndpoint: profile ? undefined : balanceEndpoint.trim() || undefined,
      apiKeyInput,
    });
  };

  return (
    <motion.section
      className="form"
      aria-labelledby="setup-title"
      variants={fadeVariants}
      initial="hidden"
      animate="show"
      exit="hide"
      onSubmit={submit}
    >
      <div className="form__head">
        <h2 className="form__title" id="setup-title">
          {copy.form.title}
        </h2>
        <button className="btn btn--quiet" type="button" onClick={onCancel}>
          {copy.actions.cancel}
        </button>
      </div>
      <p className="form__intro">{copy.form.intro}</p>
      {error && (
        <p className="form__error" role="alert">
          {error}
        </p>
      )}
      <div className="picker" role="group" aria-label={copy.a11y.profilePicker}>
        <span className="picker__heading" aria-hidden="true">
          {copy.form.protocolOpenai}
        </span>
        <div className="picker__row">
          {profiles
            .filter((item) => item.apiFormat === "openai-compatible")
            .map((item) => (
              <button
                key={item.id}
                type="button"
                className={`picker__btn${profileId === item.id ? " is-active" : ""}`}
                aria-pressed={profileId === item.id}
                onClick={() => setProfileId(item.id)}
              >
                <ProviderIcon providerId={item.id} size={18} />
                <span>{item.name}</span>
              </button>
            ))}
          <button
            type="button"
            className={`picker__btn${isCustom ? " is-active" : ""}`}
            aria-pressed={isCustom}
            onClick={() => setProfileId("custom")}
          >
            <ProviderIcon providerId="generic.openai_compatible" size={18} />
            <span>{copy.form.customOption}</span>
          </button>
        </div>
        {profiles.some((item) => item.apiFormat !== "openai-compatible") && (
          <>
            <span className="picker__heading" aria-hidden="true">
              {copy.form.protocolOther}
            </span>
            <div className="picker__row">
              {profiles
                .filter((item) => item.apiFormat !== "openai-compatible")
                .map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    className="picker__btn picker__btn--planned"
                    disabled
                    title={copy.form.protocolOther}
                  >
                    <ProviderIcon providerId={item.id} size={18} />
                    <span>{item.name}</span>
                  </button>
                ))}
            </div>
          </>
        )}
      </div>
      <div className="form__grid">
        <div className="field field--wide">
          <label className="field__label" htmlFor="account-label">
            {copy.form.label}
          </label>
          <input
            className="field__input"
            id="account-label"
            value={label}
            onChange={(event) => setLabel(event.currentTarget.value)}
            placeholder={copy.form.labelPlaceholder}
            autoComplete="off"
            required
          />
        </div>
        <div className="field field--wide">
          <label className="field__label" htmlFor="base-url">
            {copy.form.baseUrl}
          </label>
          <input
            className="field__input mono"
            id="base-url"
            type="url"
            value={effectiveBaseUrl}
            onChange={(event) => setBaseUrl(event.currentTarget.value)}
            readOnly={!isCustom && !profile?.customBaseUrl}
            placeholder={profile?.customBaseUrl ? copy.form.gatewayBaseUrl : copy.form.baseUrlPlaceholder}
            autoComplete="off"
            spellCheck={false}
            required
          />
          <span className="field__help">
            {isCustom
              ? copy.form.baseUrlHelp
              : profile?.customBaseUrl
                ? copy.form.gatewayBaseUrlHelp
                : copy.form.builtinNote}
          </span>
        </div>
        {isCustom && (
          <>
            <div className="field field--wide">
              <label className="field__label" htmlFor="quota-endpoint">
                {copy.form.quotaEndpoint}
              </label>
              <input
                className="field__input mono"
                id="quota-endpoint"
                type="text"
                value={quotaEndpoint}
                onChange={(event) => setQuotaEndpoint(event.currentTarget.value)}
                placeholder={copy.form.quotaEndpointPlaceholder}
                pattern="/.*"
                title={copy.form.quotaEndpointHelp}
                spellCheck={false}
              />
              <span className="field__help">{copy.form.quotaEndpointHelp}</span>
            </div>
            <div className="field field--wide">
              <label className="field__label" htmlFor="usage-endpoint">
                {copy.form.usageEndpoint}
              </label>
              <input
                className="field__input mono"
                id="usage-endpoint"
                type="text"
                value={usageEndpoint}
                onChange={(event) => setUsageEndpoint(event.currentTarget.value)}
                placeholder={copy.form.usageEndpointPlaceholder}
                pattern="/.*"
                title={copy.form.usageEndpointHelp}
                spellCheck={false}
              />
              <span className="field__help">{copy.form.usageEndpointHelp}</span>
            </div>
            <div className="field field--wide">
              <label className="field__label" htmlFor="balance-endpoint">
                {copy.form.balanceEndpoint}
              </label>
              <input
                className="field__input mono"
                id="balance-endpoint"
                type="text"
                value={balanceEndpoint}
                onChange={(event) => setBalanceEndpoint(event.currentTarget.value)}
                placeholder={copy.form.balanceEndpointPlaceholder}
                pattern="/.*"
                title={copy.form.balanceEndpointHelp}
                spellCheck={false}
              />
              <span className="field__help">{copy.form.balanceEndpointHelp}</span>
            </div>
          </>
        )}
        {!isCustom && (
          <div className="field field--wide">
            <span className="field__label">{copy.form.endpointsAvailable}</span>
            <div className="picker__chips">
              {endpointChips.length > 0 ? (
                endpointChips.map((chip) => (
                  <span key={chip} className="cap-chip cap-chip--l3">
                    {chip}
                  </span>
                ))
              ) : (
                <span className="field__help">{copy.form.customNote}</span>
              )}
            </div>
          </div>
        )}
        <div className="field field--wide">
          <label className="field__label" htmlFor="api-key">
            {copy.form.apiKey}
          </label>
          <input
            className="field__input secret-input"
            id="api-key"
            ref={apiKeyRef}
            type="text"
            autoComplete="off"
            aria-describedby="api-key-help"
            placeholder={copy.form.apiKeyPlaceholder}
            spellCheck={false}
            required
          />
          <span className="field__help" id="api-key-help">
            {copy.form.apiKeyHelp}
          </span>
        </div>
      </div>
      <div className="form__footer">
        <span className="form__note">
          <IconKey size={14} />
          {copy.form.credentialsNote}
        </span>
        <button className="btn btn--primary" type="submit" disabled={submitting}>
          {submitting ? copy.actions.submitting : copy.actions.submit}
        </button>
      </div>
    </motion.section>
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
        {offline ? copy.notices.offlineTitle : copy.quota.emptyTitle}
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
  const loadAccountsRequestId = useRef(0);
  const viewRef = useRef<ViewId>("overview");
  const [view, setView] = useState<ViewId>("overview");
  const [direction, setDirection] = useState(1);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [accounts, setAccounts] = useState<AccountViewModel[]>([]);
  const [phase, setPhase] = useState<LoadPhase>("loading");
  const [filter, setFilter] = useState<FilterId>("all");
  const [refreshingId, setRefreshingId] = useState<string | "all" | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [formSubmitting, setFormSubmitting] = useState(false);
  const [notice, setNotice] = useState<{ text: string; tone?: "warning" | "danger" | "accent" } | null>(null);
  const [updateState, setUpdateState] = useState<ActionState>("idle");
  const [pendingUpdate, setPendingUpdate] = useState<UpdateInfo | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [notificationTick, setNotificationTick] = useState(0);
  const [historyTick, setHistoryTick] = useState(0);
  const reduce = useReducedMotion();
  const overallStatus = overallHealth(accounts, phase);

  const goView = (next: ViewId) => {
    const from = VIEW_ORDER.indexOf(viewRef.current);
    const to = VIEW_ORDER.indexOf(next);
    setDirection(to >= from ? 1 : -1);
    viewRef.current = next;
    setView(next);
  };

  const openProvider = (accountId: string) => {
    setSelectedId(accountId);
    goView("provider");
  };

  const loadAccounts = async () => {
    const requestId = ++loadAccountsRequestId.current;
    setPhase("loading");
    try {
      const result = await bridge.listAccounts();
      if (requestId !== loadAccountsRequestId.current) return;
      setAccounts(result);
      setPhase("ready");
      setNotice(null);
    } catch {
      if (requestId !== loadAccountsRequestId.current) return;
      setPhase("offline");
      setNotice({ text: copy.notices.loadFailed, tone: "warning" });
    }
  };

  useEffect(() => {
    let active = true;
    bridge
      .listAccounts()
      .then((loaded) => {
        if (!active) return;
        setAccounts(loaded);
        setPhase("ready");
      })
      .catch(() => {
        if (!active) return;
        setPhase("offline");
        setNotice({ text: copy.notices.loadFailed, tone: "warning" });
      });
    return () => {
      active = false;
    };
  }, [bridge]);

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
    let active = true;
    const unlisten = listen<UpdateInfo>("app:update-available", (event) => {
      if (!active) return;
      setUpdateState("available");
      setPendingUpdate(event.payload);
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

  const handleRefresh = async (accountId?: string) => {
    setRefreshingId(accountId ?? "all");
    setNotice(null);
    try {
      const updated = await bridge.refresh(accountId);
      setAccounts((current) => (accountId ? mergeAccounts(current, updated) : updated));
      setPhase("ready");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setAccounts((current) =>
        current.map((account) =>
          accountId && account.id !== accountId
            ? account
            : {
                ...account,
                status: "error",
                errorMessage: message || copy.notices.refreshFailedOne,
              },
        ),
      );
      setNotice({ text: message || copy.notices.refreshFailed, tone: "warning" });
    } finally {
      setRefreshingId(null);
    }
  };

  const handleCreate = async (
    input: Omit<GenericOpenAiAccountInput, "apiKey"> & { apiKeyInput: HTMLInputElement },
  ) => {
    if (!input.label || !input.baseUrl) {
      input.apiKeyInput.value = "";
      setFormError(copy.form.missingAccount);
      return;
    }
    const secret = input.apiKeyInput.value;
    if (!secret.trim()) {
      setFormError(copy.form.missingKey);
      return;
    }
    setFormSubmitting(true);
    setFormError(null);
    try {
      const account = await bridge.createAccount({
        providerId: input.providerId,
        label: input.label,
        baseUrl: input.baseUrl,
        quotaEndpoint: input.quotaEndpoint,
        usageEndpoint: input.usageEndpoint,
        balanceEndpoint: input.balanceEndpoint,
        apiKey: secret,
      });
      setAccounts((current) => [...current.filter((item) => item.id !== account.id), account]);
      setPhase("ready");
      setNotice(null);
    } catch {
      setFormError(copy.form.failed);
    } finally {
      input.apiKeyInput.value = "";
      setFormSubmitting(false);
    }
  };

  const handleDelete = async (accountId: string) => {
    setDeletingId(accountId);
    try {
      await bridge.deleteAccount(accountId);
      setAccounts((current) => current.filter((account) => account.id !== accountId));
      if (selectedId === accountId) goView("overview");
    } catch {
      setNotice({ text: copy.notices.deleteFailed, tone: "danger" });
    } finally {
      setDeletingId(null);
    }
  };

  const handleUpdateCheck = async () => {
    setUpdateState("checking");
    try {
      const update = await bridge.checkForUpdate();
      if (!update) {
        setUpdateState("current");
        setPendingUpdate(null);
        setNotice({ text: copy.update.current, tone: "accent" });
        return;
      }
      setUpdateState("available");
      setPendingUpdate(update);
      setNotice({ text: copy.update.available(update.version), tone: "accent" });
    } catch {
      setUpdateState("idle");
      setNotice({ text: copy.update.unavailable, tone: "warning" });
    }
  };

  const handleInstallUpdate = async () => {
    try {
      await bridge.installUpdate();
      setNotice({ text: copy.update.installed, tone: "accent" });
    } catch {
      setUpdateState("idle");
      setNotice({ text: copy.update.unavailable, tone: "warning" });
    }
  };

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
    <div className="window morph-enter morph-enter-active">
      <header className="titlebar" data-tauri-drag-region>
        <TrafficLights />
        <div className="titlebar__brand">
          <span className="titlebar__owl" aria-hidden="true">
            <IconOwl size={19} />
          </span>
          <span className="titlebar__name">{copy.brand}</span>
          <h1 className="titlebar__title">{copy.appTitle}</h1>
        </div>
        <div className="titlebar__spacer" data-tauri-drag-region />
        <div className="titlebar__actions">
          <StatusChip status={overallStatus} />
          <button
            className="pill"
            type="button"
            onClick={() => void shell.showBubble().catch(() => null)}
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
              <span className="rail__hint" aria-hidden="true">
                +{accounts.length - RAIL_MAX}
              </span>
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

        <AnimatePresence initial={false}>
          {pendingUpdate && updateState === "available" && (
            <motion.div
              key="update"
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
              <span>{copy.update.available(pendingUpdate.version)}</span>
              <span className="notice__actions">
                <button className="btn btn--accent-outline" type="button" onClick={() => void handleInstallUpdate()}>
                  {copy.actions.installAndRestart}
                </button>
                <button
                  className="btn btn--quiet"
                  type="button"
                  onClick={() => {
                    setPendingUpdate(null);
                    setNotice({ text: copy.update.deferred(pendingUpdate.version) });
                  }}
                >
                  {copy.actions.later}
                </button>
              </span>
            </motion.div>
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
                <div className="section__head">
                  <div className="section__titlewrap">
                    <h2 className="section__title">{copy.rail.allProviders}</h2>
                    {accounts.length > 0 && <span className="section__count num">{accounts.length}</span>}
                  </div>
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
                    {copy.filter.attention}：{copy.summary.none}
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
                          onRefresh={() => void handleRefresh(account.id)}
                          onDelete={() => void handleDelete(account.id)}
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
    </div>
  );
}
function mergeAccounts(current: AccountViewModel[], updates: AccountViewModel[]): AccountViewModel[] {
  const byId = new Map(current.map((account) => [account.id, account]));
  updates.forEach((account) => byId.set(account.id, account));
  return Array.from(byId.values());
}

export default App;
