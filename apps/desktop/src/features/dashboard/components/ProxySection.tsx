import { useEffect, useState } from "react";
import { copy } from "../../../styles/copy";
import type { DashboardBridge } from "../../../types";

/**
 * The explicit proxy setting. The transport deliberately ignores environment
 * proxies (deterministic behavior, docs/06), so users behind a corporate
 * gateway opt in here instead; the value lives in the OS credential store
 * and applies after a restart, when the adapters rebuild their client.
 */
export function ProxySection({ bridge }: { bridge: DashboardBridge }) {
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    let active = true;
    bridge
      .getProxy()
      .then((stored) => {
        if (active) setUrl(stored ?? "");
      })
      .catch(() => null);
    return () => {
      active = false;
    };
  }, [bridge]);

  const save = async () => {
    setBusy(true);
    setStatus(null);
    const trimmed = url.trim();
    try {
      await bridge.setProxy(trimmed === "" ? null : trimmed);
      setStatus({
        ok: true,
        text: trimmed === "" ? copy.proxy.cleared : copy.proxy.saved,
      });
    } catch {
      setStatus({ ok: false, text: copy.proxy.invalid });
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="section proxy" aria-label={copy.proxy.title}>
      <div className="section__head">
        <div className="section__titlewrap">
          <h2 className="section__title">{copy.proxy.title}</h2>
        </div>
      </div>
      <div className="field field--wide">
        <label className="field__label" htmlFor="proxy-url">
          {copy.proxy.title}
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
        <span className="field__help">{copy.proxy.restartNote}</span>
        <div className="proxy__actions">
          <button
            className="btn btn--quiet"
            type="button"
            onClick={() => void save()}
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
