import { copy } from "../../../styles/copy";
import { IconCheck, IconAlert, IconRefresh, IconCloudOff, IconClock } from "../../../components/icons";

const statusIcons: Record<string, typeof IconCheck> = {
  healthy: IconCheck,
  attention: IconAlert,
  critical: IconAlert,
  stale: IconClock,
  error: IconAlert,
  syncing: IconRefresh,
  unavailable: IconCloudOff,
  offline: IconCloudOff,
};

export function StatusChip({ status }: { status: string }) {
  const Icon = statusIcons[status] ?? IconCloudOff;
  return (
    <span className={`chip chip--${status}`}>
      <span className="chip__icon">
        <Icon size={13} />
      </span>
      {copy.health[status] ?? status}
    </span>
  );
}
