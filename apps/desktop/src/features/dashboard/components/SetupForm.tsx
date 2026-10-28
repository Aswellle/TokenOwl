import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { motion } from "motion/react";
import { AlertDialog } from "../../../components/AlertDialog";
import { copy } from "../../../styles/copy";
import type {
  DashboardBridge,
  GenericOpenAiAccountInput,
  ManifestProfileViewModel,
} from "../../../types";
import { IconKey } from "../../../components/icons";
import { ProviderIcon } from "../../../components/ProviderIcon";

// Entry animation shared with the dashboard panels it replaces.
const springPanel = { type: "spring", bounce: 0, duration: 0.42 } as const;
const fadeVariants = {
  hidden: { opacity: 0, y: -6 },
  show: { opacity: 1, y: 0, transition: springPanel },
  hide: { opacity: 0, y: -4, transition: { duration: 0.16, ease: [0.23, 1, 0.32, 1] as const } },
};

export function SetupForm({
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
  const [packSkipped, setPackSkipped] = useState<string[]>([]);
  const [profileId, setProfileId] = useState("custom");
  const [label, setLabel] = useState("");
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  // A custom target speaks one of the three supported wire formats; the
  // format decides the default host, whether probe endpoints apply, and
  // which native adapter (if any) serves the account.
  const [customFormat, setCustomFormat] = useState<"openai_compatible" | "anthropic" | "gemini">(
    "openai_compatible",
  );
  // Which brand's variant menu is open (brand key), if any.
  const [expandedBrand, setExpandedBrand] = useState<string | null>(null);
  const [quotaEndpoint, setQuotaEndpoint] = useState("");
  const [usageEndpoint, setUsageEndpoint] = useState("");
  const [balanceEndpoint, setBalanceEndpoint] = useState("");
  const apiKeyRef = useRef<HTMLInputElement>(null);
  // The ⋯ trigger of each expanded brand, so Escape can hand focus back.
  const moreRefs = useRef(new Map<string, HTMLButtonElement>());

  useEffect(() => {
    let active = true;
    bridge
      .manifestProfiles()
      .then((view) => {
        if (!active) return;
        setProfiles(view.profiles);
        setPackSkipped(view.skipped);
      })
      .catch(() => null);
    return () => {
      active = false;
    };
  }, [bridge]);

  const profile = profiles.find((item) => item.id === profileId) ?? null;
  const isCustom = profile === null;
  // A gateway profile (`customBaseUrl`) carries only a placeholder host in its
  // manifest, so the form's own state is the value in every case except a
  // fixed-URL built-in — where the manifest is authoritative.
  const isGateway = profile?.customBaseUrl === true;
  const effectiveBaseUrl = profile && !isGateway ? profile.baseUrl : baseUrl;

  /// Switching profiles must reset the URL field: the custom path keeps its
  /// prefilled default, while a gateway starts empty so the placeholder — not
  /// a real-looking host — is what the user sees.
  const selectProfile = (nextId: string) => {
    const next = profiles.find((item) => item.id === nextId) ?? null;
    setProfileId(nextId);
    if (next?.customBaseUrl) setBaseUrl("");
  };

  const CUSTOM_DEFAULT_BASE_URL = {
    openai_compatible: "https://api.openai.com/v1",
    anthropic: "https://api.anthropic.com/v1",
    gemini: "https://generativelanguage.googleapis.com/v1beta",
  } as const;

  const selectCustomFormat = (format: "openai_compatible" | "anthropic" | "gemini") => {
    setCustomFormat(format);
    setBaseUrl(CUSTOM_DEFAULT_BASE_URL[format]);
  };

  // A brand's variant list behaves like a popover: any click or focus move
  // outside its group dismisses it, so it never lingers over the form.
  useEffect(() => {
    if (!expandedBrand) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Element && target.closest(`[data-brand="${expandedBrand}"]`)) return;
      setExpandedBrand(null);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [expandedBrand]);

  // Platform variants of one brand (智谱 国内版/国际版, Anthropic 原生/OpenAI
  // 兼容) collapse into a single capsule keyed by the brand; the capsule
  // lives in the section of its default variant's wire format, and its
  // variant list carries the rest. Groups keep manifest order.
  const brandGroups = useMemo(() => {
    const groups = new Map<string, ManifestProfileViewModel[]>();
    for (const item of profiles) {
      const key = item.brand ?? item.id;
      const members = groups.get(key);
      if (members) members.push(item);
      else groups.set(key, [item]);
    }
    return Array.from(groups.values()).map((members) => {
      const defaultMember = members.find((member) => !member.variant) ?? members[0];
      const activeMember = members.find((member) => member.id === profileId) ?? defaultMember;
      return {
        key: defaultMember.brand ?? defaultMember.id,
        members,
        defaultMember,
        activeMember,
        format: defaultMember.apiFormat,
      };
    });
  }, [profiles, profileId]);

  const formatLabel = (apiFormat: string) =>
    apiFormat === "anthropic"
      ? copy.form.formatAnthropic
      : apiFormat === "google-ai"
        ? copy.form.formatGemini
        : copy.form.formatOpenai;
  const endpointChips = profile
    ? Object.entries(profile.endpoints)
        .filter(([, path]) => path !== null && path !== undefined)
        .map(([kind]) => copy.form.endpointLabels[kind] ?? kind)
    : [];

  // LAN consent (docs/06 + docs/03 §12): a submit against a private address
  // asks Rust for a one-time token bound to that exact URL, and only the
  // dialog's confirm button ever creates the account — a repeat submit while
  // the dialog is open does nothing. Classification failures fail closed:
  // an unclassifiable target is never treated as public.
  const [lanConsent, setLanConsent] = useState<{ token: string } | null>(null);
  const [consentBusy, setConsentBusy] = useState(false);
  const [classifyError, setClassifyError] = useState<string | null>(null);
  const submitTriggerRef = useRef<HTMLButtonElement>(null);

  const sendAccount = async (lanConsentToken?: string) => {
    const apiKeyInput = apiKeyRef.current;
    if (!apiKeyInput || !apiKeyInput.value.trim()) return;
    await onSubmit({
      providerId: profile ? profile.id : "generic.openai_compatible",
      label: label.trim(),
      baseUrl: effectiveBaseUrl.trim(),
      quotaEndpoint: profile || customFormat !== "openai_compatible" ? undefined : quotaEndpoint.trim() || undefined,
      usageEndpoint: profile || customFormat !== "openai_compatible" ? undefined : usageEndpoint.trim() || undefined,
      balanceEndpoint: profile || customFormat !== "openai_compatible" ? undefined : balanceEndpoint.trim() || undefined,
      apiFormat: profile ? undefined : customFormat,
      lanConsentToken,
      apiKeyInput,
    });
  };

  const submit = async (event: { preventDefault(): void }) => {
    event.preventDefault();
    // The dialog is the only confirmation path: while it is open (or a
    // classification is in flight) form submits are inert.
    if (lanConsent || consentBusy) return;
    setClassifyError(null);
    setConsentBusy(true);
    let token: string | null;
    try {
      token = await bridge.lanConsentBegin(effectiveBaseUrl.trim());
    } catch {
      // Fail closed: show a recoverable error instead of guessing "public".
      setClassifyError(copy.form.lanClassifyFailed);
      setConsentBusy(false);
      return;
    }
    setConsentBusy(false);
    if (token === null) {
      await sendAccount();
      return;
    }
    setLanConsent({ token });
  };

  return (
    <motion.form
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
      {classifyError && (
        <p className="form__error" role="alert">
          {classifyError}
        </p>
      )}
      {packSkipped.length > 0 && (
        <p className="form__error" role="status">
          {copy.form.communitySkipped}: {packSkipped.join("；")}
        </p>
      )}
      {lanConsent && (
        <AlertDialog
          className="lan-consent"
          title={copy.form.lanTitle}
          body={copy.form.lanBody}
          confirmLabel={copy.form.lanConfirm}
          cancelLabel={copy.actions.cancel}
          returnFocusTo={submitTriggerRef}
          onCancel={() => setLanConsent(null)}
          onConfirm={() => {
            const token = lanConsent.token;
            setLanConsent(null);
            void sendAccount(token);
          }}
        />
      )}
      <div className="picker" role="group" aria-label={copy.a11y.profilePicker}>
        {(
          [
            ["openai-compatible", copy.form.protocolOpenai],
            ["anthropic", copy.form.protocolAnthropic],
            ["google-ai", copy.form.protocolGemini],
          ] as const
        ).map(([format, heading]) => {
          const groups = brandGroups.filter((group) => group.format === format);
          if (groups.length === 0) return null;
          return (
            <Fragment key={format}>
              <span className="picker__heading" aria-hidden="true">
                {heading}
              </span>
              <div className="picker__row">
                {groups.map((group) => {
                  const active = group.members.some((member) => member.id === profileId);
                  return (
                    <div
                      className="picker__group"
                      key={group.key}
                      data-brand={group.key}
                      onBlur={(event) => {
                        if (!event.currentTarget.contains(event.relatedTarget)) {
                          setExpandedBrand(null);
                        }
                      }}
                      onKeyDown={(event) => {
                        if (event.key === "Escape" && expandedBrand === group.key) {
                          event.stopPropagation();
                          setExpandedBrand(null);
                          moreRefs.current.get(group.key)?.focus();
                        }
                      }}
                    >
                      <button
                        type="button"
                        className={`picker__btn${active ? " is-active" : ""}${group.members.length > 1 ? " has-variants" : ""}`}
                        aria-pressed={active}
                        onClick={() => selectProfile(group.activeMember.id)}
                      >
                        <ProviderIcon providerId={group.key} size={18} />
                        <span>{group.activeMember.name}</span>
                      </button>
                      {group.members.length > 1 && (
                        <button
                          type="button"
                          className="picker__more"
                          aria-expanded={expandedBrand === group.key}
                          aria-controls={`picker-menu-${group.key}`}
                          aria-label={copy.form.variantsOf(group.activeMember.name)}
                          title={copy.form.variantsOf(group.activeMember.name)}
                          ref={(node) => {
                            if (node) moreRefs.current.set(group.key, node);
                            else moreRefs.current.delete(group.key);
                          }}
                          onClick={() =>
                            setExpandedBrand(expandedBrand === group.key ? null : group.key)
                          }
                        >
                          ⋯
                        </button>
                      )}
                      {group.members.length > 1 && expandedBrand === group.key && (
                        <div className="picker__menu" id={`picker-menu-${group.key}`}>
                          {group.members.map((member) => (
                            <button
                              key={member.id}
                              type="button"
                              className={`picker__option${member.id === profileId ? " is-active" : ""}`}
                              onClick={() => {
                                selectProfile(member.id);
                                setExpandedBrand(null);
                              }}
                            >
                              <span>{member.variant ?? member.name}</span>
                              <span className="picker__option-meta">{formatLabel(member.apiFormat)}</span>
                            </button>
                          ))}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </Fragment>
          );
        })}
        <span className="picker__heading" aria-hidden="true">
          {copy.form.customSection}
        </span>
        <div className="picker__row">
          <button
            type="button"
            className={`picker__btn${isCustom ? " is-active" : ""}`}
            aria-pressed={isCustom}
            onClick={() => selectProfile("custom")}
          >
            <ProviderIcon
              providerId={isCustom ? customFormat : "generic.openai_compatible"}
              size={18}
            />
            <span>{copy.form.customOption}</span>
          </button>
        </div>
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
            {isGateway ? copy.form.gatewayBaseUrl : copy.form.baseUrl}
          </label>
          <input
            className="field__input mono"
            id="base-url"
            type="url"
            value={effectiveBaseUrl}
            onChange={(event) => setBaseUrl(event.currentTarget.value)}
            readOnly={!isCustom && !isGateway}
            placeholder={
              isGateway ? copy.form.gatewayBaseUrlPlaceholder : copy.form.baseUrlPlaceholder
            }
            autoComplete="off"
            spellCheck={false}
            required
          />
          <span className="field__help">
            {isCustom
              ? copy.form.baseUrlHelp
              : isGateway
                ? copy.form.gatewayBaseUrlHelp
                : copy.form.builtinNote}
          </span>
        </div>
        {isCustom && (
          <>
            <div className="field field--wide" role="radiogroup" aria-label={copy.form.customFormatLabel}>
              <span className="field__label" id="custom-format-label">
                {copy.form.customFormatLabel}
              </span>
              <div className="picker__row">
                {(
                  [
                    ["openai_compatible", copy.form.formatOpenai],
                    ["anthropic", copy.form.formatAnthropic],
                    ["gemini", copy.form.formatGemini],
                  ] as const
                ).map(([value, label]) => (
                  <button
                    key={value}
                    type="button"
                    role="radio"
                    aria-checked={customFormat === value}
                    className={`picker__btn${customFormat === value ? " is-active" : ""}`}
                    onClick={() => selectCustomFormat(value)}
                  >
                    <span>{label}</span>
                  </button>
                ))}
              </div>
            </div>
        </>
        )}
        {isCustom && customFormat !== "openai_compatible" && (
          <div className="field field--wide">
            <span className="field__help">{copy.form.nativeFormatHelp}</span>
          </div>
        )}
        {isCustom && customFormat === "openai_compatible" && (
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
        <button
          className="btn btn--primary"
          type="submit"
          disabled={submitting || consentBusy}
          ref={submitTriggerRef}
        >
          {submitting ? copy.actions.submitting : copy.actions.submit}
        </button>
      </div>
    </motion.form>
  );
}
