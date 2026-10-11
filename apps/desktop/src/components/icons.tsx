/**
 * Inline SVG icons on a shared 24px grid.
 *
 * Kept in-repo instead of a dependency so the Tauri CSP stays restrictive and
 * stroke weight, size and alignment remain consistent. Geometry follows the
 * lucide-style outline language (round caps/joins, 1.6 stroke) so glyphs read
 * at 13–26px next to CJK text.
 */

import type { ReactElement } from "react";

type IconProps = {
  size?: number;
  className?: string;
  strokeWidth?: number;
};

const base = (size: number, strokeWidth = 1.6) => ({
  width: size,
  height: size,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
  focusable: false,
});

/** TokenOwl brand mark: a round owl with tufts, ringed eyes and a small beak. */
export function IconOwl({ size = 18, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M12 3.9c4.55 0 7.75 3.3 7.75 8.25 0 4.75-3.2 7.95-7.75 7.95S4.25 16.9 4.25 12.15C4.25 7.2 7.45 3.9 12 3.9Z" />
      <path d="m6.55 6.05-1.8-2.9 3.7 1.05" />
      <path d="m17.45 6.05 1.8-2.9-3.7 1.05" />
      <circle cx="8.9" cy="12" r="2.85" />
      <circle cx="15.1" cy="12" r="2.85" />
      <circle cx="8.9" cy="12" r="1.1" fill="currentColor" stroke="none" />
      <circle cx="15.1" cy="12" r="1.1" fill="currentColor" stroke="none" />
      <path d="M11.1 15.7c.3.85.6 1.3.9 1.3s.6-.45.9-1.3" />
    </svg>
  );
}

/** Floating companion bubble: the collapse-to-bubble affordance. */
export function IconBubble({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <circle cx="10.6" cy="10.6" r="6.85" />
      <circle cx="17.85" cy="17.85" r="2.9" />
    </svg>
  );
}

/** Quota pressure gauge. */
export function IconGauge({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="m12 14 3.6-3.6" />
      <path d="M4.1 18.6a9.2 9.2 0 1 1 15.8 0" />
    </svg>
  );
}

/** Sync activity pulse. */
export function IconActivity({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M21.5 12h-3.8l-2.9 8.2-6.2-16.4-2.6 8.2H2.5" />
    </svg>
  );
}

export function IconRefresh({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M20 11a8 8 0 1 0-2.3 5.6" />
      <path d="M20 5v6h-6" />
    </svg>
  );
}

export function IconPlus({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

export function IconClose({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M6 6l12 12M18 6L6 18" />
    </svg>
  );
}

export function IconMinus({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M5 12h14" />
    </svg>
  );
}

export function IconCheck({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M20 6L9 17l-5-5" />
    </svg>
  );
}

export function IconAlert({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M12 8v5" />
      <path d="M12 16.5h.01" />
      <path d="M10.3 3.9 2.7 17a2 2 0 0 0 1.7 3h15.2a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z" />
    </svg>
  );
}

export function IconClock({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7.5V12l3 2" />
    </svg>
  );
}

export function IconTrash({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M4 7h16" />
      <path d="M10 11v6M14 11v6" />
      <path d="M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12" />
      <path d="M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2" />
    </svg>
  );
}

export function IconDownload({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M12 4v10" />
      <path d="M8 11l4 4 4-4" />
      <path d="M5 19h14" />
    </svg>
  );
}

export function IconCloudOff({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M6.5 18a4.5 4.5 0 0 1-.6-8.96A5.5 5.5 0 0 1 16 7.5" />
      <path d="M8 18h9a4 4 0 0 0 3.2-6.4" />
      <path d="M4 4l16 16" />
    </svg>
  );
}

export function IconKey({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <circle cx="8.5" cy="14.5" r="3.5" />
      <path d="M11 12l7.5-7.5M16 5l2 2M14 9l2 2" />
    </svg>
  );
}

export function IconShield({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M12 3l7 3v6c0 4.2-2.9 7.6-7 9-4.1-1.4-7-4.8-7-9V6l7-3Z" />
      <path d="M9.5 12.5l1.8 1.8 3.4-3.6" />
    </svg>
  );
}

export function IconChevronRight({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M9 6l6 6-6 6" />
    </svg>
  );
}

/** Account avatar glyph. */
export function IconCircleUser({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="9.8" r="2.9" />
      <path d="M6.6 18.7a6.6 6.6 0 0 1 10.8 0" />
    </svg>
  );
}

export type HealthIcon = (props: IconProps) => ReactElement;

/** Status glyph per health state: shape carries meaning, color only reinforces it. */
export const healthIcons: Record<string, HealthIcon> = {
  healthy: IconCheck,
  attention: IconAlert,
  critical: IconAlert,
  stale: IconClock,
  error: IconAlert,
  offline: IconCloudOff,
  syncing: IconRefresh,
  unavailable: IconClock,
};

/** Settings: a gear, for the dock's settings entry. */
export function IconGear({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <circle cx="12" cy="12" r="3.2" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.11-1.56 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.56-1.11 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34h.08a1.7 1.7 0 0 0 1.03-1.56V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87v.08a1.7 1.7 0 0 0 1.56 1.03H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.51 1.03Z" />
    </svg>
  );
}

/** External link: box with an arrow leaving it, for the GitHub dock entry. */
export function IconExternal({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <path d="M15 3h6v6" />
      <path d="M10 14 21 3" />
      <path d="M18 13.5V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h5.5" />
    </svg>
  );
}

/** Layout grid: the "all providers" overview affordance. */
export function IconGrid({ size = 16, className, strokeWidth }: IconProps) {
  return (
    <svg {...base(size, strokeWidth)} className={className}>
      <rect x="3.5" y="3.5" width="7" height="7" rx="1.6" />
      <rect x="13.5" y="3.5" width="7" height="7" rx="1.6" />
      <rect x="3.5" y="13.5" width="7" height="7" rx="1.6" />
      <rect x="13.5" y="13.5" width="7" height="7" rx="1.6" />
    </svg>
  );
}

/** GitHub mark: filled octocat glyph for the dock's project-page entry. */
export function IconGitHub({ size = 16, className }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="currentColor"
      aria-hidden
      focusable={false}
      className={className}
    >
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z" />
    </svg>
  );
}
