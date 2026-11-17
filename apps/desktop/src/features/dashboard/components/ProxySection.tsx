import { useEffect, useRef, useState } from "react";
import { bridgeErrorCode } from "../../../lib/tauri";
import { copy } from "../../../styles/copy";
import type { DashboardBridge, ProxyRoute, ProxyRouting } from "../../../types";

const ROUTES: { value: ProxyRoute; label: string }[] = [
  { value: "direct", label: copy.proxy.routeDirect },
  { value: "system", label: copy.proxy.routeSystem },
  { value: "custom", label: copy.proxy.routeCustom },
];

/**
 * Proxy transport settings, split by provider region.
 *
 * Domestic and international services usually need opposite treatment — a
 * direct connection at home, the system proxy abroad — so the policy is one
 * route per class instead of a single global switch. The backend reads the
 * policy per request, which is why a change applies immediately and no copy
 * talks about restarting.
 */
export function ProxySection({ bridge }: { bridge: DashboardBridge }) {
  const [url, setUrl] = useState("");
  const [routing, setRouting] = useState<ProxyRouting | null>(null);
  const [busy, setBusy] = useState(false);
  const [routingBusy, setRoutingBusy] = useState(false);
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);
  const [routingError, setRoutingError] = useState<string | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    // The URL and the routing policy degrade independently: a failed URL read
    // must not hide the routing controls, and vice versa.
    bridge
      .getProxy()
      .then((stored) => {
        if (mounted.current) setUrl(stored ?? "");
      })
      .catch(() => null);
    // A bridge from an older host may not expose routing yet; the safe
    // default keeps the panel usable instead of crashing the dashboard.
    const readRouting = bridge.getProxyRouting?.bind(bridge);
    if (readRouting) {
      readRouting()
        .then((value) => {
          if (mounted.current) setRouting(value);
        })
        .catch(() => {
          if (mounted.current) setRouting({ domestic: "direct", international: "system" });
        });
    } else {
      setRouting({ domestic: "direct", international: "system" });
    }
    return () => {
      mounted.current = false;
    };
  }, [bridge]);

  const hasCustomUrl = url.trim().length > 0;

  const saveUrl = async () => {
    setBusy(true);
    setStatus(null);
    const trimmed = url.trim();
    try {
      await bridge.setProxy(trimmed === "" ? null : trimmed);
      if (!mounted.current) return;
      setStatus({
        ok: true,
        text: trimmed === "" ? copy.proxy.cleared : copy.proxy.saved,
      });
      if (trimmed === "") {
        // Clearing the URL makes the custom route unusable; the backend
        // falls back to direct, so mirror that here.
        setRouting((current) =>
          current === null
            ? current
            : {
                domestic: current.domestic === "custom" ? "direct" : current.domestic,
                international:
                  current.international === "custom" ? "direct" : current.international,
              },
        );
      }
    } catch (error) {
      if (!mounted.current) return;
      // Two different failures live behind this button: a rejected URL (the
      // backend's InvalidInput, with the fix named in the canned copy) and a
      // credential-store outage — blaming the address for the latter would
      // send the user editing a URL that is already fine.
      const code = bridgeErrorCode(error);
      setStatus({
        ok: false,
        text: code && code !== "InvalidInput" ? copy.proxy.storeFailed : copy.proxy.invalid,
      });
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  const changeRoute = async (
    kind: "domestic" | "international",
    value: ProxyRoute,
  ) => {
    if (routing === null || routingBusy || routing[kind] === value) return;
    if (!bridge.setProxyRouting) return;
    const previous = routing;
    const next: ProxyRouting = { ...routing, [kind]: value };
    setRouting(next);
    setRoutingBusy(true);
    setRoutingError(null);
    try {
      const saved = await bridge.setProxyRouting(next);
      if (mounted.current) setRouting(saved);
    } catch {
      if (!mounted.current) return;
      setRouting(previous);
      setRoutingError(copy.proxy.routingFailed);
    } finally {
      if (mounted.current) setRoutingBusy(false);
    }
  };

  const routeRow = (
    kind: "domestic" | "international",
    label: string,
    help: string,
    fieldId: string,
  ) => (
    <div className="setting-row">
      <div className="setting-row__text">
        <label className="setting-row__label" htmlFor={fieldId}>
          {label}
        </label>
        <small>{help}</small>
      </div>
      <span className="setting-row__control">
        <select
          className="field__input"
          id={fieldId}
          value={routing?.[kind] ?? "direct"}
          disabled={routing === null || routingBusy}
          aria-label={label}
          onChange={(event) => void changeRoute(kind, event.currentTarget.value as ProxyRoute)}
        >
          {ROUTES.map((route) => (
            <option
              key={route.value}
              value={route.value}
              disabled={route.value === "custom" && !hasCustomUrl}
            >
              {route.label}
            </option>
          ))}
        </select>
      </span>
    </div>
  );

  return (
    <section className="section settings-card proxy" aria-labelledby="proxy-title">
      <div className="settings-card__head">
        <h2 className="settings-card__title" id="proxy-title">
          {copy.proxy.title}
        </h2>
        <p className="settings-card__intro">{copy.proxy.intro}</p>
      </div>

      <div className="settings-group">
        <h3 className="settings-group__title">{copy.proxy.groupRouting}</h3>
        <p className="settings-group__intro">{copy.proxy.groupRoutingIntro}</p>
        {routeRow(
          "domestic",
          copy.proxy.domesticRoute,
          copy.proxy.domesticHelp,
          "proxy-route-domestic",
        )}
        {routeRow(
          "international",
          copy.proxy.internationalRoute,
          copy.proxy.internationalHelp,
          "proxy-route-international",
        )}
        {routingError && (
          <p className="form__error" role="alert">
            {routingError}
          </p>
        )}
        <p className="history__note">{copy.proxy.systemNote}</p>
      </div>

      <div className="settings-group">
        <h3 className="settings-group__title">{copy.proxy.groupCustom}</h3>
        <p className="settings-group__intro">{copy.proxy.groupCustomIntro}</p>
        <div className="field">
          <label className="field__label" htmlFor="proxy-url">
            {copy.proxy.proxyLabel}
          </label>
          <input
            className="field__input mono"
            id="proxy-url"
            type="text"
            value={url}
            onChange={(event) => setUrl(event.currentTarget.value)}
            placeholder={copy.proxy.placeholder}
            autoComplete="off"
            spellCheck={false}
          />
          <span className="field__help">{copy.proxy.help}</span>
          <span className="field__help">{copy.proxy.maskedNote}</span>
        </div>
        <div className="proxy__actions">
          <button
            className="btn btn--outline"
            type="button"
            onClick={() => void saveUrl()}
            disabled={busy}
          >
            {busy ? copy.proxy.saving : copy.proxy.save}
          </button>
        </div>
        {status && (
          <p className={status.ok ? "form__success" : "form__error"} role="status">
            {status.text}
          </p>
        )}
      </div>
    </section>
  );
}
