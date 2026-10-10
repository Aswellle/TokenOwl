import { copy } from "../../../styles/copy";
import { healthIcons } from "../../../components/icons";

export function StatusChip({ status }: { status: string }) {
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

