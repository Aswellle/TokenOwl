import { useEffect, useState } from "react";
import { copy } from "../../../styles/copy";
import type { DashboardBridge } from "../../../types";

/**
 * Management view for the LAN targets the user has explicitly trusted.
 * Trust skips the per-creation confirmation for that exact URL; removing a
 * target restores the prompt without touching any account.
 */
export function LanTrustSection({ bridge }: { bridge: DashboardBridge }) {
  const [targets, setTargets] = useState<string[]>([]);
  const [removing, setRemoving] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let active = true;
    bridge
      .lanTrustList()
      .then((list) => {
        if (active) setTargets(list);
      })
      .catch(() => null);
    return () => {
      active = false;
    };
  }, [bridge]);

  const remove = async (target: string) => {
    setRemoving(target);
    setFailed(false);
    try {
      await bridge.lanTrustRemove(target);
      setTargets((current) => current.filter((entry) => entry !== target));
    } catch {
      setFailed(true);
    } finally {
      setRemoving(null);
    }
  };

  return (
    <section className="section lan-trust" aria-label={copy.lanTrust.title}>
      <div className="section__head">
        <div className="section__titlewrap">
          <h2 className="section__title">{copy.lanTrust.title}</h2>
        </div>
      </div>
      {targets.length === 0 ? (
        <p className="lan-trust__empty">{copy.lanTrust.empty}</p>
      ) : (
        <ul className="lan-trust__list">
          {targets.map((target) => (
            <li className="lan-trust__row" key={target}>
              <code className="lan-trust__url">{target}</code>
              <button
                className="btn btn--quiet"
                type="button"
                disabled={removing !== null}
                onClick={() => void remove(target)}
                aria-label={`${copy.lanTrust.remove} ${target}`}
              >
                {removing === target ? "…" : copy.lanTrust.remove}
              </button>
            </li>
          ))}
        </ul>
      )}
      <span className="field__help">{copy.lanTrust.help}</span>
      {failed && (
        <p className="form__error" role="alert">
          {copy.lanTrust.removeFailed}
        </p>
      )}
    </section>
  );
}
