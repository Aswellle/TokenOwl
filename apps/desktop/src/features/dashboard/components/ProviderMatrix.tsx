import { useEffect, useState } from "react";
import { copy } from "../../../styles/copy";
import type { DashboardBridge, ProviderMatrixRowViewModel } from "../../../types";

const LEVEL_CLASS: Record<string, string> = {
  L4: "cap-chip--l4",
  L3: "cap-chip--l3",
  L2: "cap-chip--l2",
  L1: "cap-chip--l1",
};

function levelLabel(level: string): string {
  return copy.matrix.level[level] ?? level;
}

/**
 * The docs/09 capability catalog: implemented providers with their measured
 * level from the embedded manifest, planned providers honestly untagged.
 */
export function ProviderMatrix({ bridge }: { bridge: DashboardBridge }) {
  const [rows, setRows] = useState<ProviderMatrixRowViewModel[] | null>(null);

  useEffect(() => {
    let active = true;
    bridge
      .providerMatrix()
      .then((value) => {
        if (active) setRows(value);
      })
      .catch(() => {
        if (active) setRows([]);
      });
    return () => {
      active = false;
    };
  }, [bridge]);

  return (
    <section className="section matrix" aria-label={copy.a11y.matrixRegion}>
      <div className="section__head">
        <div className="section__titlewrap">
          <h2 className="section__title">{copy.matrix.title}</h2>
        </div>
      </div>
      <p className="matrix__intro">{copy.matrix.intro}</p>
      {rows === null ? (
        <div className="skeleton" aria-hidden="true">
          <div className="skeleton__line skeleton__line--short" />
          <div className="skeleton__line" />
        </div>
      ) : (
        <ul className="matrix__list">
          {rows.map((row) => (
            <li
              key={row.id}
              className={`matrix__row${row.implemented ? "" : " matrix__row--planned"}`}
            >
              <span className="matrix__name">{row.name}</span>
              {row.implemented && row.level ? (
                <span
                  className={`cap-chip ${LEVEL_CLASS[row.level] ?? "cap-chip--l1"}`}
                  title={copy.a11y.capabilityChip(row.level)}
                >
                  {row.level} · {levelLabel(row.level)}
                </span>
              ) : (
                <span className="cap-chip cap-chip--planned">
                  {copy.matrix.planned} · {row.priority}
                </span>
              )}
              <span className="matrix__meta num">
                {[...row.quotaUnits, ...row.targetWindows].join(" · ")}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
