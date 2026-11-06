import { copy } from "../../../styles/copy";
import { healthIcons } from "../../../components/icons";
import type { AccountHealth } from "../../../types";

export function StatusChip({ status }: { status: AccountHealth }) {
  const Icon = healthIcons[status] ?? healthIcons.unavailable ?? (() => null);
  return (
    <span className={`chip chip--${status}`}>
      <span className="chip__icon">
        <Icon size={13} />
      </span>
      {copy.health[status] ?? status}
    </span>
  );
}

